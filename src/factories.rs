//! Factory functions for creating Schwab adapter components.
//!
//! These factories are called by Nautilus Trader's plugin system
//! to instantiate data and execution clients from configuration.

use crate::common::credential::SchwabCredential;
use crate::data::{SchwabDataClient, SchwabDataClientConfig};
use crate::execution::{SchwabExecutionClient, SchwabExecutionClientConfig};

/// Create a Schwab data client from environment credentials and default config.
pub fn create_data_client_default() -> Result<SchwabDataClient, Box<dyn std::error::Error + Send + Sync>> {
    let credential = SchwabCredential::from_env()?;
    SchwabDataClient::new(credential, SchwabDataClientConfig::default())
}

/// Create a Schwab data client with custom configuration.
pub fn create_data_client(
    credential: SchwabCredential,
    config: SchwabDataClientConfig,
) -> Result<SchwabDataClient, Box<dyn std::error::Error + Send + Sync>> {
    SchwabDataClient::new(credential, config)
}

/// Create a Schwab execution client from environment credentials and default config.
pub fn create_execution_client_default() -> Result<SchwabExecutionClient, Box<dyn std::error::Error + Send + Sync>> {
    let credential = SchwabCredential::from_env()?;
    SchwabExecutionClient::new(credential, SchwabExecutionClientConfig::default())
}

/// Create a Schwab execution client with custom configuration.
pub fn create_execution_client(
    credential: SchwabCredential,
    config: SchwabExecutionClientConfig,
) -> Result<SchwabExecutionClient, Box<dyn std::error::Error + Send + Sync>> {
    SchwabExecutionClient::new(credential, config)
}
