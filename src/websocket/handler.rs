//! Message handler for dispatching streamer events to Nautilus data callbacks.
//!
//! Translates Schwab streamer messages into Nautilus domain events
//! (QuoteTick, TradeTick, Bar) and routes them to the appropriate callbacks.

use crate::common::symbol::SchwabSymbol;
use crate::websocket::messages::{parse_bar_update, parse_quote_update, BarUpdate, QuoteUpdate};
use tracing::debug;

/// Handler for processing streamer data events.
///
/// Maintains state for incremental updates and converts Schwab-specific
/// message formats into Nautilus-compatible data structures.
pub struct StreamerDataHandler {
    /// Known instrument mappings for active subscriptions.
    subscribed_symbols: std::collections::HashSet<String>,
}

impl StreamerDataHandler {
    pub fn new() -> Self {
        Self {
            subscribed_symbols: std::collections::HashSet::new(),
        }
    }

    /// Register a symbol as actively subscribed.
    pub fn add_subscription(&mut self, symbol: &str) {
        self.subscribed_symbols.insert(symbol.to_uppercase());
    }

    /// Remove a symbol subscription.
    pub fn remove_subscription(&mut self, symbol: &str) {
        self.subscribed_symbols.remove(&symbol.to_uppercase());
    }

    /// Process a quote update from the streamer.
    ///
    /// Returns parsed quote data if the symbol is subscribed and valid.
    pub fn handle_quote(&self, content: &serde_json::Value) -> Option<ProcessedQuote> {
        let update = parse_quote_update(content)?;

        if !self.subscribed_symbols.contains(&update.symbol) {
            debug!(symbol = %update.symbol, "ignoring quote for unsubscribed symbol");
            return None;
        }

        // Convert to Nautilus-compatible format
        let instrument_id = match SchwabSymbol::new(&update.symbol) {
            Ok(sym) => sym.to_instrument_id(),
            Err(e) => {
                debug!(symbol = %update.symbol, error = %e, "invalid symbol in quote");
                return None;
            }
        };

        Some(ProcessedQuote {
            instrument_id,
            bid_price: update.bid_price,
            ask_price: update.ask_price,
            bid_size: update.bid_size,
            ask_size: update.ask_size,
            last_price: update.last_price,
            volume: update.volume,
            timestamp_ns: update.timestamp.map(|ts| ts * 1_000_000_000),
        })
    }

    /// Process a bar update from the streamer.
    ///
    /// Returns parsed bar data if the symbol is subscribed and valid.
    pub fn handle_bar(&self, content: &serde_json::Value) -> Option<ProcessedBar> {
        let update = parse_bar_update(content)?;

        if !self.subscribed_symbols.contains(&update.symbol) {
            return None;
        }

        let instrument_id = match SchwabSymbol::new(&update.symbol) {
            Ok(sym) => sym.to_instrument_id(),
            Err(_) => return None,
        };

        Some(ProcessedBar {
            instrument_id,
            open: update.open,
            high: update.high,
            low: update.low,
            close: update.close,
            volume: update.volume,
            timestamp_ns: update.timestamp * 1_000_000_000,
            interval_secs: update.interval_secs,
        })
    }
}

/// Processed quote ready for Nautilus conversion.
#[derive(Debug, Clone)]
pub struct ProcessedQuote {
    pub instrument_id: String,
    pub bid_price: Option<f64>,
    pub ask_price: Option<f64>,
    pub bid_size: Option<i64>,
    pub ask_size: Option<i64>,
    pub last_price: Option<f64>,
    pub volume: Option<i64>,
    pub timestamp_ns: Option<i64>,
}

/// Processed bar ready for Nautilus conversion.
#[derive(Debug, Clone)]
pub struct ProcessedBar {
    pub instrument_id: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: i64,
    pub timestamp_ns: i64,
    pub interval_secs: u32,
}
