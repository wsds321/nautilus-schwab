//! Schwab DataClient implementation for Nautilus Trader.
//!
//! Provides real-time and historical market data from Charles Schwab:
//! - Real-time quotes via WebSocket streamer (LEVELONE_EQUITIES)
//! - Real-time bars via WebSocket streamer (CHART_EQUITY)
//! - Historical bars via REST API (/marketdata/v1/pricehistory)
//! - Instrument definitions via REST API (/marketdata/v1/instruments)

use crate::common::credential::SchwabCredential;
use crate::common::symbol::SchwabSymbol;
use crate::http::client::{HttpClientConfig, SchwabHttpClient};
use crate::oauth::provider::SchwabTokenProvider;
use crate::websocket::client::{StreamerConfig, SchwabStreamerClient};
use std::sync::Arc;
use tracing::info;

/// Configuration for the Schwab data client.
#[derive(Debug, Clone)]
pub struct SchwabDataClientConfig {
    /// HTTP client configuration.
    pub http: HttpClientConfig,
    /// WebSocket streamer configuration.
    pub streamer: StreamerConfig,
    /// Market data API base URL.
    pub market_data_base_url: String,
}

impl Default for SchwabDataClientConfig {
    fn default() -> Self {
        Self {
            http: HttpClientConfig::default(),
            streamer: StreamerConfig::default(),
            market_data_base_url: crate::DEFAULT_MARKET_DATA_BASE_URL.to_string(),
        }
    }
}

/// Schwab DataClient for Nautilus Trader.
///
/// Implements the Nautilus `DataClient` interface, providing:
/// - Live quote ticks from the WebSocket streamer
/// - Live bar data from the WebSocket streamer
/// - Historical bar requests via REST API
/// - Instrument definition lookups
pub struct SchwabDataClient {
    /// HTTP client for REST API calls.
    http_client: Arc<SchwabHttpClient>,
    /// WebSocket streamer for real-time data.
    streamer: SchwabStreamerClient,
    /// Client configuration.
    config: SchwabDataClientConfig,
}

impl SchwabDataClient {
    /// Create a new Schwab data client.
    pub fn new(
        credential: SchwabCredential,
        config: SchwabDataClientConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let token_provider = Arc::new(SchwabTokenProvider::new(credential.clone()));
        let http_client = Arc::new(SchwabHttpClient::new(credential, config.http.clone())?);
        let streamer = SchwabStreamerClient::new(token_provider, config.streamer.clone());

        Ok(Self {
            http_client,
            streamer,
            config,
        })
    }

    /// Subscribe to real-time quotes for a symbol.
    pub async fn subscribe_quotes(
        &self,
        symbol: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let schwab_sym = SchwabSymbol::new(symbol)?;
        info!(symbol = %schwab_sym, "subscribing to quotes");
        self.streamer
            .subscribe(
                "LEVELONE_EQUITIES",
                vec![schwab_sym.as_str().to_string()],
                // Fields: 1=Bid, 2=Ask, 3=Last, 4=BidSize, 5=AskSize, 6=Volume, 7=Timestamp
                vec!["1".into(), "2".into(), "3".into(), "4".into(), "5".into(), "6".into(), "7".into()],
            )
            .await?;
        Ok(())
    }

    /// Subscribe to real-time bars for a symbol.
    pub async fn subscribe_bars(
        &self,
        symbol: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let schwab_sym = SchwabSymbol::new(symbol)?;
        info!(symbol = %schwab_sym, "subscribing to bars");
        self.streamer
            .subscribe(
                "CHART_EQUITY",
                vec![schwab_sym.as_str().to_string()],
                // Fields: 1=Open, 2=High, 3=Low, 4=Close, 5=Volume, 6=Timestamp, 7=Interval
                vec!["1".into(), "2".into(), "3".into(), "4".into(), "5".into(), "6".into(), "7".into()],
            )
            .await?;
        Ok(())
    }

    /// Unsubscribe from real-time data for a symbol.
    pub async fn unsubscribe(
        &self,
        symbol: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let schwab_sym = SchwabSymbol::new(symbol)?;
        self.streamer
            .unsubscribe("LEVELONE_EQUITIES", vec![schwab_sym.as_str().to_string()])
            .await?;
        self.streamer
            .unsubscribe("CHART_EQUITY", vec![schwab_sym.as_str().to_string()])
            .await?;
        Ok(())
    }

    /// Shut down the data client and its streamer.
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.streamer.shutdown().await?;
        Ok(())
    }
}
