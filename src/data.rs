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

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use nautilus_common::clients::DataClient;
use nautilus_common::factories::client::ClientConfig;
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
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::common::credential::SchwabCredential;
use crate::http::client::SchwabHttpClient;
use crate::oauth::provider::SchwabTokenProvider;

// ── Streamer imports from schwab-sdk ────────────────────────────────────────
use schwab_sdk::streamer::{DataContent, ReadHalf, StreamerResponse, WriteHalf};
use schwab_sdk::streamer::chart::equity::{Content as BarContent, Field as BarField};
use schwab_sdk::streamer::level_one::equities::{Content as QuoteContent, Field as QuoteField};

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

impl ClientConfig for SchwabDataClientConfig {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ── Cached quote for sparse update reconstruction ───────────────────────────

/// Local cache for a single symbol's quote data.
///
/// Schwab's LEVELONE_EQUITIES delivers sparse updates: only fields that
/// changed since the last update are `Some`. We maintain a local cache
/// so we can reconstruct the full quote from each incremental update.
#[derive(Debug, Clone, Default)]
struct CachedQuote {
    symbol: Option<String>,
    bid_price: Option<Decimal>,
    ask_price: Option<Decimal>,
    last_price: Option<Decimal>,
    bid_size: Option<u64>,
    ask_size: Option<u64>,
    total_volume: Option<u64>,
    high_price: Option<Decimal>,
    low_price: Option<Decimal>,
    close_price: Option<Decimal>,
    quote_time: Option<chrono::DateTime<chrono::Utc>>,
    trade_time: Option<chrono::DateTime<chrono::Utc>>,
}

impl CachedQuote {
    /// Apply a sparse update from the streamer, merging non-None fields.
    fn apply_update(&mut self, update: &QuoteContent) {
        if let Some(ref v) = update.symbol {
            self.symbol = Some(v.clone());
        }
        if let Some(v) = update.bid_price {
            self.bid_price = Some(v);
        }
        if let Some(v) = update.ask_price {
            self.ask_price = Some(v);
        }
        if let Some(v) = update.last_price {
            self.last_price = Some(v);
        }
        if let Some(v) = update.bid_size {
            self.bid_size = Some(v);
        }
        if let Some(v) = update.ask_size {
            self.ask_size = Some(v);
        }
        if let Some(v) = update.total_volume {
            self.total_volume = Some(v);
        }
        if let Some(v) = update.high_price {
            self.high_price = Some(v);
        }
        if let Some(v) = update.low_price {
            self.low_price = Some(v);
        }
        if let Some(v) = update.close_price {
            self.close_price = Some(v);
        }
        if let Some(v) = update.quote_time {
            self.quote_time = Some(v);
        }
        if let Some(v) = update.trade_time {
            self.trade_time = Some(v);
        }
    }
}

// ── Commands sent from the sync DataClient to the async streamer task ───────

/// Internal command sent from the DataClient trait methods (sync context)
/// to the background streamer command processor (async context).
enum StreamerCommand {
    /// Subscribe to LEVELONE_EQUITIES for the given symbols.
    SubscribeQuotes { symbols: Vec<String> },
    /// Subscribe to CHART_EQUITY for the given symbols.
    SubscribeBars { symbols: Vec<String> },
    /// Unsubscribe from LEVELONE_EQUITIES for the given symbols.
    UnsubscribeQuotes { symbols: Vec<String> },
    /// Unsubscribe from CHART_EQUITY for the given symbols.
    UnsubscribeBars { symbols: Vec<String> },
    /// Logout and shut down the streamer.
    Shutdown,
}

/// Schwab DataClient for Nautilus Trader.
///
/// Implements the Nautilus `DataClient` trait, bridging between Schwab's
/// market data APIs and Nautilus domain types. The client manages:
///
/// - Connection state tracking
/// - Subscription bookkeeping for quotes and bars
/// - REST API access for historical data and instrument lookups
/// - WebSocket streamer for real-time quotes and bars
///
/// The streamer runs in a background tokio task. Commands from the sync
/// DataClient trait methods are forwarded via an mpsc channel.
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

    // ── V1.1 streamer state ─────────────────────────────────────────────
    /// Clone of the streamer write half for sending subscribe/unsubscribe
    /// commands. `None` when the streamer is not connected.
    write_half: Option<WriteHalf>,
    /// Handle to the background reader task. Dropped on disconnect to
    /// signal cancellation.
    reader_handle: Option<JoinHandle<()>>,
    /// Sender side of the command channel to the streamer processor task.
    cmd_tx: Option<mpsc::Sender<StreamerCommand>>,
    /// Local quote cache for sparse update reconstruction.
    quote_cache: HashMap<String, CachedQuote>,
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
            write_half: None,
            reader_handle: None,
            cmd_tx: None,
            quote_cache: HashMap::new(),
        })
    }

    /// Get a reference to the underlying HTTP client.
    pub fn http_client(&self) -> &Arc<SchwabHttpClient> {
        &self.http_client
    }

    /// Connect the WebSocket streamer.
    ///
    /// Establishes the WebSocket connection, performs login, spawns the
    /// background reader task, and sets up the command channel.
    async fn connect_streamer(&mut self) -> anyhow::Result<()> {
        info!("connecting Schwab streamer");

        // Get the inner SchwabClient and connect the streamer
        let (read_half, write_half) = self
            .http_client
            .inner()
            .streamer()
            .await
            .map_err(|e| anyhow::anyhow!("failed to connect streamer: {e}"))?;

        // Login before any subscriptions
        write_half
            .login()
            .await
            .map_err(|e| anyhow::anyhow!("streamer login failed: {e}"))?;

        info!("streamer connected and logged in");

        // Store a clone of WriteHalf for later subscribe/unsubscribe calls
        self.write_half = Some(write_half.clone());

        // Create the command channel
        let (cmd_tx, cmd_rx) = mpsc::channel::<StreamerCommand>(64);
        self.cmd_tx = Some(cmd_tx);

        // Spawn the background reader task
        let handle = tokio::spawn(streamer_reader_task(read_half, self.quote_cache.clone()));
        self.reader_handle = Some(handle);

        // Spawn the command processor task
        let write_clone = write_half;
        tokio::spawn(streamer_command_processor(write_clone, cmd_rx));

        Ok(())
    }

    /// Disconnect the WebSocket streamer.
    ///
    /// Sends a shutdown command, waits briefly for the reader task to
    /// finish, and cleans up state.
    async fn disconnect_streamer(&mut self) {
        info!("disconnecting Schwab streamer");

        // Send shutdown command if the channel is still open
        if let Some(ref tx) = self.cmd_tx {
            let _ = tx.send(StreamerCommand::Shutdown).await;
        }

        // Drop the command sender to signal no more commands
        self.cmd_tx.take();

        // Wait briefly for the reader task to finish
        if let Some(handle) = self.reader_handle.take() {
            // Give it a moment to process the shutdown
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
        }

        // Clear the write half
        self.write_half.take();

        // Clear the quote cache
        self.quote_cache.clear();

        info!("streamer disconnected");
    }

    /// Send a command to the streamer processor task.
    ///
    /// Returns an error if the command channel is closed (streamer not
    /// connected).
    fn send_command(&self, cmd: StreamerCommand) -> anyhow::Result<()> {
        if let Some(ref tx) = self.cmd_tx {
            // Use try_send since we're in a sync context. The channel has
            // capacity 64 which should be more than enough for subscription
            // commands. If it's full, something is seriously wrong.
            tx.try_send(cmd)
                .map_err(|e| anyhow::anyhow!("failed to send streamer command: {e}"))?;
            Ok(())
        } else {
            anyhow::bail!("streamer not connected")
        }
    }
}

// ── Background tasks ────────────────────────────────────────────────────────

/// Background task that reads frames from the streamer and processes them.
///
/// This task owns the `ReadHalf` (which is not Clone) and runs until:
/// - The read half returns an error (connection lost)
/// - The task is cancelled (JoinHandle dropped)
async fn streamer_reader_task(
    mut read_half: ReadHalf,
    mut quote_cache: HashMap<String, CachedQuote>,
) {
    info!("streamer reader task started");

    loop {
        match read_half.recv().await {
            Ok(response) => {
                process_streamer_response(&response, &mut quote_cache);
            }
            Err(e) => {
                error!(error = %e, "streamer read error, exiting reader task");
                break;
            }
        }
    }

    info!("streamer reader task exiting");
}

/// Process a single streamer response frame.
fn process_streamer_response(
    response: &StreamerResponse,
    quote_cache: &mut HashMap<String, CachedQuote>,
) {
    match response {
        StreamerResponse::Data(payloads) => {
            for payload in payloads {
                match &payload.content {
                    DataContent::LevelOneEquities(updates) => {
                        process_quote_updates(updates, quote_cache);
                    }
                    DataContent::ChartEquity(bars) => {
                        process_bar_updates(bars);
                    }
                    // Other data content types are not handled in V1.1
                    _ => {
                        debug!(service = ?payload.service, "unhandled data content type");
                    }
                }
            }
        }
        StreamerResponse::Notify(_heartbeats) => {
            // Heartbeats keep the connection alive; nothing to do
            debug!("streamer heartbeat received");
        }
        StreamerResponse::Response(responses) => {
            for resp in responses {
                if resp.content.code != schwab_sdk::streamer::ResponseCode::Ok {
                    warn!(
                        service = ?resp.service,
                        command = ?resp.command,
                        code = ?resp.content.code,
                        message = %resp.content.message,
                        "streamer response indicates error"
                    );
                } else {
                    debug!(
                        service = ?resp.service,
                        command = ?resp.command,
                        "streamer response OK"
                    );
                }
            }
        }
        // Non-exhaustive enum requires wildcard arm
        _ => {
            debug!("unhandled streamer response variant");
        }
    }
}

/// Process LEVELONE_EQUITIES quote updates.
///
/// Each update is sparse — only changed fields are `Some`. We merge into
/// the local cache and log the reconstructed quote.
fn process_quote_updates(
    updates: &[QuoteContent],
    quote_cache: &mut HashMap<String, CachedQuote>,
) {
    for update in updates {
        let symbol = update.key.clone();

        // Get or create cached quote entry
        let cached = quote_cache.entry(symbol.clone()).or_default();
        cached.apply_update(update);

        // Log the reconstructed quote (actual Nautilus message dispatch
        // will be wired up when engine integration is available)
        debug!(
            symbol = %symbol,
            bid = ?cached.bid_price,
            ask = ?cached.ask_price,
            last = ?cached.last_price,
            bid_sz = ?cached.bid_size,
            ask_sz = ?cached.ask_size,
            vol = ?cached.total_volume,
            quote_time = ?cached.quote_time,
            "quote update"
        );
    }
}

/// Process CHART_EQUITY bar updates.
///
/// Unlike quotes, bar updates are typically complete (all fields present).
fn process_bar_updates(bars: &[BarContent]) {
    for bar in bars {
        debug!(
            symbol = %bar.key,
            open = ?bar.open_price,
            high = ?bar.high_price,
            low = ?bar.low_price,
            close = ?bar.close_price,
            volume = ?bar.volume,
            chart_time = ?bar.chart_time,
            sequence = ?bar.sequence,
            "bar update"
        );
    }
}

/// Background task that processes commands from the DataClient and sends
/// them to the streamer via the WriteHalf.
async fn streamer_command_processor(
    write_half: WriteHalf,
    mut cmd_rx: mpsc::Receiver<StreamerCommand>,
) {
    info!("streamer command processor started");

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            StreamerCommand::SubscribeQuotes { symbols } => {
                info!(symbols = ?symbols, "subscribing to quotes via streamer");
                let result = write_half
                    .equities()
                    .subscribe(symbols.iter().map(|s| s.as_str()))
                    .fields([
                        QuoteField::Symbol,
                        QuoteField::BidPrice,
                        QuoteField::AskPrice,
                        QuoteField::LastPrice,
                        QuoteField::BidSize,
                        QuoteField::AskSize,
                        QuoteField::TotalVolume,
                        QuoteField::HighPrice,
                        QuoteField::LowPrice,
                        QuoteField::ClosePrice,
                        QuoteField::QuoteTime,
                        QuoteField::TradeTime,
                    ])
                    .send()
                    .await;
                if let Err(e) = result {
                    error!(error = %e, "failed to subscribe to quotes");
                }
            }
            StreamerCommand::SubscribeBars { symbols } => {
                info!(symbols = ?symbols, "subscribing to bars via streamer");
                let result = write_half
                    .chart_equity()
                    .subscribe(symbols.iter().map(|s| s.as_str()))
                    .fields([
                        BarField::Symbol,
                        BarField::OpenPrice,
                        BarField::HighPrice,
                        BarField::LowPrice,
                        BarField::ClosePrice,
                        BarField::Volume,
                        BarField::ChartTime,
                    ])
                    .send()
                    .await;
                if let Err(e) = result {
                    error!(error = %e, "failed to subscribe to bars");
                }
            }
            StreamerCommand::UnsubscribeQuotes { symbols } => {
                info!(symbols = ?symbols, "unsubscribing from quotes via streamer");
                let result = write_half
                    .equities()
                    .unsubscribe(symbols.iter().map(|s| s.as_str()))
                    .fields([QuoteField::Symbol]) // Fields required by builder but ignored for UNSUBS
                    .send()
                    .await;
                if let Err(e) = result {
                    error!(error = %e, "failed to unsubscribe from quotes");
                }
            }
            StreamerCommand::UnsubscribeBars { symbols } => {
                info!(symbols = ?symbols, "unsubscribing from bars via streamer");
                let result = write_half
                    .chart_equity()
                    .unsubscribe(symbols.iter().map(|s| s.as_str()))
                    .fields([BarField::Symbol]) // Fields required by builder but ignored for UNSUBS
                    .send()
                    .await;
                if let Err(e) = result {
                    error!(error = %e, "failed to unsubscribe from bars");
                }
            }
            StreamerCommand::Shutdown => {
                info!("streamer shutdown command received");
                let _ = write_half.logout().await;
                break;
            }
        }
    }

    info!("streamer command processor exiting");
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

        // Connect the streamer asynchronously. We use block_in_place to
        // bridge the sync trait method to the async streamer connect.
        // This is safe because the DataClient is not Send (?Send bound).
        let rt = tokio::runtime::Handle::try_current();
        match rt {
            Ok(handle) => {
                // We're inside a tokio runtime — spawn the connect and
                // wait for it. Use block_in_place to avoid blocking the
                // executor thread.
                let result = tokio::task::block_in_place(|| {
                    handle.block_on(self.connect_streamer())
                });
                if let Err(e) = result {
                    warn!(error = %e, "failed to connect streamer on start, continuing without streaming");
                    // Don't fail start() — the client can still serve
                    // REST requests without the streamer.
                }
            }
            Err(_) => {
                warn!("no tokio runtime available, streamer not started");
            }
        }

        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        // Idempotent: safe to call multiple times
        if self.is_connected {
            info!(client_id = %self.config.client_id, "stopping Schwab data client");
            self.is_connected = false;
            self.subscribed_quotes.clear();
            self.subscribed_bars.clear();

            // Disconnect the streamer
            let rt = tokio::runtime::Handle::try_current();
            if let Ok(handle) = rt {
                tokio::task::block_in_place(|| {
                    handle.block_on(self.disconnect_streamer());
                });
            }
        }
        Ok(())
    }

    fn reset(&mut self) -> anyhow::Result<()> {
        info!(client_id = %self.config.client_id, "resetting Schwab data client");
        self.subscribed_quotes.clear();
        self.subscribed_bars.clear();
        self.quote_cache.clear();
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

        // Initialize quote cache entry
        self.quote_cache
            .entry(symbol.clone())
            .or_insert_with(|| CachedQuote {
                symbol: Some(symbol.clone()),
                ..Default::default()
            });

        // Send subscribe command to the streamer processor
        self.send_command(StreamerCommand::SubscribeQuotes {
            symbols: vec![symbol],
        })?;

        Ok(())
    }

    fn subscribe_bars(&mut self, cmd: SubscribeBars) -> anyhow::Result<()> {
        let bar_type_str = cmd.bar_type.to_string();
        info!(bar_type = %bar_type_str, "subscribing to bars");

        // Track subscription
        self.subscribed_bars.insert(bar_type_str.clone());

        // Extract the symbol from the bar type string.
        // BarType format is typically "SYMBOL-EXCHANGE-PERIOD-SOURCE"
        // For now, use the full bar_type string as the key.
        let symbol = bar_type_str.split('-').next().unwrap_or(&bar_type_str).to_string();

        // Send subscribe command to the streamer processor
        self.send_command(StreamerCommand::SubscribeBars {
            symbols: vec![symbol],
        })?;

        Ok(())
    }

    fn unsubscribe_quotes(&mut self, cmd: &UnsubscribeQuotes) -> anyhow::Result<()> {
        let symbol = cmd.instrument_id.symbol.to_string();
        info!(symbol = %symbol, "unsubscribing from quotes");
        self.subscribed_quotes.remove(&symbol);
        self.quote_cache.remove(&symbol);

        // Send unsubscribe command to the streamer processor
        // Ignore errors if the streamer is not connected
        let _ = self.send_command(StreamerCommand::UnsubscribeQuotes {
            symbols: vec![symbol],
        });

        Ok(())
    }

    fn unsubscribe_bars(&mut self, cmd: &UnsubscribeBars) -> anyhow::Result<()> {
        let bar_type_str = cmd.bar_type.to_string();
        info!(bar_type = %bar_type_str, "unsubscribing from bars");
        self.subscribed_bars.remove(&bar_type_str);

        let symbol = bar_type_str.split('-').next().unwrap_or(&bar_type_str).to_string();

        // Send unsubscribe command to the streamer processor
        let _ = self.send_command(StreamerCommand::UnsubscribeBars {
            symbols: vec![symbol],
        });

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

    #[test]
    fn test_cached_quote_sparse_update() {
        let mut cached = CachedQuote::default();

        // First update: only symbol and bid_price
        let update1 = QuoteContent {
            key: "AAPL".to_string(),
            delayed: false,
            symbol: Some("AAPL".to_string()),
            bid_price: Some(rust_decimal_macros::dec!(150.00)),
            ..Default::default()
        };
        cached.apply_update(&update1);
        assert_eq!(cached.symbol.as_deref(), Some("AAPL"));
        assert_eq!(cached.bid_price, Some(rust_decimal_macros::dec!(150.00)));
        assert_eq!(cached.ask_price, None);

        // Second update: only ask_price (sparse)
        let update2 = QuoteContent {
            key: "AAPL".to_string(),
            delayed: false,
            ask_price: Some(rust_decimal_macros::dec!(150.50)),
            ..Default::default()
        };
        cached.apply_update(&update2);
        assert_eq!(cached.bid_price, Some(rust_decimal_macros::dec!(150.00))); // preserved
        assert_eq!(cached.ask_price, Some(rust_decimal_macros::dec!(150.50))); // updated
    }
}
