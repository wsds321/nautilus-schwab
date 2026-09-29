//! WebSocket message types for Schwab Streamer API.
//!
//! The Schwab streamer uses a JSON-based protocol with typed messages
//! for authentication, subscriptions, and data delivery.

use serde::{Deserialize, Serialize};

/// Outbound request message to the streamer.
#[derive(Debug, Clone, Serialize)]
pub struct StreamerRequest {
    /// Request ID for correlation.
    pub reqid: u64,
    /// Service name (e.g., "ADMIN", "LEVELONE_EQUITIES", "CHART_EQUITY").
    pub service: String,
    /// Command type.
    pub command: StreamerCommand,
    /// Account identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Service-specific parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// Streamer command types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StreamerCommand {
    Login,
    Logout,
    Subscribe,
    Unsubscribe,
    Add,
    View,
}

/// Inbound response message from the streamer.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamerResponse {
    /// Response type discriminator.
    #[serde(rename = "type")]
    pub response_type: String,
    /// Array of content entries.
    #[serde(default)]
    pub content: Vec<serde_json::Value>,
    /// Notification messages.
    #[serde(default)]
    pub notify: Vec<StreamerNotification>,
}

/// Notification from the streamer (login status, errors, etc.).
#[derive(Debug, Clone, Deserialize)]
pub struct StreamerNotification {
    #[serde(rename = "heartbeat")]
    pub heartbeat: Option<i64>,
    #[serde(rename = "statusCode")]
    pub status_code: Option<u32>,
    #[serde(rename = "statusMsg")]
    pub status_msg: Option<String>,
}

/// Parsed quote update from LEVELONE_EQUITIES subscription.
#[derive(Debug, Clone)]
pub struct QuoteUpdate {
    pub symbol: String,
    pub bid_price: Option<f64>,
    pub ask_price: Option<f64>,
    pub last_price: Option<f64>,
    pub bid_size: Option<i64>,
    pub ask_size: Option<i64>,
    pub volume: Option<i64>,
    pub timestamp: Option<i64>,
}

/// Parsed bar/candle update from CHART_EQUITY subscription.
#[derive(Debug, Clone)]
pub struct BarUpdate {
    pub symbol: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: i64,
    pub timestamp: i64,
    /// Bar interval in seconds.
    pub interval_secs: u32,
}

/// Parse a raw streamer content entry into a typed update.
pub fn parse_quote_update(content: &serde_json::Value) -> Option<QuoteUpdate> {
    let obj = content.as_object()?;
    Some(QuoteUpdate {
        symbol: obj.get("key")?.as_str()?.to_string(),
        bid_price: obj.get("1").and_then(|v| v.as_f64()),
        ask_price: obj.get("2").and_then(|v| v.as_f64()),
        last_price: obj.get("3").and_then(|v| v.as_f64()),
        bid_size: obj.get("4").and_then(|v| v.as_i64()),
        ask_size: obj.get("5").and_then(|v| v.as_i64()),
        volume: obj.get("6").and_then(|v| v.as_i64()),
        timestamp: obj.get("7").and_then(|v| v.as_i64()),
    })
}

/// Parse a raw streamer content entry into a bar update.
pub fn parse_bar_update(content: &serde_json::Value) -> Option<BarUpdate> {
    let obj = content.as_object()?;
    Some(BarUpdate {
        symbol: obj.get("key")?.as_str()?.to_string(),
        open: obj.get("1").and_then(|v| v.as_f64())?,
        high: obj.get("2").and_then(|v| v.as_f64())?,
        low: obj.get("3").and_then(|v| v.as_f64())?,
        close: obj.get("4").and_then(|v| v.as_f64())?,
        volume: obj.get("5").and_then(|v| v.as_i64())?,
        timestamp: obj.get("6").and_then(|v| v.as_i64())?,
        interval_secs: obj.get("7").and_then(|v| v.as_u64()).unwrap_or(60) as u32,
    })
}
