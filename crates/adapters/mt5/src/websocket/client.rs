//! MT5 ZeroMQ client implementation.
//!
//! This client connects to MT5-ZeroMQ (JsonAPI.mq5) and handles:
//! - Market data streaming (via liveSocket)
//! - Order/position updates (via streamSocket)
//! - Command/response (via sysSocket)
//!
//! ## Architecture: Dedicated Thread Pattern
//!
//! ZMQ sockets run in a dedicated OS thread (not tokio), communicating with
//! async Rust via channels. This avoids Send/Sync issues with raw ZMQ pointers.

use super::{messages::*, parse::*};
use crate::common::parse_instrument_id;
use nautilus_core::time::get_atomic_clock_realtime;
use nautilus_model::{
    data::Data,
    identifiers::{AccountId, InstrumentId},
    instruments::{Instrument, InstrumentAny},
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Error types for MT5 client
#[derive(Debug, thiserror::Error)]
pub enum Mt5Error {
    #[error("ZeroMQ error: {0}")]
    Zmq(#[from] zmq::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Instrument not found: {0}")]
    InstrumentNotFound(InstrumentId),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not connected")]
    NotConnected,

    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

pub type Mt5Result<T> = Result<T, Mt5Error>;

/// Nautilus message wrapper for parsed MT5 data
#[derive(Debug, Clone)]
pub enum NautilusMessage {
    Data(Data),
    Raw(String),
}

/// Configuration for MT5 client
#[derive(Debug, Clone)]
pub struct Mt5ClientConfig {
    pub host: String,
    pub data_port: u16,
    pub live_port: u16,
    pub stream_port: u16,
    pub sys_port: u16,
}

impl Default for Mt5ClientConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            data_port: 2202,
            live_port: 2203,
            stream_port: 2204,
            sys_port: 2201,
        }
    }
}

/// MT5 ZeroMQ client
///
/// Uses a dedicated thread for ZMQ operations with channel-based communication
#[cfg_attr(feature = "python", pyo3::pyclass(module = "nautilus_pyo3.mt5"))]
#[derive(Clone)]
pub struct Mt5Client {
    pub(crate) config: Mt5ClientConfig,
    account_id: Option<AccountId>,
    pub(crate) instruments: Arc<Mutex<HashMap<String, InstrumentAny>>>,
    is_running: Arc<AtomicBool>,
    thread_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Mt5Client {
    /// Create a new MT5 client
    ///
    /// # Arguments
    /// * `config` - Client configuration
    /// * `account_id` - Optional account ID
    pub fn new(config: Mt5ClientConfig, account_id: Option<AccountId>) -> Self {
        Self {
            config,
            account_id,
            instruments: Arc::new(Mutex::new(HashMap::new())),
            is_running: Arc::new(AtomicBool::new(false)),
            thread_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Create client with default configuration
    pub fn with_defaults(account_id: Option<AccountId>) -> Self {
        Self::new(Mt5ClientConfig::default(), account_id)
    }

    /// Add instrument to cache
    pub fn add_instrument(&mut self, instrument: InstrumentAny) {
        let symbol = instrument.id().symbol.as_str().to_string();
        self.instruments.lock().unwrap().insert(symbol, instrument);
    }

    /// Check if client is connected and running
    pub fn is_active(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }

    /// Subscribe to symbols via MT5 CONFIG action
    ///
    /// This sends CONFIG messages to MT5 to start streaming data
    pub fn subscribe(&self, symbols: Vec<String>) -> Mt5Result<()> {
        if !self.is_active() {
            return Err(Mt5Error::NotConnected);
        }

        // Create a temporary socket for subscription
        // In production, you'd want to keep sys_socket in the struct
        let context = zmq::Context::new();
        let socket = context.socket(zmq::REQ)?;
        socket.connect(&format!("tcp://{}:{}", self.config.host, self.config.sys_port))?;

        for symbol in symbols {
            let config = Mt5ConfigRequest::new(&symbol, crate::common::Mt5TimeFrame::M1);
            let json = serde_json::to_string(&config)?;

            socket.send(&json, 0)?;
            let response = socket.recv_string(0)?.map_err(|e| {
                Mt5Error::Connection(format!("Invalid UTF-8 in response: {:?}", e))
            })?;
            tracing::debug!("MT5 CONFIG response for {}: {}", symbol, response);
        }

        Ok(())
    }

    /// Start streaming messages
    ///
    /// Spawns a dedicated thread for ZMQ operations and returns an async stream
    pub fn stream(&self) -> UnboundedReceiverStream<NautilusMessage> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Mark as running
        self.is_running.store(true, Ordering::Relaxed);

        // Clone everything we need for the thread
        let config = self.config.clone();
        let instruments = Arc::clone(&self.instruments);
        let is_running = Arc::clone(&self.is_running);
        let account_id = self.account_id;

        // Spawn dedicated thread for ZMQ operations
        let handle = std::thread::spawn(move || {
            if let Err(e) = Self::zmq_thread_loop(config, instruments, is_running, tx, account_id) {
                tracing::error!("ZMQ thread error: {}", e);
            }
        });

        *self.thread_handle.lock().unwrap() = Some(handle);

        UnboundedReceiverStream::new(rx)
    }

    /// Stop streaming and disconnect
    pub fn disconnect(&self) {
        self.is_running.store(false, Ordering::Relaxed);

        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.lock().unwrap().take() {
            let _ = handle.join();
        }

        tracing::info!("Disconnected from MT5-ZeroMQ");
    }

    /// ZMQ thread loop (runs in dedicated OS thread)
    ///
    /// This is where ZMQ sockets live - they never leave this thread!
    fn zmq_thread_loop(
        config: Mt5ClientConfig,
        instruments: Arc<Mutex<HashMap<String, InstrumentAny>>>,
        is_running: Arc<AtomicBool>,
        tx: mpsc::UnboundedSender<NautilusMessage>,
        account_id: Option<AccountId>,
    ) -> Mt5Result<()> {
        // Create ZMQ context and sockets (they live in this thread only!)
        let context = zmq::Context::new();

        // Data socket (PULL) - receives command responses from data_port (2202)
        let data_socket = context.socket(zmq::PULL)?;
        data_socket.connect(&format!("tcp://{}:{}", config.host, config.data_port))?;

        // Live socket (PULL) - receives tick/bar streaming data from live_port (2203)
        let live_socket = context.socket(zmq::PULL)?;
        live_socket.connect(&format!("tcp://{}:{}", config.host, config.live_port))?;

        // Stream socket (PULL) - receives trade confirmations from stream_port (2204)
        let stream_socket = context.socket(zmq::PULL)?;
        stream_socket.connect(&format!("tcp://{}:{}", config.host, config.stream_port))?;

        tracing::info!(
            "ZMQ thread connected to MT5-ZeroMQ at {}:{},{},{}",
            config.host,
            config.data_port,
            config.live_port,
            config.stream_port
        );

        // Main polling loop
        while is_running.load(Ordering::Relaxed) {
            // Poll data socket (non-blocking)
            // This receives responses to commands sent via sys_port
            if let Ok(msg_bytes) = data_socket.recv_bytes(zmq::DONTWAIT) {
                match Self::handle_data_message(&msg_bytes, &instruments, account_id) {
                    Ok(Some(nautilus_msg)) => {
                        if tx.send(nautilus_msg).is_err() {
                            tracing::warn!("Channel closed, stopping ZMQ thread");
                            break;
                        }
                    }
                    Err(e) => tracing::error!("Error handling data message: {}", e),
                    _ => {}
                }
            }

            // Poll live socket (non-blocking)
            if let Ok(msg_bytes) = live_socket.recv_bytes(zmq::DONTWAIT) {
                match Self::handle_live_message(&msg_bytes, &instruments, account_id) {
                    Ok(Some(nautilus_msg)) => {
                        if tx.send(nautilus_msg).is_err() {
                            tracing::warn!("Channel closed, stopping ZMQ thread");
                            break;
                        }
                    }
                    Err(e) => tracing::error!("Error handling live message: {}", e),
                    _ => {}
                }
            }

            // Poll stream socket (non-blocking)
            if let Ok(msg_bytes) = stream_socket.recv_bytes(zmq::DONTWAIT) {
                match Self::handle_stream_message(&msg_bytes, &instruments, account_id) {
                    Ok(Some(nautilus_msg)) => {
                        if tx.send(nautilus_msg).is_err() {
                            tracing::warn!("Channel closed, stopping ZMQ thread");
                            break;
                        }
                    }
                    Err(e) => tracing::error!("Error handling stream message: {}", e),
                    _ => {}
                }
            }

            // Small sleep to prevent busy-waiting (1ms)
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        tracing::info!("ZMQ thread stopped");
        Ok(())
    }

    /// Handle message from liveSocket (tick data)
    fn handle_live_message(
        msg_bytes: &[u8],
        instruments: &Arc<Mutex<HashMap<String, InstrumentAny>>>,
        account_id: Option<AccountId>,
    ) -> Mt5Result<Option<NautilusMessage>> {
        let json_str = String::from_utf8_lossy(msg_bytes);
        let msg: Mt5TickMsg = serde_json::from_str(&json_str)?;

        // Get instrument from cache
        let instruments = instruments.lock().unwrap();
        let instrument = instruments
            .get(msg.symbol.as_str())
            .ok_or_else(|| {
                Mt5Error::InstrumentNotFound(parse_instrument_id(msg.symbol.as_str()).unwrap())
            })?;

        let ts_init = get_atomic_clock_realtime().get_time_ns();

        // Parse to QuoteTick (better for forex)
        let quote = parse_mt5_tick_to_quote(&msg, instrument, ts_init)
            .map_err(|e| Mt5Error::Parse(e.to_string()))?;

        Ok(Some(NautilusMessage::Data(Data::Quote(quote))))
    }

    /// Handle message from dataSocket (command responses)
    fn handle_data_message(
        msg_bytes: &[u8],
        _instruments: &Arc<Mutex<HashMap<String, InstrumentAny>>>,
        _account_id: Option<AccountId>,
    ) -> Mt5Result<Option<NautilusMessage>> {
        let json_str = String::from_utf8_lossy(msg_bytes);

        // For now, just return raw message
        // TODO: Parse specific responses (ACCOUNT, POSITIONS, ORDERS, etc.)
        Ok(Some(NautilusMessage::Raw(json_str.to_string())))
    }

    /// Handle message from streamSocket (orders/positions)
    fn handle_stream_message(
        msg_bytes: &[u8],
        instruments: &Arc<Mutex<HashMap<String, InstrumentAny>>>,
        account_id: Option<AccountId>,
    ) -> Mt5Result<Option<NautilusMessage>> {
        let json_str = String::from_utf8_lossy(msg_bytes);

        // For now, just return raw message
        // TODO: Parse order/position updates
        Ok(Some(NautilusMessage::Raw(json_str.to_string())))
    }

    /// Wait until client is active or timeout
    ///
    /// Polls `is_active()` until true or timeout expires
    pub async fn wait_until_active(&self, timeout_secs: f64) -> Mt5Result<()> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs_f64(timeout_secs);

        while !self.is_active() {
            if start.elapsed() > timeout {
                return Err(Mt5Error::Connection(format!(
                    "Timeout waiting for connection after {timeout_secs}s"
                )));
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        Ok(())
    }

    /// Send request to MT5 sysSocket and get response
    ///
    /// This is a blocking request-response pattern (REQ/REP)
    fn send_sys_request(&self, request: &str) -> Mt5Result<String> {
        let context = zmq::Context::new();
        let socket = context.socket(zmq::REQ)?;
        socket.connect(&format!("tcp://{}:{}", self.config.host, self.config.sys_port))?;

        // Set timeout to prevent hanging
        socket.set_rcvtimeo(5000)?; // 5 seconds
        socket.set_sndtimeo(5000)?;

        socket.send(request, 0)?;
        let response = socket.recv_string(0)?.map_err(|e| {
            Mt5Error::Connection(format!("Invalid UTF-8 in response: {:?}", e))
        })?;

        Ok(response)
    }

    /// Receive response from MT5 dataSocket
    ///
    /// This receives a PULL message from data_port (async responses)
    fn receive_data_response(&self) -> Mt5Result<String> {
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PULL)?;
        socket.connect(&format!("tcp://{}:{}", self.config.host, self.config.data_port))?;

        // Set timeout for receiving
        socket.set_rcvtimeo(10000)?; // 10 seconds for large responses

        let response = socket.recv_string(0)?.map_err(|e| {
            Mt5Error::Connection(format!("Invalid UTF-8 in data response: {:?}", e))
        })?;

        Ok(response)
    }

    /// Request available instruments from MT5
    ///
    /// Sends SYMBOL_INFO action to query symbol specifications
    pub async fn request_instruments(&self) -> Mt5Result<Vec<InstrumentAny>> {
        // Build SYMBOL_INFO request (no symbol specified = get all symbols)
        let request = serde_json::json!({
            "action": "SYMBOL_INFO"
        });

        let request_str = serde_json::to_string(&request)?;

        // Clone self to move into spawn_blocking
        let client = self.clone();

        // Send via blocking sysSocket
        let ack_response = tokio::task::spawn_blocking(move || client.send_sys_request(&request_str))
            .await
            .map_err(|e| Mt5Error::Connection(format!("Task join error: {}", e)))??;

        tracing::debug!("SYMBOL_INFO ACK: {}", ack_response);

        // Receive actual response on data_socket
        let client_clone = self.clone();
        let response = tokio::task::spawn_blocking(move || client_clone.receive_data_response())
            .await
            .map_err(|e| Mt5Error::Connection(format!("Task join error: {}", e)))??;

        tracing::debug!("SYMBOL_INFO response received: {} bytes", response.len());

        // Parse response as Mt5SymbolInfoResponse
        let symbol_info_response: Mt5SymbolInfoResponse = serde_json::from_str(&response)
            .map_err(|e| Mt5Error::Parse(format!("Failed to parse SYMBOL_INFO response: {}", e)))?;

        if symbol_info_response.error {
            return Err(Mt5Error::Parse("SYMBOL_INFO returned error=true".to_string()));
        }

        // Convert each Mt5SymbolInfo to InstrumentAny
        use nautilus_core::UnixNanos;
        use nautilus_model::identifiers::Venue;

        let ts_init = UnixNanos::default();
        let ts_event = UnixNanos::default();
        let venue = Venue::new("MT5");

        let mut instruments = Vec::new();
        for symbol_info in symbol_info_response.symbols {
            match super::parse::parse_mt5_symbol_info_to_instrument(&symbol_info, &venue, ts_event, ts_init) {
                Ok(instrument) => {
                    tracing::info!("Loaded instrument: {}", symbol_info.symbol);
                    instruments.push(instrument);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse instrument {}: {}", symbol_info.symbol, e);
                    // Continue with other instruments
                }
            }
        }

        tracing::info!("Successfully loaded {} instruments from MT5", instruments.len());

        Ok(instruments)
    }

    /// Request account state from MT5
    ///
    /// Sends ACCOUNT action to query balance, margin, etc.
    pub async fn request_account_state(&self) -> Mt5Result<Mt5AccountMsg> {
        let request = serde_json::json!({
            "action": "ACCOUNT"
        });

        let request_str = serde_json::to_string(&request)?;
        let client = self.clone();

        let response = tokio::task::spawn_blocking(move || client.send_sys_request(&request_str))
            .await
            .map_err(|e| Mt5Error::Connection(format!("Task join error: {}", e)))??;

        let account_msg: Mt5AccountMsg = serde_json::from_str(&response)?;
        Ok(account_msg)
    }

    /// Request open orders from MT5
    ///
    /// Sends ORDERS action to query pending orders
    pub async fn request_orders(&self) -> Mt5Result<Vec<Mt5OrderMsg>> {
        let request = serde_json::json!({
            "action": "ORDERS"
        });

        let request_str = serde_json::to_string(&request)?;
        let client = self.clone();

        let response = tokio::task::spawn_blocking(move || client.send_sys_request(&request_str))
            .await
            .map_err(|e| Mt5Error::Connection(format!("Task join error: {}", e)))??;

        // Parse response - expected format: {"orders": [...]}
        let response_json: serde_json::Value = serde_json::from_str(&response)?;
        let orders_array = response_json["orders"]
            .as_array()
            .ok_or_else(|| Mt5Error::Parse("Missing 'orders' array in response".to_string()))?;

        let mut orders = Vec::new();
        for order_val in orders_array {
            let order: Mt5OrderMsg = serde_json::from_value(order_val.clone())?;
            orders.push(order);
        }

        Ok(orders)
    }

    /// Request open positions from MT5
    ///
    /// Sends POSITIONS action to query open positions
    pub async fn request_positions(&self) -> Mt5Result<Vec<Mt5PositionMsg>> {
        let request = serde_json::json!({
            "action": "POSITIONS"
        });

        let request_str = serde_json::to_string(&request)?;
        let client = self.clone();

        let response = tokio::task::spawn_blocking(move || client.send_sys_request(&request_str))
            .await
            .map_err(|e| Mt5Error::Connection(format!("Task join error: {}", e)))??;

        // Parse response - expected format: {"positions": [...]}
        let response_json: serde_json::Value = serde_json::from_str(&response)?;
        let positions_array = response_json["positions"]
            .as_array()
            .ok_or_else(|| Mt5Error::Parse("Missing 'positions' array in response".to_string()))?;

        let mut positions = Vec::new();
        for pos_val in positions_array {
            let position: Mt5PositionMsg = serde_json::from_value(pos_val.clone())?;
            positions.push(position);
        }

        Ok(positions)
    }

    /// Submit a trade order to MT5
    ///
    /// Sends TRADE action with order parameters
    pub async fn submit_order(&self, request: Mt5TradeRequest) -> Mt5Result<Mt5TradeResponseMsg> {
        let request_str = serde_json::to_string(&request)?;
        let client = self.clone();

        let response = tokio::task::spawn_blocking(move || client.send_sys_request(&request_str))
            .await
            .map_err(|e| Mt5Error::Connection(format!("Task join error: {}", e)))??;

        let trade_response: Mt5TradeResponseMsg = serde_json::from_str(&response)?;
        Ok(trade_response)
    }

    /// Cancel an order in MT5
    ///
    /// Sends DELETE action with order ticket
    pub async fn cancel_order(&self, ticket: i64) -> Mt5Result<Mt5TradeResponseMsg> {
        let request = serde_json::json!({
            "action": "DELETE",
            "order": ticket
        });

        let request_str = serde_json::to_string(&request)?;
        let client = self.clone();

        let response = tokio::task::spawn_blocking(move || client.send_sys_request(&request_str))
            .await
            .map_err(|e| Mt5Error::Connection(format!("Task join error: {}", e)))??;

        let trade_response: Mt5TradeResponseMsg = serde_json::from_str(&response)?;
        Ok(trade_response)
    }
}

impl Drop for Mt5Client {
    fn drop(&mut self) {
        self.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_client() {
        let client = Mt5Client::with_defaults(None);
        assert!(!client.is_active());
    }

    #[test]
    fn test_config_default() {
        let config = Mt5ClientConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.live_port, 2203);
        assert_eq!(config.stream_port, 2204);
        assert_eq!(config.sys_port, 2201);
    }

    #[test]
    fn test_mt5_error_display() {
        let error = Mt5Error::Connection("test error".to_string());
        assert_eq!(error.to_string(), "Connection error: test error");
    }
}
