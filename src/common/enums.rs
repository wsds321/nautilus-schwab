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

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_terminal() ───────────────────────────────────────────────────

    #[test]
    fn test_order_status_terminal_states() {
        assert!(SchwabOrderStatus::Filled.is_terminal());
        assert!(SchwabOrderStatus::Canceled.is_terminal());
        assert!(SchwabOrderStatus::Rejected.is_terminal());
        assert!(SchwabOrderStatus::Expired.is_terminal());
    }

    #[test]
    fn test_order_status_non_terminal_states() {
        assert!(!SchwabOrderStatus::Working.is_terminal());
        assert!(!SchwabOrderStatus::Accepted.is_terminal());
        assert!(!SchwabOrderStatus::PartiallyFilled.is_terminal());
        assert!(!SchwabOrderStatus::PendingCancel.is_terminal());
        assert!(!SchwabOrderStatus::PendingReplace.is_terminal());
        assert!(!SchwabOrderStatus::AwaitingParentOrder.is_terminal());
        assert!(!SchwabOrderStatus::AwaitingCondition.is_terminal());
        assert!(!SchwabOrderStatus::AwaitingManualReview.is_terminal());
        assert!(!SchwabOrderStatus::Unknown.is_terminal());
    }

    // ── is_active() ─────────────────────────────────────────────────────

    #[test]
    fn test_order_status_active_states() {
        assert!(SchwabOrderStatus::Working.is_active());
        assert!(SchwabOrderStatus::Accepted.is_active());
        assert!(SchwabOrderStatus::PartiallyFilled.is_active());
    }

    #[test]
    fn test_order_status_inactive_states() {
        assert!(!SchwabOrderStatus::Filled.is_active());
        assert!(!SchwabOrderStatus::Canceled.is_active());
        assert!(!SchwabOrderStatus::Rejected.is_active());
        assert!(!SchwabOrderStatus::Expired.is_active());
        assert!(!SchwabOrderStatus::PendingCancel.is_active());
        assert!(!SchwabOrderStatus::PendingReplace.is_active());
        assert!(!SchwabOrderStatus::AwaitingParentOrder.is_active());
        assert!(!SchwabOrderStatus::AwaitingCondition.is_active());
        assert!(!SchwabOrderStatus::AwaitingManualReview.is_active());
        assert!(!SchwabOrderStatus::Unknown.is_active());
    }

    // ── Terminal and active are disjoint ────────────────────────────────

    #[test]
    fn test_terminal_and_active_are_disjoint() {
        let all_statuses = [
            SchwabOrderStatus::AwaitingParentOrder,
            SchwabOrderStatus::AwaitingCondition,
            SchwabOrderStatus::AwaitingManualReview,
            SchwabOrderStatus::Accepted,
            SchwabOrderStatus::Working,
            SchwabOrderStatus::Rejected,
            SchwabOrderStatus::Canceled,
            SchwabOrderStatus::Filled,
            SchwabOrderStatus::PartiallyFilled,
            SchwabOrderStatus::Expired,
            SchwabOrderStatus::PendingCancel,
            SchwabOrderStatus::PendingReplace,
            SchwabOrderStatus::Unknown,
        ];
        for status in all_statuses {
            assert!(
                !(status.is_terminal() && status.is_active()),
                "{:?} should not be both terminal and active",
                status
            );
        }
    }

    // ── Serde round-trip ────────────────────────────────────────────────

    #[test]
    fn test_serde_roundtrip_all_variants() {
        let variants = [
            ("\"AWAITING_PARENT_ORDER\"", SchwabOrderStatus::AwaitingParentOrder),
            ("\"AWAITING_CONDITION\"", SchwabOrderStatus::AwaitingCondition),
            ("\"AWAITING_MANUAL_REVIEW\"", SchwabOrderStatus::AwaitingManualReview),
            ("\"ACCEPTED\"", SchwabOrderStatus::Accepted),
            ("\"WORKING\"", SchwabOrderStatus::Working),
            ("\"REJECTED\"", SchwabOrderStatus::Rejected),
            ("\"CANCELED\"", SchwabOrderStatus::Canceled),
            ("\"FILLED\"", SchwabOrderStatus::Filled),
            ("\"PARTIALLY_FILLED\"", SchwabOrderStatus::PartiallyFilled),
            ("\"EXPIRED\"", SchwabOrderStatus::Expired),
            ("\"PENDING_CANCEL\"", SchwabOrderStatus::PendingCancel),
            ("\"PENDING_REPLACE\"", SchwabOrderStatus::PendingReplace),
        ];
        for (json, expected) in variants {
            let deserialized: SchwabOrderStatus = serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("failed to deserialize {}: {}", json, e));
            assert_eq!(deserialized, expected, "mismatch for {}", json);

            // Round-trip: serialize back and re-deserialize
            let serialized = serde_json::to_string(&deserialized).unwrap();
            let re_deserialized: SchwabOrderStatus = serde_json::from_str(&serialized).unwrap();
            assert_eq!(re_deserialized, expected, "round-trip failed for {}", json);
        }
    }

    #[test]
    fn test_unknown_variant_fallback() {
        let json = r#""SOME_NEW_STATUS""#;
        let status: SchwabOrderStatus = serde_json::from_str(json).unwrap();
        assert!(matches!(status, SchwabOrderStatus::Unknown));
    }

    #[test]
    fn test_unknown_variant_is_neither_terminal_nor_active() {
        let json = r#""FUTURE_STATUS_V2""#;
        let status: SchwabOrderStatus = serde_json::from_str(json).unwrap();
        assert!(!status.is_terminal());
        assert!(!status.is_active());
    }

    // ── SchwabOrderType serde ───────────────────────────────────────────

    #[test]
    fn test_order_type_serde_roundtrip() {
        let variants = [
            ("\"MARKET\"", SchwabOrderType::Market),
            ("\"LIMIT\"", SchwabOrderType::Limit),
            ("\"STOP\"", SchwabOrderType::Stop),
            ("\"STOP_LIMIT\"", SchwabOrderType::StopLimit),
            ("\"TRAILING_STOP\"", SchwabOrderType::TrailingStop),
        ];
        for (json, expected) in variants {
            let deserialized: SchwabOrderType = serde_json::from_str(json).unwrap();
            assert_eq!(deserialized, expected);
            let serialized = serde_json::to_string(&deserialized).unwrap();
            let re_deserialized: SchwabOrderType = serde_json::from_str(&serialized).unwrap();
            assert_eq!(re_deserialized, expected);
        }
    }

    #[test]
    fn test_order_type_unknown_fallback() {
        let json = r#""NEW_ORDER_TYPE""#;
        let ot: SchwabOrderType = serde_json::from_str(json).unwrap();
        assert!(matches!(ot, SchwabOrderType::Unknown));
    }

    // ── SchwabOrderSide serde ───────────────────────────────────────────

    #[test]
    fn test_order_side_serde_roundtrip() {
        let variants = [
            ("\"BUY\"", SchwabOrderSide::Buy),
            ("\"SELL\"", SchwabOrderSide::Sell),
            ("\"BUY_TO_COVER\"", SchwabOrderSide::BuyToCover),
            ("\"SELL_SHORT\"", SchwabOrderSide::SellShort),
        ];
        for (json, expected) in variants {
            let deserialized: SchwabOrderSide = serde_json::from_str(json).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    #[test]
    fn test_order_side_unknown_fallback() {
        let json = r#""SELL_TO_OPEN""#;
        let side: SchwabOrderSide = serde_json::from_str(json).unwrap();
        assert!(matches!(side, SchwabOrderSide::Unknown));
    }

    // ── SchwabTimeInForce serde ─────────────────────────────────────────

    #[test]
    fn test_time_in_force_serde_roundtrip() {
        let variants = [
            ("\"DAY\"", SchwabTimeInForce::Day),
            ("\"GOOD_TILL_CANCEL\"", SchwabTimeInForce::GoodTillCancel),
            ("\"FILL_OR_KILL\"", SchwabTimeInForce::FillOrKill),
            ("\"IMMEDIATE_OR_CANCEL\"", SchwabTimeInForce::ImmediateOrCancel),
            ("\"END_OF_WEEK\"", SchwabTimeInForce::EndOfWeek),
            ("\"END_OF_MONTH\"", SchwabTimeInForce::EndOfMonth),
            ("\"NEXT_END_OF_MONTH\"", SchwabTimeInForce::NextEndOfMonth),
        ];
        for (json, expected) in variants {
            let deserialized: SchwabTimeInForce = serde_json::from_str(json).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    // ── SchwabAssetType serde ───────────────────────────────────────────

    #[test]
    fn test_asset_type_serde_roundtrip() {
        let variants = [
            ("\"EQUITY\"", SchwabAssetType::Equity),
            ("\"OPTION\"", SchwabAssetType::Option),
            ("\"FUTURE\"", SchwabAssetType::Future),
            ("\"FOREX\"", SchwabAssetType::Forex),
            ("\"BOND\"", SchwabAssetType::Bond),
            ("\"MUTUAL_FUND\"", SchwabAssetType::MutualFund),
        ];
        for (json, expected) in variants {
            let deserialized: SchwabAssetType = serde_json::from_str(json).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    #[test]
    fn test_asset_type_unknown_fallback() {
        let json = r#""CRYPTO""#;
        let at: SchwabAssetType = serde_json::from_str(json).unwrap();
        assert!(matches!(at, SchwabAssetType::Unknown));
    }
}
