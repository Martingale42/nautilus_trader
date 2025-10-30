//! WebSocket module (actually ZeroMQ for MT5).
//!
//! This module is named "websocket" to maintain consistency with other NautilusTrader adapters,
//! but it actually uses ZeroMQ to communicate with MT5-ZeroMQ (JsonAPI.mq5).

pub mod client;
pub mod messages;
pub mod parse;

pub use client::Mt5Client;
pub use messages::*;
