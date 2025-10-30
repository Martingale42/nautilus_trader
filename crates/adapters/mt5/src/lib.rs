//! MetaTrader 5 adapter for NautilusTrader.
//!
//! This adapter provides integration with MetaTrader 5 via ZeroMQ using the MT5-ZeroMQ bridge.
//! It follows the same patterns as other NautilusTrader adapters (e.g., Coinbase, Bybit).
//!
//! # Architecture
//!
//! - Uses ZeroMQ to connect to MT5-ZeroMQ (JsonAPI.mq5)
//! - Parses native MT5 JSON format to NautilusTrader domain types in Rust
//! - Exposes functionality to Python via PyO3 bindings
//!
//! # Features
//!
//! - Real-time market data (ticks, quotes)
//! - Order execution (market, limit, stop orders)
//! - Position management
//! - Historical data retrieval

#![allow(dead_code)] // Remove in production
#![allow(unused_variables)] // Remove in production

pub mod common;
pub mod websocket;

#[cfg(feature = "python")]
pub mod python;

// Re-exports
pub use websocket::client::Mt5Client;
pub use websocket::messages::*;
