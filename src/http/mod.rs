//! HTTP transport layer wrapping `schwab_sdk::SchwabClient`.
//!
//! The SDK handles token injection, HTTP transport, and error classification.
//! This module adds a thin logging/tracing wrapper on top.

pub mod client;
pub mod error;

pub use client::SchwabHttpClient;
pub use error::SchwabHttpError;
