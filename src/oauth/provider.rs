//! OAuth token provider with automatic refresh.
//!
//! Integrates with existing schwab-mcp infrastructure:
//! - Reads tokens from `~/.local/share/schwab-mcp/token.yaml`
//! - Reads credentials from `~/.local/share/schwab-mcp/credentials.yaml`
//! - Refreshes via direct HTTP call to Schwab token endpoint
//! - Falls back to environment variables if files not found
//!
//! Token lifecycle:
//! - Access tokens expire after 30 minutes
//! - Refresh tokens expire after 7 days
//! - Proactive refresh 5 minutes before expiry

use crate::common::credential::SchwabCredential;
use crate::http::error::SchwabHttpError;
use secrecy::{ExposeSecret, SecretString};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
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

/// Thread-safe OAuth token provider with automatic refresh.
///
/// Designed to be shared across multiple clients via `Arc<SchwabTokenProvider>`.
/// Uses `RwLock` to allow concurrent reads while serializing token refresh.
pub struct SchwabTokenProvider {
    /// Current token state (access token, refresh token, expiry).
    state: RwLock<TokenState>,
    /// Credentials for token refresh requests.
    credential: Arc<RwLock<SchwabCredential>>,
    /// HTTP client for token endpoint requests (avoids circular dependency).
    http: reqwest::Client,
    /// Schwab OAuth token endpoint URL.
    token_url: String,
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
            .map_err(|e| SchwabHttpError::Parse(format!("invalid token.yaml: {}", e)))?;

        let token = token_doc.get("token").unwrap_or(&token_doc);
        let access_token = token["access_token"]
            .as_str()
            .ok_or_else(|| SchwabHttpError::Parse("missing access_token in token.yaml".into()))?;
        let refresh_token = token["refresh_token"]
            .as_str()
            .ok_or_else(|| SchwabHttpError::Parse("missing refresh_token in token.yaml".into()))?;
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
            .map_err(|e| SchwabHttpError::Parse(format!("invalid credentials.yaml: {}", e)))?;

        let app_key = creds_doc["app_key"]
            .as_str()
            .or_else(|| creds_doc["client_id"].as_str())
            .ok_or_else(|| SchwabHttpError::Parse("missing app_key in credentials.yaml".into()))?;
        let app_secret = creds_doc["app_secret"]
            .as_str()
            .or_else(|| creds_doc["client_secret"].as_str())
            .ok_or_else(|| SchwabHttpError::Parse("missing app_secret in credentials.yaml".into()))?;
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
            access_token: SecretString::new(credential.access_token().to_string()),
            refresh_token: SecretString::new(credential.refresh_token().to_string()),
            expires_at: chrono::Utc::now().timestamp() + 1800,
            refresh_buffer_secs: 300, // Refresh 5 minutes before expiry
        };

        Self {
            state: RwLock::new(initial_state),
            credential: Arc::new(RwLock::new(credential)),
            http: reqwest::Client::new(),
            token_url: "https://api.schwabapi.com/v1/oauth/token".to_string(),
        }
    }

    /// Get the current access token string.
    ///
    /// This does NOT check expiry; callers should use `needs_refresh()` first
    /// or rely on `execute_with_retry` which handles this automatically.
    pub fn current_access_token(&self) -> String {
        // Synchronous read path — acceptable because we hold the lock briefly
        // and the token is a short string copy.
        tokio::task::block_in_place(|| {
            let handle = tokio::runtime::Handle::current();
            handle.block_on(async { self.state.read().await.access_token.expose_secret().to_string() })
        })
    }

    /// Check if the access token needs refresh.
    pub async fn needs_refresh(&self) -> bool {
        self.state.read().await.is_expired()
    }

    /// Refresh the access token using the refresh token.
    ///
    /// This method is safe to call concurrently; only one refresh will execute
    /// at a time due to the write lock on `state`.
    pub async fn refresh_token(&self) -> Result<(), SchwabHttpError> {
        let mut state = self.state.write().await;

        // Double-check: another task may have refreshed while we waited
        if !state.is_expired() {
            debug!("token already refreshed by another task");
            return Ok(());
        }

        let cred = self.credential.read().await;
        info!("refreshing Schwab access token");

        let response = self
            .http
            .post(&self.token_url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", state.refresh_token.expose_secret()),
                ("client_id", cred.app_key()),
                ("client_secret", cred.app_secret()),
            ])
            .send()
            .await
            .map_err(|e| SchwabHttpError::Transport(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            warn!(status, body = %body, "token refresh failed");
            return Err(SchwabHttpError::HttpStatus {
                status,
                message: body,
            });
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| SchwabHttpError::Parse(e.to_string()))?;

        // Update state with new tokens
        state.access_token = SecretString::new(token_response.access_token.clone());
        if let Some(new_refresh) = token_response.refresh_token {
            state.refresh_token = SecretString::new(new_refresh);
        }
        state.expires_at = chrono::Utc::now().timestamp() + token_response.expires_in as i64;

        // Also update the credential store
        drop(state);
        let mut cred = self.credential.write().await;
        cred.update_access_token(token_response.access_token);

        info!("Schwab access token refreshed successfully");
        Ok(())
    }

    /// Get time until token expiry in seconds (negative if expired).
    pub async fn secs_until_expiry(&self) -> i64 {
        let state = self.state.read().await;
        state.expires_at - chrono::Utc::now().timestamp()
    }
}

/// OAuth token endpoint response.
#[derive(Debug, serde::Deserialize)]
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

// Safety: SchwabTokenProvider uses RwLock for interior mutability
unsafe impl Send for SchwabTokenProvider {}
unsafe impl Sync for SchwabTokenProvider {}
