//! OAuth 2.0 authentication for Schwab API.
//!
//! Handles the three-legged OAuth flow and automatic token refresh.
//! Access tokens expire after 30 minutes; refresh tokens after 7 days.

pub mod flow;
pub mod provider;

pub use provider::SchwabTokenProvider;
