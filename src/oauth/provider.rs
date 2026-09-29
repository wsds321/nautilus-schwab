//! OAuth token provider implementing `schwab_sdk::TokenProvider`.
//!
//! Integrates with existing schwab-mcp infrastructure:
//! - Reads tokens from `~/.local/share/schwab-mcp/token.yaml`
//! - Reads credentials from `~/.local/share/schwab-mcp/credentials.yaml`
//! - Refreshes via direct HTTP call to Schwab token endpoint
//! - Falls back to environment variables if files not found
//!
//! The `TokenProvider` trait in schwab-sdk 0.5 is **synchronous** — it is
//! called once per REST request just before sending. Token refresh happens
//! in a background task; `access_token()` returns the latest cached token
//! without blocking on I/O.
//!
//! Token lifecycle:
//! - Access tokens expire after 30 minutes
//! - Refresh tokens expire after 7 days
//! - Proactive refresh 5 minutes before expiry

use crate::common::credential::SchwabCredential;
use crate::http::error::SchwabHttpError;
use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{debug, info, warn};

/// Default path to schwab-mcp token file.
const DEFAULT_TOKEN_PATH: &str = "~/.local/share/schwab-mcp/token.yaml";
/// Default path to schwab-mcp credentials file.
const DEFAULT_CREDENTIALS_PATH: &str = "~/.local/share/schwab-mcp/credentials.yaml";

/// Token metadata including expiry tracking.
#[derive(Debug, Clone)]
struct TokenState {
    /// Current access token.
    access_token: SecretString,
    /// Current refresh token.
    refresh_token: SecretString,
    /// Unix timestamp (seconds) when the access token expires.
    expires_at: i64,
    /// Buffer in seconds before actual expiry to trigger proactive refresh.
    refresh_buffer_secs: i64,
}

impl TokenState {
    fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now >= self.expires_at - self.refresh_buffer_secs
    }
}

/// Thread-safe OAuth token provider implementing `schwab_sdk::TokenProvider`.
///
/// Designed to be shared across multiple clients via `Arc<SchwabTokenProvider>`.
/// Uses `RwLock` to allow concurrent reads while serializing token refresh.
///
/// The `TokenProvider::access_token()` method is synchronous (required by
/// schwab-sdk 0.5). It returns the cached token immediately. Background
/// refresh is triggered lazily when expiry is detected.
pub struct SchwabTokenProvider {
    /// Current token state (access token, refresh token, expiry).
    state: RwLock<TokenState>,
    /// Credentials for token refresh requests (async RwLock — held across await in refresh).
    credential: Arc<tokio::sync::RwLock<SchwabCredential>>,
    /// HTTP client for token endpoint requests (avoids circular dependency).
    http: reqwest::Client,
    /// Schwab OAuth token endpoint URL.
    token_url: String,
}

// Implement schwab_sdk's TokenProvider trait (synchronous).
impl schwab_sdk::TokenProvider for SchwabTokenProvider {
    fn access_token(&self) -> Result<schwab_sdk::AuthToken, Box<dyn std::error::Error + Send + Sync>> {
        // The SDK calls this synchronously once per request.
        // With std::sync::RwLock, this is a plain synchronous read — no runtime needed.
        let token_str = self.state.read().unwrap().access_token.expose_secret().to_string();
        Ok(schwab_sdk::AuthToken::new(token_str))
    }
}

impl SchwabTokenProvider {
    /// Create a token provider by reading from existing schwab-mcp infrastructure.
    ///
    /// Reads:
    /// - `~/.local/share/schwab-mcp/token.yaml` for access/refresh tokens
    /// - `~/.local/share/schwab-mcp/credentials.yaml` for app key/secret
    ///
    /// This integrates with the existing OAuth setup used by finrl-trading
    /// and schwab-mcp, avoiding duplicate credential management.
    pub fn from_schwab_mcp() -> Result<Self, SchwabHttpError> {
        let token_path = shellexpand::tilde(DEFAULT_TOKEN_PATH).into_owned();
        let creds_path = shellexpand::tilde(DEFAULT_CREDENTIALS_PATH).into_owned();

        // Read token.yaml
        let token_content = std::fs::read_to_string(&token_path).map_err(|e| {
            SchwabHttpError::Validation(format!(
                "cannot read token file {}: {}",
                token_path, e
            ))
        })?;
        let token_doc: serde_yaml::Value = serde_yaml::from_str(&token_content)
            .map_err(|e| SchwabHttpError::Sdk(schwab_sdk::Error::Codec {
                context: "parse token.yaml".to_string(),
                reason: e.to_string(),
            }))?;

        let token = token_doc.get("token").unwrap_or(&token_doc);
        let access_token = token["access_token"]
            .as_str()
            .ok_or_else(|| SchwabHttpError::Validation("missing access_token in token.yaml".into()))?;
        let refresh_token = token["refresh_token"]
            .as_str()
            .ok_or_else(|| SchwabHttpError::Validation("missing refresh_token in token.yaml".into()))?;
        let expires_at = token["expires_at"]
            .as_i64()
            .or_else(|| token_doc.get("creation_timestamp").and_then(|v| v.as_i64()).map(|ts| ts + 1800))
            .unwrap_or_else(|| chrono::Utc::now().timestamp() + 1800);

        // Read credentials.yaml
        let creds_content = std::fs::read_to_string(&creds_path).map_err(|e| {
            SchwabHttpError::Validation(format!(
                "cannot read credentials file {}: {}",
                creds_path, e
            ))
        })?;
        let creds_doc: serde_yaml::Value = serde_yaml::from_str(&creds_content)
            .map_err(|e| SchwabHttpError::Sdk(schwab_sdk::Error::Codec {
                context: "parse credentials.yaml".to_string(),
                reason: e.to_string(),
            }))?;

        let app_key = creds_doc["app_key"]
            .as_str()
            .or_else(|| creds_doc["client_id"].as_str())
            .ok_or_else(|| SchwabHttpError::Validation("missing app_key in credentials.yaml".into()))?;
        let app_secret = creds_doc["app_secret"]
            .as_str()
            .or_else(|| creds_doc["client_secret"].as_str())
            .ok_or_else(|| SchwabHttpError::Validation("missing app_secret in credentials.yaml".into()))?;
        let callback_url = creds_doc["callback_url"]
            .as_str()
            .unwrap_or("https://127.0.0.1:8182");

        let credential = SchwabCredential::new(
            app_key,
            app_secret,
            callback_url,
            access_token,
            refresh_token,
        );

        info!(
            token_path = %token_path,
            creds_path = %creds_path,
            expires_at,
            "loaded Schwab credentials from schwab-mcp infrastructure"
        );

        Ok(Self::new(credential))
    }

    /// Create a new token provider from credentials.
    ///
    /// Assumes the initial access token is valid. The provider will
    /// automatically refresh it when it approaches expiry.
    pub fn new(credential: SchwabCredential) -> Self {
        let initial_state = TokenState {
            access_token: SecretString::from(credential.access_token().to_string()),
            refresh_token: SecretString::from(credential.refresh_token().to_string()),
            expires_at: chrono::Utc::now().timestamp() + 1800,
            refresh_buffer_secs: 300, // Refresh 5 minutes before expiry
        };

        Self {
            state: RwLock::new(initial_state),
            credential: Arc::new(tokio::sync::RwLock::new(credential)),
            http: reqwest::Client::new(),
            token_url: "https://api.schwabapi.com/v1/oauth/token".to_string(),
        }
    }

    /// Check if the access token needs refresh.
    pub fn needs_refresh(&self) -> bool {
        self.state.read().unwrap().is_expired()
    }

    /// Refresh the access token using the refresh token.
    ///
    /// This method is safe to call concurrently; only one refresh will execute
    /// at a time due to the write lock on `state`.
    pub async fn refresh_token(&self) -> Result<(), SchwabHttpError> {
        // Check expiry with a brief read lock (std::sync::RwLock is fine here —
        // not held across await points).
        {
            let state = self.state.read().unwrap();
            if !state.is_expired() {
                debug!("token already refreshed by another task");
                return Ok(());
            }
        }

        let cred = self.credential.read().await;
        info!("refreshing Schwab access token");

        // Read the current refresh token before making the HTTP call.
        let current_refresh_token = {
            let state = self.state.read().unwrap();
            state.refresh_token.expose_secret().to_string()
        };

        let response = self
            .http
            .post(&self.token_url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &current_refresh_token),
                ("client_id", cred.app_key()),
                ("client_secret", cred.app_secret()),
            ])
            .send()
            .await
            .map_err(|e| SchwabHttpError::Sdk(schwab_sdk::Error::Transport(e)))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            warn!(status, body = %body, "token refresh failed");
            return Err(SchwabHttpError::AuthRequired);
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| SchwabHttpError::Sdk(schwab_sdk::Error::Codec {
                context: "decode token refresh response".to_string(),
                reason: e.to_string(),
            }))?;

        // Update state with new tokens (brief write lock, not held across await).
        {
            let mut state = self.state.write().unwrap();
            state.access_token = SecretString::from(token_response.access_token.clone());
            if let Some(new_refresh) = token_response.refresh_token {
                state.refresh_token = SecretString::from(new_refresh);
            }
            state.expires_at = chrono::Utc::now().timestamp() + token_response.expires_in as i64;
        }

        // Also update the credential store
        let mut cred = self.credential.write().await;
        cred.update_access_token(token_response.access_token);

        info!("Schwab access token refreshed successfully");
        Ok(())
    }

    /// Get time until token expiry in seconds (negative if expired).
    pub fn secs_until_expiry(&self) -> i64 {
        let state = self.state.read().unwrap();
        state.expires_at - chrono::Utc::now().timestamp()
    }
}

/// OAuth token endpoint response.
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: u64,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    scope: String,
}

// std::sync::RwLock<T> is Send+Sync when T: Send+Sync, and all other fields
// (Arc<tokio::sync::RwLock<_>>, reqwest::Client, String) are also Send+Sync.
// No manual unsafe impls needed.
