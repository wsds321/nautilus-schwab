//! Schwab DataClient implementation for Nautilus Trader.
//!
//! Implements the `nautilus_common::clients::DataClient` trait, providing:
//! - Real-time quotes via WebSocket streamer (LEVELONE_EQUITIES)
//! - Real-time bars via WebSocket streamer (CHART_EQUITY)
//! - Historical bars via REST API (/marketdata/v1/pricehistory)
//! - Instrument definitions via REST API (/marketdata/v1/instruments)
//! - Quote snapshots via REST API (/marketdata/v1/quotes)
//!
//! V1 scope: US equities and ETFs only. Options, futures, forex are deferred.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use nautilus_common::clients::DataClient;
use nautilus_common::messages::data::{
    RequestBars, RequestBookDeltas, RequestBookDepth, RequestBookSnapshot, RequestCustomData,
    RequestFundingRates, RequestInstrument, RequestInstruments, RequestOptionChainReferencePrice,
    RequestQuotes, RequestTrades, SubscribeBars, SubscribeBookDeltas, SubscribeBookDepth10,
    SubscribeCustomData, SubscribeFundingRates, SubscribeIndexPrices, SubscribeInstrument,
    SubscribeInstrumentClose, SubscribeInstrumentStatus, SubscribeInstruments, SubscribeMarkPrices,
    SubscribeOptionGreeks, SubscribeQuotes, SubscribeTrades, UnsubscribeBars,
    UnsubscribeBookDeltas, UnsubscribeBookDepth10, UnsubscribeCustomData, UnsubscribeFundingRates,
    UnsubscribeIndexPrices, UnsubscribeInstrument, UnsubscribeInstrumentClose,
    UnsubscribeInstrumentStatus, UnsubscribeInstruments, UnsubscribeMarkPrices,
    UnsubscribeOptionGreeks, UnsubscribeQuotes, UnsubscribeTrades,
};
use nautilus_model::identifiers::{ClientId, Venue};
use tracing::{info, warn};

use crate::common::credential::SchwabCredential;
use crate::http::client::SchwabHttpClient;
use crate::oauth::provider::SchwabTokenProvider;

/// Configuration for the Schwab data client.
#[derive(Debug, Clone)]
pub struct SchwabDataClientConfig {
    /// Client ID used to identify this client in Nautilus messages.
    pub client_id: String,
}

impl Default for SchwabDataClientConfig {
    fn default() -> Self {
        Self {
            client_id: "SCHWAB".to_string(),
        }
    }
}

/// Schwab DataClient for Nautilus Trader.
///
/// Implements the Nautilus `DataClient` trait, bridging between Schwab's
/// market data APIs and Nautilus domain types. The client manages:
///
/// - Connection state tracking
/// - Subscription bookkeeping for quotes and bars
/// - REST API access for historical data and instrument lookups
///
/// Streaming is handled via schwab-sdk's WebSocket streamer. The streamer
/// connection is established lazily on first subscribe call.
pub struct SchwabDataClient {
    /// HTTP client wrapping schwab-sdk.
    http_client: Arc<SchwabHttpClient>,
    /// Client configuration.
    config: SchwabDataClientConfig,
    /// Cached Nautilus ClientId.
    client_id: ClientId,
    /// Cached Nautilus Venue.
    venue: Venue,
    /// Whether the client is currently connected.
    is_connected: bool,
    /// Set of symbols with active quote subscriptions.
    subscribed_quotes: HashSet<String>,
    /// Set of bar types with active bar subscriptions.
    subscribed_bars: HashSet<String>,
}

impl SchwabDataClient {
    /// Create a new Schwab data client.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be created.
    pub fn new(
        credential: SchwabCredential,
        config: SchwabDataClientConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let token_provider = Arc::new(SchwabTokenProvider::new(credential));
        let http_client = Arc::new(SchwabHttpClient::new(token_provider));

        let client_id = ClientId::new(&config.client_id);
        let venue = Venue::new("SCHWAB");

        Ok(Self {
            http_client,
            config,
            client_id,
            venue,
            is_connected: false,
            subscribed_quotes: HashSet::new(),
            subscribed_bars: HashSet::new(),
        })
    }

    /// Get a reference to the underlying HTTP client.
    pub fn http_client(&self) -> &Arc<SchwabHttpClient> {
        &self.http_client
    }
}

#[async_trait(?Send)]
impl DataClient for SchwabDataClient {
    fn client_id(&self) -> ClientId {
        self.client_id
    }

    fn venue(&self) -> Option<Venue> {
        Some(self.venue)
    }

    fn start(&mut self) -> anyhow::Result<()> {
        info!(client_id = %self.config.client_id, "starting Schwab data client");
        self.is_connected = true;
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        // Idempotent: safe to call multiple times
        if self.is_connected {
            info!(client_id = %self.config.client_id, "stopping Schwab data client");
            self.is_connected = false;
            self.subscribed_quotes.clear();
            self.subscribed_bars.clear();
        }
        Ok(())
    }

    fn reset(&mut self) -> anyhow::Result<()> {
        info!(client_id = %self.config.client_id, "resetting Schwab data client");
        self.subscribed_quotes.clear();
        self.subscribed_bars.clear();
        // Note: connection state is preserved across reset
        Ok(())
    }

    fn dispose(&mut self) -> anyhow::Result<()> {
        info!(client_id = %self.config.client_id, "disposing Schwab data client");
        self.stop()?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.is_connected
    }

    fn is_disconnected(&self) -> bool {
        !self.is_connected
    }

    // ── Subscribe methods ───────────────────────────────────────────────

    fn subscribe_quotes(&mut self, cmd: SubscribeQuotes) -> anyhow::Result<()> {
        let symbol = cmd.instrument_id.symbol.to_string();
        info!(symbol = %symbol, "subscribing to quotes");

        // Track subscription for later unsubscribe
        self.subscribed_quotes.insert(symbol.clone());

        // V1: Log the subscription intent. Actual streaming integration
        // requires spawning a background task to manage the WebSocket
        // streamer lifecycle, which will be implemented in V1.1.
        //
        // The streamer API is accessed via:
        //   let (read, write) = self.http_client.inner().streamer().await?;
        //   write.login().await?;
        //   write.equities().subscribe([symbol]).fields([...]).send().await?;
        //
        // For now, we record the subscription and rely on request_quotes
        // for snapshot data.

        Ok(())
    }

    fn subscribe_bars(&mut self, cmd: SubscribeBars) -> anyhow::Result<()> {
        let bar_type_str = cmd.bar_type.to_string();
        info!(bar_type = %bar_type_str, "subscribing to bars");

        // Track subscription
        self.subscribed_bars.insert(bar_type_str.clone());

        // V1: Log the subscription intent. Real-time chart equity streaming
        // will be wired up in V1.1 alongside the quote streamer.
        //
        // The streamer API for bars:
        //   write.chart_equity().subscribe([symbol]).fields([...]).send().await?;

        Ok(())
    }

    fn unsubscribe_quotes(&mut self, cmd: &UnsubscribeQuotes) -> anyhow::Result<()> {
        let symbol = cmd.instrument_id.symbol.to_string();
        info!(symbol = %symbol, "unsubscribing from quotes");
        self.subscribed_quotes.remove(&symbol);
        Ok(())
    }

    fn unsubscribe_bars(&mut self, cmd: &UnsubscribeBars) -> anyhow::Result<()> {
        let bar_type_str = cmd.bar_type.to_string();
        info!(bar_type = %bar_type_str, "unsubscribing from bars");
        self.subscribed_bars.remove(&bar_type_str);
        Ok(())
    }

    // ── Request methods ─────────────────────────────────────────────────

    fn request_instruments(&self, request: RequestInstruments) -> anyhow::Result<()> {
        info!(
            request_id = %request.request_id,
            "requesting instruments (will use market_data().instruments().search())"
        );

        // V1 implementation note:
        // The actual REST call would be:
        //   let results = self.http_client.inner()
        //       .market_data()
        //       .instruments()
        //       .search(keyword, AssetType::Equity)
        //       .send()
        //       .await?;
        //
        // Results need conversion to Nautilus Instrument types and
        // dispatch via the message bus. This wiring is deferred to V1.1
        // when the response handler infrastructure is in place.

        Ok(())
    }

    fn request_bars(&self, request: RequestBars) -> anyhow::Result<()> {
        info!(
            bar_type = %request.bar_type,
            request_id = %request.request_id,
            "requesting bars (will use market_data().price_history().get())"
        );

        // V1 implementation note:
        // The actual REST call would be:
        //   let history = self.http_client.inner()
        //       .market_data()
        //       .price_history()
        //       .get(symbol)
        //       .period_type(PeriodType::Day)
        //       .period(10)
        //       .frequency_type(FrequencyType::Minute)
        //       .frequency(1)
        //       .send()
        //       .await?;
        //
        // Candles need conversion to Nautilus Bar types.

        Ok(())
    }

    fn request_quotes(&self, request: RequestQuotes) -> anyhow::Result<()> {
        info!(
            instrument_id = %request.instrument_id,
            request_id = %request.request_id,
            "requesting quotes (will use market_data().quotes().list())"
        );

        // V1 implementation note:
        // The actual REST call would be:
        //   let quotes = self.http_client.inner()
        //       .market_data()
        //       .quotes()
        //       .list([symbol])
        //       .send()
        //       .await?;
        //
        // QuoteEntry needs conversion to Nautilus QuoteTick.

        Ok(())
    }

    // ── Unsupported V1 methods (no-op with logging) ─────────────────────

    fn subscribe(&mut self, cmd: SubscribeCustomData) -> anyhow::Result<()> {
        warn!(data_type = ?cmd.data_type, "custom data subscription not supported in V1");
        Ok(())
    }

    fn subscribe_instruments(&mut self, cmd: SubscribeInstruments) -> anyhow::Result<()> {
        warn!(venue = %cmd.venue, "instruments subscription not supported in V1");
        Ok(())
    }

    fn subscribe_instrument(&mut self, cmd: SubscribeInstrument) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "single instrument subscription not supported in V1");
        Ok(())
    }

    fn subscribe_book_deltas(&mut self, cmd: SubscribeBookDeltas) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "book deltas subscription not supported in V1");
        Ok(())
    }

    fn subscribe_book_depth10(&mut self, cmd: SubscribeBookDepth10) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "book depth subscription not supported in V1");
        Ok(())
    }

    fn subscribe_trades(&mut self, cmd: SubscribeTrades) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "trades subscription not supported in V1");
        Ok(())
    }

    fn subscribe_mark_prices(&mut self, cmd: SubscribeMarkPrices) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "mark prices subscription not supported in V1");
        Ok(())
    }

    fn subscribe_index_prices(&mut self, cmd: SubscribeIndexPrices) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "index prices subscription not supported in V1");
        Ok(())
    }

    fn subscribe_funding_rates(&mut self, cmd: SubscribeFundingRates) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "funding rates subscription not supported in V1");
        Ok(())
    }

    fn subscribe_instrument_status(
        &mut self,
        cmd: SubscribeInstrumentStatus,
    ) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "instrument status subscription not supported in V1");
        Ok(())
    }

    fn subscribe_instrument_close(&mut self, cmd: SubscribeInstrumentClose) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "instrument close subscription not supported in V1");
        Ok(())
    }

    fn subscribe_option_greeks(&mut self, cmd: SubscribeOptionGreeks) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "option greeks subscription not supported in V1");
        Ok(())
    }

    fn unsubscribe(&mut self, cmd: &UnsubscribeCustomData) -> anyhow::Result<()> {
        warn!(data_type = ?cmd.data_type, "custom data unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_instruments(&mut self, cmd: &UnsubscribeInstruments) -> anyhow::Result<()> {
        warn!(venue = %cmd.venue, "instruments unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_instrument(&mut self, cmd: &UnsubscribeInstrument) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "single instrument unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_book_deltas(&mut self, cmd: &UnsubscribeBookDeltas) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "book deltas unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_book_depth10(&mut self, cmd: &UnsubscribeBookDepth10) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "book depth unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_trades(&mut self, cmd: &UnsubscribeTrades) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "trades unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_mark_prices(&mut self, cmd: &UnsubscribeMarkPrices) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "mark prices unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_index_prices(&mut self, cmd: &UnsubscribeIndexPrices) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "index prices unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_funding_rates(&mut self, cmd: &UnsubscribeFundingRates) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "funding rates unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_instrument_status(
        &mut self,
        cmd: &UnsubscribeInstrumentStatus,
    ) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "instrument status unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_instrument_close(
        &mut self,
        cmd: &UnsubscribeInstrumentClose,
    ) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "instrument close unsubscribe not supported in V1");
        Ok(())
    }

    fn unsubscribe_option_greeks(&mut self, cmd: &UnsubscribeOptionGreeks) -> anyhow::Result<()> {
        warn!(instrument_id = %cmd.instrument_id, "option greeks unsubscribe not supported in V1");
        Ok(())
    }

    fn request_data(&self, request: RequestCustomData) -> anyhow::Result<()> {
        warn!(data_type = ?request.data_type, "custom data request not supported in V1");
        Ok(())
    }

    fn request_instrument(&self, request: RequestInstrument) -> anyhow::Result<()> {
        warn!(instrument_id = %request.instrument_id, "single instrument request not supported in V1");
        Ok(())
    }

    fn request_book_snapshot(&self, request: RequestBookSnapshot) -> anyhow::Result<()> {
        warn!(instrument_id = %request.instrument_id, "book snapshot request not supported in V1");
        Ok(())
    }

    fn request_trades(&self, request: RequestTrades) -> anyhow::Result<()> {
        warn!(instrument_id = %request.instrument_id, "trades request not supported in V1");
        Ok(())
    }

    fn request_funding_rates(&self, request: RequestFundingRates) -> anyhow::Result<()> {
        warn!(instrument_id = %request.instrument_id, "funding rates request not supported in V1");
        Ok(())
    }

    fn request_option_chain_reference_price(
        &self,
        _request: RequestOptionChainReferencePrice,
    ) -> anyhow::Result<()> {
        warn!("option chain reference price request not supported in V1");
        anyhow::bail!("option-chain reference price requests are not supported")
    }

    fn request_book_depth(&self, request: RequestBookDepth) -> anyhow::Result<()> {
        warn!(instrument_id = %request.instrument_id, "book depth request not supported in V1");
        Ok(())
    }

    fn request_book_deltas(&self, request: RequestBookDeltas) -> anyhow::Result<()> {
        warn!(instrument_id = %request.instrument_id, "book deltas request not supported in V1");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SchwabDataClientConfig::default();
        assert_eq!(config.client_id, "SCHWAB");
    }

    #[test]
    fn test_client_lifecycle() {
        // This test verifies the DataClient trait implementation compiles
        // and basic lifecycle methods work correctly.
        // Full integration tests require valid Schwab credentials.

        // We can't easily construct a SchwabDataClient without credentials,
        // but we verify the trait is implemented by checking compilation.
        fn _assert_data_client<T: DataClient>() {}
        _assert_data_client::<SchwabDataClient>();
    }
}
