//! WebSocket streamer for real-time market data from Schwab.
//!
//! Connects to Schwab's Streamer API for real-time quotes, bars,
//! and account activity updates. Handles authentication, reconnection,
//! and message dispatching.

pub mod client;
pub mod handler;
pub mod messages;

pub use client::SchwabStreamerClient;
