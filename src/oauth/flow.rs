//! Interactive OAuth 2.0 authorization flow for Schwab.
//!
//! Implements the three-legged OAuth flow required by Schwab's API:
//! 1. Generate authorization URL with PKCE challenge
//! 2. User authorizes in browser and receives callback with auth code
//! 3. Exchange auth code for access + refresh tokens
//!
//! This module is used during initial setup and when refresh tokens expire.
//! It is NOT used during normal trading operations (TokenProvider handles that).

use crate::common::credential::{CredentialError, SchwabCredential};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use sha2::{Digest, Sha256};
use tracing::info;

/// PKCE (Proof Key for Code Exchange) challenge parameters.
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// Code verifier (random string, stored locally).
    pub verifier: String,
    /// Code challenge (SHA-256 hash of verifier, sent to server).
    pub challenge: String,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge pair.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let verifier_bytes: Vec<u8> = (0..64).map(|_| rng.gen()).collect();
        let verifier = URL_SAFE_NO_PAD.encode(&verifier_bytes);

        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

        Self { verifier, challenge }
    }
}

/// Parameters for building the Schwab authorization URL.
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    pub app_key: String,
    pub callback_url: String,
    pub pkce: PkceChallenge,
    pub state: String,
}

impl AuthorizationRequest {
    /// Build the full authorization URL to open in the user's browser.
    pub fn to_url(&self) -> String {
        let params = [
            ("client_id", &self.app_key),
            ("redirect_uri", &self.callback_url),
            ("response_type", &"code".to_string()),
            ("code_challenge", &self.pkce.challenge),
            ("code_challenge_method", &"S256".to_string()),
            ("state", &self.state),
        ];

        let query: String = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        format!(
            "https://api.schwabapi.com/v1/oauth/authorize?{}",
            query
        )
    }
}

/// Result of a successful OAuth token exchange.
#[derive(Debug, Clone)]
pub struct TokenExchangeResult {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

/// Execute the token exchange step of the OAuth flow.
///
/// Exchanges an authorization code for access and refresh tokens.
pub async fn exchange_code_for_tokens(
    app_key: &str,
    app_secret: &str,
    callback_url: &str,
    code: &str,
    pkce_verifier: &str,
) -> Result<TokenExchangeResult, CredentialError> {
    let client = reqwest::Client::new();

    let response = client
        .post("https://api.schwabapi.com/v1/oauth/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", callback_url),
            ("client_id", app_key),
            ("client_secret", app_secret),
            ("code_verifier", pkce_verifier),
        ])
        .send()
        .await
        .map_err(|e| CredentialError::InvalidFormat(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(CredentialError::InvalidFormat(format!(
            "HTTP {}: {}",
            status, body
        )));
    }

    let token_response: serde_json::Value = response
        .json()
        .await
        .map_err(|e| CredentialError::InvalidFormat(e.to_string()))?;

    let access_token = token_response["access_token"]
        .as_str()
        .ok_or_else(|| CredentialError::InvalidFormat("missing access_token".into()))?
        .to_string();

    let refresh_token = token_response["refresh_token"]
        .as_str()
        .ok_or_else(|| CredentialError::InvalidFormat("missing refresh_token".into()))?
        .to_string();

    let expires_in = token_response["expires_in"]
        .as_u64()
        .unwrap_or(1800);

    info!("OAuth token exchange successful");

    Ok(TokenExchangeResult {
        access_token,
        refresh_token,
        expires_in,
    })
}

/// Build a complete SchwabCredential from an OAuth flow result.
pub fn build_credential_from_flow(
    app_key: String,
    app_secret: String,
    callback_url: String,
    result: TokenExchangeResult,
) -> SchwabCredential {
    SchwabCredential::new(
        app_key,
        app_secret,
        callback_url,
        result.access_token,
        result.refresh_token,
    )
}
