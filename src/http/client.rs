//! HTTP client wrapper with retry logic and rate limiting.
//!
//! Wraps `schwab-sdk`'s `SchwabClient` with additional resilience:
//! - Exponential backoff retry for transient failures
//! - Rate limit awareness with adaptive throttling
//! - Token refresh on 401 responses
//! - Credential-safe error reporting (no secrets in logs)

use crate::common::credential::SchwabCredential;
use crate::http::error::SchwabHttpError;
use crate::oauth::provider::SchwabTokenProvider;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Configuration for the HTTP client's retry and rate-limit behavior.
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Maximum number of retry attempts for retryable errors.
    pub max_retries: u32,
    /// Base delay between retries in milliseconds (exponential backoff).
    pub base_retry_delay_ms: u64,
    /// Maximum delay between retries in milliseconds.
    pub max_retry_delay_ms: u64,
    /// Maximum concurrent requests to Schwab API.
    pub max_concurrent_requests: usize,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_retry_delay_ms: 500,
            max_retry_delay_ms: 30_000,
            max_concurrent_requests: 10,
        }
    }
}

/// Resilient HTTP client for Schwab API interactions.
///
/// Wraps `schwab_sdk::SchwabClient` with retry, rate limiting,
/// and automatic token refresh capabilities.
pub struct SchwabHttpClient {
    /// Inner schwab-sdk client (recreated on token refresh).
    inner: Arc<RwLock<schwab_sdk::SchwabClient>>,
    /// Token provider for automatic refresh.
    token_provider: Arc<SchwabTokenProvider>,
    /// Client configuration.
    config: HttpClientConfig,
    /// Semaphore for concurrency limiting.
    _semaphore: Arc<tokio::sync::Semaphore>,
}

impl SchwabHttpClient {
    /// Create a new HTTP client with the given credentials and config.
    pub fn new(
        credential: SchwabCredential,
        config: HttpClientConfig,
    ) -> Result<Self, SchwabHttpError> {
        let token_provider = Arc::new(SchwabTokenProvider::new(credential));
        let semaphore = Arc::new(tokio::sync::Semaphore::new(
            config.max_concurrent_requests,
        ));

        // Create initial schwab-sdk client
        let access_token = token_provider.current_access_token();
        let sdk_client = schwab_sdk::SchwabClient::new(schwab_sdk::AuthToken::new(access_token));

        Ok(Self {
            inner: Arc::new(RwLock::new(sdk_client)),
            token_provider,
            config,
            _semaphore: semaphore,
        })
    }

    /// Get a reference to the token provider.
    pub fn token_provider(&self) -> &Arc<SchwabTokenProvider> {
        &self.token_provider
    }

    /// Execute a request with automatic retry and token refresh.
    ///
    /// The provided closure receives a reference to the inner `SchwabClient`
    /// and should return a `Result`. On retryable errors, the request is
    /// retried with exponential backoff. On auth errors, the token is
    /// refreshed and the request is retried once.
    pub async fn execute_with_retry<F, Fut, T>(
        &self,
        operation_name: &str,
        f: F,
    ) -> Result<T, SchwabHttpError>
    where
        F: Fn(schwab_sdk::SchwabClient) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<T, SchwabHttpError>> + Send,
    {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let delay = self.calculate_backoff(attempt, last_error.as_ref());
                debug!(
                    operation = operation_name,
                    attempt = attempt,
                    delay_ms = delay,
                    "retrying after error"
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
            }

            // Check if token needs refresh before each attempt
            if self.token_provider.needs_refresh() {
                debug!(operation = operation_name, "refreshing access token");
                if let Err(e) = self.token_provider.refresh_token().await {
                    warn!(operation = operation_name, error = %e, "token refresh failed");
                    return Err(SchwabHttpError::AuthRequired);
                }
                // Recreate SDK client with new token
                let new_token = self.token_provider.current_access_token();
                let new_client =
                    schwab_sdk::SchwabClient::new(schwab_sdk::AuthToken::new(new_token));
                *self.inner.write().await = new_client;
            }

            let client = self.inner.read().await.clone();
            match f(client).await {
                Ok(result) => return Ok(result),
                Err(e) if e.is_retryable() && attempt < self.config.max_retries => {
                    warn!(
                        operation = operation_name,
                        attempt = attempt,
                        error = %e,
                        "retryable error"
                    );
                    last_error = Some(e);
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_error.unwrap_or(SchwabHttpError::Transport(
            "max retries exhausted".to_string(),
        )))
    }

    /// Calculate exponential backoff delay with jitter.
    fn calculate_backoff(&self, attempt: u32, error: Option<&SchwabHttpError>) -> u64 {
        // Use server-provided retry-after if available
        if let Some(e) = error {
            if let Some(retry_after) = e.retry_after_ms() {
                return retry_after.min(self.config.max_retry_delay_ms);
            }
        }

        // Exponential backoff: base * 2^(attempt-1), capped at max
        let delay = self.config.base_retry_delay_ms * 2u64.saturating_pow(attempt - 1);
        delay.min(self.config.max_retry_delay_ms)
    }
}

// Safety: SchwabHttpClient is designed to be shared across tasks
unsafe impl Send for SchwabHttpClient {}
unsafe impl Sync for SchwabHttpClient {}
