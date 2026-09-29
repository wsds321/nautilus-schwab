//! Factory implementations for creating Schwab adapter components.
//!
//! Implements the Nautilus `DataClientFactory` and `ExecutionClientFactory` traits,
//! enabling the plugin system to instantiate Schwab clients from configuration.
//! Also retains legacy convenience functions for backward compatibility.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Context, anyhow};
use nautilus_common::cache::CacheView;
use nautilus_common::clock::Clock;
use nautilus_common::clients::{DataClient, ExecutionClient};
use nautilus_common::factories::client::{ClientConfig, DataClientFactory, ExecutionClientFactory};
use nautilus_model::identifiers::TraderId;

use crate::common::credential::SchwabCredential;
use crate::data::{SchwabDataClient, SchwabDataClientConfig};
use crate::execution::{SchwabExecutionClient, SchwabExecutionClientConfig};

// ── Data Client Factory ─────────────────────────────────────────────────────

/// Factory for creating Schwab data client instances.
///
/// Implements the Nautilus `DataClientFactory` trait, allowing the live system
/// kernel to instantiate `SchwabDataClient` from a generic `ClientConfig`.
#[derive(Debug)]
pub struct SchwabDataClientFactory;

impl DataClientFactory for SchwabDataClientFactory {
    fn create(
        &self,
        _name: &str,
        config: &dyn ClientConfig,
        _cache: CacheView,
        _clock: Rc<RefCell<dyn Clock>>,
    ) -> anyhow::Result<Box<dyn DataClient>> {
        let schwab_config = config
            .as_any()
            .downcast_ref::<SchwabDataClientConfig>()
            .ok_or_else(|| {
                anyhow!(
                    "expected SchwabDataClientConfig, got {}",
                    std::any::type_name_of_val(config)
                )
            })?;

        let credential = SchwabCredential::from_env()
            .context("failed to load Schwab credentials from environment")?;

        let client = SchwabDataClient::new(credential, schwab_config.clone())
            .map_err(|e| anyhow!("failed to create SchwabDataClient: {e}"))?;

        Ok(Box::new(client))
    }

    fn name(&self) -> &str {
        "SCHWAB"
    }

    fn config_type(&self) -> &str {
        "SchwabDataClientConfig"
    }
}

// ── Execution Client Factory ────────────────────────────────────────────────

/// Factory for creating Schwab execution client instances.
///
/// Implements the Nautilus `ExecutionClientFactory` trait, allowing the live system
/// kernel to instantiate `SchwabExecutionClient` from a generic `ClientConfig`.
#[derive(Debug)]
pub struct SchwabExecutionClientFactory;

impl ExecutionClientFactory for SchwabExecutionClientFactory {
    fn create(
        &self,
        _trader_id: TraderId,
        _name: &str,
        config: &dyn ClientConfig,
        _cache: CacheView,
        _clock: Rc<RefCell<dyn Clock>>,
    ) -> anyhow::Result<Box<dyn ExecutionClient>> {
        let schwab_config = config
            .as_any()
            .downcast_ref::<SchwabExecutionClientConfig>()
            .ok_or_else(|| {
                anyhow!(
                    "expected SchwabExecutionClientConfig, got {}",
                    std::any::type_name_of_val(config)
                )
            })?;

        let credential = SchwabCredential::from_env()
            .context("failed to load Schwab credentials from environment")?;

        let client = SchwabExecutionClient::new(credential, schwab_config.clone())
            .map_err(|e| anyhow!("failed to create SchwabExecutionClient: {e}"))?;

        Ok(Box::new(client))
    }

    fn name(&self) -> &str {
        "SCHWAB"
    }

    fn config_type(&self) -> &str {
        "SchwabExecutionClientConfig"
    }
}

// ── Legacy convenience functions (backward compatibility) ───────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_factory_metadata() {
        let factory = SchwabDataClientFactory;
        assert_eq!(factory.name(), "SCHWAB");
        assert_eq!(factory.config_type(), "SchwabDataClientConfig");
    }

    #[test]
    fn test_execution_factory_metadata() {
        let factory = SchwabExecutionClientFactory;
        assert_eq!(factory.name(), "SCHWAB");
        assert_eq!(factory.config_type(), "SchwabExecutionClientConfig");
    }

    #[test]
    fn test_data_factory_rejects_wrong_config() {
        use std::any::Any;

        #[derive(Debug)]
        struct WrongConfig;
        impl ClientConfig for WrongConfig {
            fn as_any(&self) -> &dyn Any { self }
        }

        let factory = SchwabDataClientFactory;
        let wrong = WrongConfig;
        // We can't easily construct a CacheView or Clock in a unit test,
        // but we verify the downcast logic by checking the error message.
        // The actual create() call would fail at downcast before touching cache/clock.
        let result = factory.create(
            "test",
            &wrong,
            // These won't be reached due to downcast failure
            unsafe { std::mem::zeroed() },
            Rc::new(RefCell::new(nautilus_common::clock::TestClock::default())),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SchwabDataClientConfig"));
    }

    #[test]
    fn test_execution_factory_rejects_wrong_config() {
        use std::any::Any;

        #[derive(Debug)]
        struct WrongConfig;
        impl ClientConfig for WrongConfig {
            fn as_any(&self) -> &dyn Any { self }
        }

        let factory = SchwabExecutionClientFactory;
        let wrong = WrongConfig;
        let trader_id = TraderId::from("TESTER-001");
        let result = factory.create(
            trader_id,
            "test",
            &wrong,
            unsafe { std::mem::zeroed() },
            Rc::new(RefCell::new(nautilus_common::clock::TestClock::default())),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SchwabExecutionClientConfig"));
    }
}
