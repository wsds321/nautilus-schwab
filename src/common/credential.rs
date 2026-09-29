//! Schwab credential management with secret zeroization.
//!
//! Follows Nautilus adapter credential handling conventions:
//! - All secret fields use `SecretString` or zeroizing wrappers
//! - Debug output redacts all sensitive values
//! - Plaintext lifetime is bounded to the authentication boundary

use secrecy::{ExposeSecret, SecretString};
use std::fmt;
use zeroize::ZeroizeOnDrop;

/// Complete set of Schwab API credentials.
///
/// Resolved once at startup from environment variables or config file.
/// All secret fields are zeroized on drop.
#[derive(Clone, ZeroizeOnDrop)]
pub struct SchwabCredential {
    /// OAuth App Key (Client ID)
    app_key: SecretString,
    /// OAuth App Secret (Client Secret)
    app_secret: SecretString,
    /// OAuth callback URL
    callback_url: String,
    /// Current access token (short-lived, auto-refreshed)
    access_token: SecretString,
    /// Current refresh token (long-lived, used to obtain new access tokens)
    refresh_token: SecretString,
}

impl SchwabCredential {
    /// Create credentials from individual components.
    pub fn new(
        app_key: impl Into<String>,
        app_secret: impl Into<String>,
        callback_url: impl Into<String>,
        access_token: impl Into<String>,
        refresh_token: impl Into<String>,
    ) -> Self {
        Self {
            app_key: SecretString::new(app_key.into()),
            app_secret: SecretString::new(app_secret.into()),
            callback_url: callback_url.into(),
            access_token: SecretString::new(access_token.into()),
            refresh_token: SecretString::new(refresh_token.into()),
        }
    }

    /// Load credentials from environment variables.
    ///
    /// Expected variables:
    /// - `SCHWAB_APP_KEY`
    /// - `SCHWAB_APP_SECRET`
    /// - `SCHWAB_CALLBACK_URL`
    /// - `SCHWAB_ACCESS_TOKEN`
    /// - `SCHWAB_REFRESH_TOKEN`
    pub fn from_env() -> Result<Self, CredentialError> {
        let app_key = std::env::var("SCHWAB_APP_KEY")
            .map_err(|_| CredentialError::MissingEnv("SCHWAB_APP_KEY"))?;
        let app_secret = std::env::var("SCHWAB_APP_SECRET")
            .map_err(|_| CredentialError::MissingEnv("SCHWAB_APP_SECRET"))?;
        let callback_url = std::env::var("SCHWAB_CALLBACK_URL")
            .unwrap_or_else(|_| "https://127.0.0.1/callback".to_string());
        let access_token = std::env::var("SCHWAB_ACCESS_TOKEN")
            .map_err(|_| CredentialError::MissingEnv("SCHWAB_ACCESS_TOKEN"))?;
        let refresh_token = std::env::var("SCHWAB_REFRESH_TOKEN")
            .map_err(|_| CredentialError::MissingEnv("SCHWAB_REFRESH_TOKEN"))?;

        Ok(Self::new(
            app_key,
            app_secret,
            callback_url,
            access_token,
            refresh_token,
        ))
    }

    /// Get the app key (client ID) for OAuth requests.
    pub fn app_key(&self) -> &str {
        self.app_key.expose_secret()
    }

    /// Get the app secret for OAuth token exchange.
    pub fn app_secret(&self) -> &str {
        self.app_secret.expose_secret()
    }

    /// Get the OAuth callback URL.
    pub fn callback_url(&self) -> &str {
        &self.callback_url
    }

    /// Get the current access token.
    pub fn access_token(&self) -> &str {
        self.access_token.expose_secret()
    }

    /// Get the current refresh token.
    pub fn refresh_token(&self) -> &str {
        self.refresh_token.expose_secret()
    }

    /// Update the access token after a successful refresh.
    pub fn update_access_token(&mut self, new_token: impl Into<String>) {
        self.access_token = SecretString::new(new_token.into());
    }

    /// Update both tokens after a full re-authorization.
    pub fn update_tokens(
        &mut self,
        new_access: impl Into<String>,
        new_refresh: impl Into<String>,
    ) {
        self.access_token = SecretString::new(new_access.into());
        self.refresh_token = SecretString::new(new_refresh.into());
    }
}

impl fmt::Debug for SchwabCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SchwabCredential")
            .field("app_key", &"<redacted>")
            .field("app_secret", &"<redacted>")
            .field("callback_url", &self.callback_url)
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .finish()
    }
}

/// Errors during credential resolution.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("missing environment variable: {0}")]
    MissingEnv(&'static str),

    #[error("invalid credential format: {0}")]
    InvalidFormat(String),

    #[error("IO error reading credentials: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_secrets() {
        let cred = SchwabCredential::new(
            "my-app-key",
            "my-app-secret",
            "https://localhost/callback",
            "my-access-token",
            "my-refresh-token",
        );
        let debug = format!("{:?}", cred);
        assert!(!debug.contains("my-app-key"));
        assert!(!debug.contains("my-app-secret"));
        assert!(!debug.contains("my-access-token"));
        assert!(!debug.contains("my-refresh-token"));
        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("https://localhost/callback"));
    }

    #[test]
    fn expose_returns_actual_values() {
        let cred = SchwabCredential::new(
            "key123",
            "secret456",
            "https://localhost/cb",
            "access789",
            "refresh012",
        );
        assert_eq!(cred.app_key(), "key123");
        assert_eq!(cred.app_secret(), "secret456");
        assert_eq!(cred.access_token(), "access789");
        assert_eq!(cred.refresh_token(), "refresh012");
    }
}
