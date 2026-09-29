//! Schwab-specific enums and their mapping to Nautilus domain types.

use serde::{Deserialize, Serialize};

/// Schwab order status as returned by the API.
///
/// Maps to Nautilus `OrderStatus` at the execution client boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchwabOrderStatus {
    /// Order has been received but not yet processed
    AwaitingParentOrder,
    /// Order is being processed
    AwaitingCondition,
    /// Order is awaiting manual review
    AwaitingManualReview,
    /// Order has been accepted by the exchange
    Accepted,
    /// Order is active and working
    Working,
    /// Order has been rejected
    Rejected,
    /// Order was cancelled before any fills
    Canceled,
    /// Order has been fully filled
    Filled,
    /// Order has been partially filled
    PartiallyFilled,
    /// Order expired (e.g., day order past market close)
    Expired,
    /// Order is pending cancellation
    PendingCancel,
    /// Order is being replaced
    PendingReplace,
    /// Unknown or unrecognized status (forward-compatible fallback)
    #[serde(other)]
    Unknown,
}

/// Schwab order type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchwabOrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
    TrailingStop,
    #[serde(other)]
    Unknown,
}

/// Schwab order side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchwabOrderSide {
    Buy,
    Sell,
    BuyToCover,
    SellShort,
    #[serde(other)]
    Unknown,
}

/// Schwab order time-in-force.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchwabTimeInForce {
    Day,
    GoodTillCancel,
    FillOrKill,
    ImmediateOrCancel,
    EndOfWeek,
    EndOfMonth,
    NextEndOfMonth,
    #[serde(other)]
    Unknown,
}

/// Schwab asset type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchwabAssetType {
    Equity,
    Option,
    Future,
    Forex,
    Bond,
    MutualFund,
    #[serde(other)]
    Unknown,
}

impl SchwabOrderStatus {
    /// Check if this status represents a terminal (final) state.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Filled
                | Self::Canceled
                | Self::Rejected
                | Self::Expired
        )
    }

    /// Check if this status represents an active (working) state.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Working | Self::Accepted | Self::PartiallyFilled
        )
    }
}
