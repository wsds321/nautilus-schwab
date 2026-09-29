//! HTTP error types wrapping `schwab_sdk::Error`.
//!
//! Provides a thin adapter layer that:
//! - Wraps `schwab_sdk::Error` as the primary error source
//! - Adds adapter-specific error variants (validation, auth lifecycle)
//! - Delegates retry classification to `schwab_sdk::Error::is_retryable()`
//! - Credentials are never included in error output

use std::time::Duration;

/// Classified error for Schwab API interactions.
///
/// Wraps `schwab_sdk::Error` for all SDK-originated errors and adds
/// adapter-specific variants for validation and token lifecycle events.
#[derive(Debug, thiserror::Error)]
pub enum SchwabHttpError {
    /// Error originating from schwab-sdk (transport, HTTP status, codec, etc.).
    /// Retry classification delegates to `schwab_sdk::Error::is_retryable()`.
    #[error(transparent)]
    Sdk(#[from] schwab_sdk::Error),

    /// Request validation failed before sending.
    #[error("validation error: {0}")]
    Validation(String),

    /// OAuth token expired or invalid; refresh required.
    #[error("authentication required: token expired or invalid")]
    AuthRequired,

    /// Token provider failed to produce a valid token.
    #[error("token provider error: {0}")]
    TokenProvider(String),
}

impl SchwabHttpError {
    /// Whether this error is safe to retry.
    ///
    /// Delegates to `schwab_sdk::Error::is_retryable()` for SDK errors.
    /// Adapter-specific errors are not retryable (validation/auth require
    /// corrective action, not repetition).
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Sdk(e) => e.is_retryable(),
            Self::Validation(_) | Self::AuthRequired | Self::TokenProvider(_) => false,
        }
    }

    /// Suggested retry delay, if applicable.
    ///
    /// Delegates to `schwab_sdk::Error::retry_after()` for SDK errors.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Sdk(e) => e.retry_after(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_retryable() ──────────────────────────────────────────────────

    #[test]
    fn test_validation_not_retryable() {
        let err = SchwabHttpError::Validation("bad input".into());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_auth_required_not_retryable() {
        let err = SchwabHttpError::AuthRequired;
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_token_provider_not_retryable() {
        let err = SchwabHttpError::TokenProvider("failed to refresh".into());
        assert!(!err.is_retryable());
    }

    // ── retry_after() ───────────────────────────────────────────────────

    #[test]
    fn test_validation_no_retry_after() {
        let err = SchwabHttpError::Validation("x".into());
        assert_eq!(err.retry_after(), None);
    }

    #[test]
    fn test_auth_required_no_retry_after() {
        let err = SchwabHttpError::AuthRequired;
        assert_eq!(err.retry_after(), None);
    }

    #[test]
    fn test_token_provider_no_retry_after() {
        let err = SchwabHttpError::TokenProvider("x".into());
        assert_eq!(err.retry_after(), None);
    }

    // ── Display / Error trait ───────────────────────────────────────────

    #[test]
    fn test_validation_display() {
        let err = SchwabHttpError::Validation("missing price".into());
        let msg = format!("{}", err);
        assert!(msg.contains("validation error"));
        assert!(msg.contains("missing price"));
    }

    #[test]
    fn test_auth_required_display() {
        let err = SchwabHttpError::AuthRequired;
        let msg = format!("{}", err);
        assert!(msg.contains("authentication required"));
    }

    #[test]
    fn test_token_provider_display() {
        let err = SchwabHttpError::TokenProvider("network timeout".into());
        let msg = format!("{}", err);
        assert!(msg.contains("token provider error"));
        assert!(msg.contains("network timeout"));
    }

    // ── From<sdk::Error> conversion ─────────────────────────────────────

    #[test]
    fn test_from_sdk_error() {
        // Verify the From impl compiles and produces the Sdk variant
        use schwab_sdk::error::ErrorBody;
        let sdk_err = schwab_sdk::Error::Unauthorized(ErrorBody::Unrecognized("test".into()));
        let http_err: SchwabHttpError = sdk_err.into();
        assert!(matches!(http_err, SchwabHttpError::Sdk(_)));
    }
}
