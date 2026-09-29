//! OAuth token provider with automatic refresh.
//!
//! Manages the lifecycle of Schwab OAuth tokens:
//! - Tracks access token expiry (30-minute TTL)
//! - Automatically refreshes using refresh token before expiry
//! - Thread-safe for concurrent access from data and execution clients
//! - Zeroizes old tokens on replacement

use crate::common::credential::SchwabCredential;
use crate::http::error::SchwabHttpError;
use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

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
    /// Create a new token provider from credentials.
    ///
    /// Assumes the initial access token is valid. The provider will
    /// automatically refresh it when it approaches expiry.
    pub fn new(credential: SchwabCredential) -> Self {
        let initial_state = TokenState {
            access_token: SecretString::new(credential.access_token().to_string()),
            refresh_token: SecretString::new(credential.refresh_token().to_string()),
            // Assume token was just issued; expires in 30 minutes
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
