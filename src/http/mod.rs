//! HTTP transport layer with retry, rate limiting, and error classification.

pub mod client;
pub mod error;

pub use client::SchwabHttpClient;
pub use error::SchwabHttpError;
