//! # nautilus-schwab
//!
//! Charles Schwab adapter for [Nautilus Trader](https://nautilustrader.io/).
//!
//! This crate provides a Rust-native adapter that connects Nautilus Trader
//! to Charles Schwab's Trader API, Market Data API, and real-time streamer.
//!
//! ## Architecture
//!
//! The adapter translates between [`schwab-sdk`] types and Nautilus domain types,
//! implementing the standard `DataClient` and `ExecutionClient` traits.
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │           Nautilus Trader Engine                │
//! │  ┌──────────────┐   ┌────────────────────────┐  │
//! │  │ SchwabData    │   │ SchwabExecution        │  │
//! │  │ Client        │   │ Client                 │  │
//! │  └──────┬───────┘   └──────────┬─────────────┘  │
//! │         │                       │                │
//! │  ┌──────▼───────────────────────▼─────────────┐  │
//! │  │          schwab-sdk 0.5 (Rust crate)        │  │
//! │  │  accounts · orders · market_data · streamer │  │
//! │  └──────────────────┬─────────────────────────┘  │
//! │                     │                             │
//! │  ┌──────────────────▼─────────────────────────┐  │
//! │  │   TokenProvider impl (refresh-on-demand)   │  │
//! │  └────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! ## V1 Capability Matrix
//!
//! | Capability | Status | Notes |
//! |---|---|---|
//! | US Equities & ETFs | ✅ Planned | Primary target |
//! | Market Orders | ✅ Planned | |
//! | Limit Orders | ✅ Planned | Via `OrderRequest::buy_limit()` etc. |
//! | Stop Orders | ✅ Planned | |
//! | Real-time Quotes | ✅ Planned | Via `client.streamer()` |
//! | Real-time Bars | ✅ Planned | Via `client.streamer()` |
//! | Account Balance | ✅ Planned | |
//! | Position Reconciliation | ✅ Planned | Startup mass-status |
//! | OAuth Token Refresh | ✅ Planned | Automatic via TokenProvider |
//! | Options | ❌ V2 | |
//! | Futures | ❌ V2 | |
//! | Forex | ❌ V2 | |
//! | OCO/Bracket Orders | ❌ V2 | |
//! | Historical Bar Bulk | ❌ V2 | Single requests OK |

pub mod common;
pub mod http;
pub mod oauth;

pub mod data;
pub mod execution;
pub mod factories;

#[cfg(feature = "python")]
pub mod python;

// Re-export key types for convenience
pub use common::credential::SchwabCredential;
pub use common::symbol::SchwabSymbol;
pub use oauth::provider::SchwabTokenProvider;

/// Adapter venue identifier used in Nautilus InstrumentId values.
pub const VENUE: &str = "SCHWAB";
