//! Thin HTTP client wrapper around `schwab_sdk::SchwabClient`.
//!
//! Adds logging/tracing on top of schwab-sdk's built-in client.
//! The SDK handles token injection via `TokenProvider`, and exposes
//! `Error::is_retryable()` / `Error::retry_after()` for retry policies.
//!
//! This wrapper does NOT implement its own retry logic — callers should
//! use `backon` or similar, guided by the error's retry hints.

use crate::http::error::SchwabHttpError;
use crate::oauth::provider::SchwabTokenProvider;
use std::sync::Arc;
use tracing::debug;

/// Resilient HTTP client for Schwab API interactions.
///
/// Wraps `schwab_sdk::SchwabClient` with tracing instrumentation.
/// Token management is handled by the `TokenProvider` trait implementation
/// in `SchwabTokenProvider`, which the SDK consults once per request.
pub struct SchwabHttpClient {
    /// Inner schwab-sdk client with our TokenProvider wired in.
    inner: schwab_sdk::SchwabClient,
    /// Token provider (shared with the SDK client via Arc).
    token_provider: Arc<SchwabTokenProvider>,
}

impl SchwabHttpClient {
    /// Create a new HTTP client backed by the given token provider.
    pub fn new(token_provider: Arc<SchwabTokenProvider>) -> Self {
        let sdk_client =
            schwab_sdk::SchwabClient::with_token_provider(token_provider.clone() as Arc<_>);

        debug!("created SchwabHttpClient with TokenProvider");

        Self {
            inner: sdk_client,
            token_provider,
        }
    }

    /// Get a reference to the underlying `schwab_sdk::SchwabClient`.
    ///
    /// Use this to access all SDK namespace methods: `.accounts()`,
    /// `.orders(hash)`, `.market_data()`, `.streamer()`, etc.
    pub fn inner(&self) -> &schwab_sdk::SchwabClient {
        &self.inner
    }

    /// Get a reference to the token provider.
    pub fn token_provider(&self) -> &Arc<SchwabTokenProvider> {
        &self.token_provider
    }

    /// Execute an operation against the SDK client with tracing.
    ///
    /// This is a convenience wrapper that logs the operation name and
    /// maps SDK errors into `SchwabHttpError`. For complex retry logic,
    /// use `inner()` directly with your own policy.
    pub async fn execute<F, Fut, T>(
        &self,
        operation_name: &str,
        f: F,
    ) -> Result<T, SchwabHttpError>
    where
        F: FnOnce(&schwab_sdk::SchwabClient) -> Fut,
        Fut: std::future::Future<Output = Result<T, schwab_sdk::Error>>,
    {
        debug!(operation = operation_name, "executing Schwab API call");
        f(&self.inner)
            .await
            .map_err(|e| {
                if e.is_retryable() {
                    tracing::warn!(
                        operation = operation_name,
                        retry_after = ?e.retry_after(),
                        error = %e,
                        "retryable Schwab API error"
                    );
                } else {
                    tracing::error!(
                        operation = operation_name,
                        error = %e,
                        "non-retryable Schwab API error"
                    );
                }
                SchwabHttpError::from(e)
            })
    }
}

// SchwabClient is Clone + Send + Sync; our wrapper inherits these properties.
unsafe impl Send for SchwabHttpClient {}
unsafe impl Sync for SchwabHttpClient {}
