//! HTTP error types with retry classification.
//!
//! Follows Nautilus adapter error handling conventions:
//! - Transport, HTTP status, venue, parsing, and validation errors are distinct
//! - Retryability is classified per error variant
//! - Credentials are never included in error output

use std::fmt;

/// Classified HTTP error for Schwab API interactions.
#[derive(Debug, thiserror::Error)]
pub enum SchwabHttpError {
    /// Network-level transport failure (DNS, TCP, TLS, timeout).
    #[error("transport error: {0}")]
    Transport(String),

    /// HTTP status code indicating a server or client error.
    #[error("HTTP {status}: {message}")]
    HttpStatus {
        status: u16,
        message: String,
    },

    /// Schwab API returned a structured error response.
    #[error("Schwab API error [{code}]: {message}")]
    ApiError {
        code: String,
        message: String,
    },

    /// Response body could not be parsed.
    #[error("parse error: {0}")]
    Parse(String),

    /// Request validation failed before sending.
    #[error("validation error: {0}")]
    Validation(String),

    /// OAuth token expired or invalid; refresh required.
    #[error("authentication required: token expired or invalid")]
    AuthRequired,

    /// Rate limit exceeded; retry after the specified duration.
    #[error("rate limited: retry after {retry_after_ms}ms")]
    RateLimited {
        retry_after_ms: u64,
    },
}

impl SchwabHttpError {
    /// Whether this error is safe to retry.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transport(_) => true,
            Self::HttpStatus { status, .. } => {
                // Retry on 429, 500, 502, 503, 504
                matches!(status, 429 | 500 | 502 | 503 | 504)
            }
            Self::ApiError { .. } => false,
            Self::Parse(_) => false,
            Self::Validation(_) => false,
            Self::AuthRequired => false, // Needs token refresh, not simple retry
            Self::RateLimited { .. } => true,
        }
    }

    /// Suggested retry delay in milliseconds, if applicable.
    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_ms } => Some(*retry_after_ms),
            Self::HttpStatus { status: 429, .. } => Some(1000),
            Self::Transport(_) => Some(500),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for SchwabHttpError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() || err.is_connect() || err.is_request() {
            Self::Transport(err.to_string())
        } else if let Some(status) = err.status() {
            Self::HttpStatus {
                status: status.as_u16(),
                message: err.to_string(),
            }
        } else {
            Self::Transport(err.to_string())
        }
    }
}
