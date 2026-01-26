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

use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

use nautilus_core::time::get_atomic_clock_realtime;
use nautilus_model::{
    data::{BarType, Data},
    enums::{OrderSide, OrderType},
    identifiers::{AccountId, ClientOrderId, InstrumentId, VenueOrderId},
    instruments::{Instrument, InstrumentAny},
    types::{Price, Quantity},
};
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{messages::*, parse::*};
use crate::common::parse_instrument_id;

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
///
/// Similar to Bybit's `BybitWebSocketMessage`, this enum provides typed variants
/// for different MT5 message types, enabling proper handling in Python bindings.
#[derive(Debug, Clone)]
pub enum NautilusMessage {
    /// Fully parsed Nautilus data (QuoteTick, Bar, etc.)
    Data(Data),
    /// Trade response from MT5 (order submission result)
    TradeResponse(Mt5TradeResponseMsg),
    /// Order update from stream socket
    OrderUpdate(Mt5OrderMsg),
    /// Position update from stream socket
    PositionUpdate(Mt5PositionMsg),
    /// Raw/unhandled message (for debugging)
    Raw(String),
}

/// Commands sent to ZMQ thread for request-response operations
///
/// This ensures all socket I/O happens in a single thread,
/// preventing conflicts between the polling loop and request-response operations
pub enum ZmqCommand {
    /// Request account state (balance, margin, etc.)
    RequestAccountState {
        response_tx: oneshot::Sender<Mt5Result<String>>,
    },
    /// Request instruments (symbol specifications)
    RequestInstruments {
        response_tx: oneshot::Sender<Mt5Result<String>>,
    },
    /// Submit a new order to MT5
    SubmitOrder {
        instrument_id: InstrumentId,
        client_order_id: ClientOrderId,
        order_side: OrderSide,
        order_type: OrderType,
        quantity: Quantity,
        price: Option<Price>,
        sl: Option<Price>,
        tp: Option<Price>,
        comment: String,
        response_tx: oneshot::Sender<Mt5Result<String>>,
    },
    /// Cancel an existing order
    CancelOrder {
        venue_order_id: VenueOrderId,
        response_tx: oneshot::Sender<Mt5Result<String>>,
    },
    /// Modify an existing order
    ModifyOrder {
        venue_order_id: VenueOrderId,
        price: Option<Price>,
        quantity: Option<Quantity>,
        sl: Option<Price>,
        tp: Option<Price>,
        response_tx: oneshot::Sender<Mt5Result<String>>,
    },
    /// Request historical data (bars or ticks)
    RequestHistory {
        symbol: String,
        timeframe: String,
        start: Option<i64>,
        end: Option<i64>,
        count: Option<i32>,
        response_tx: oneshot::Sender<Mt5Result<String>>,
    },
    /// Request trade/deal history from account
    RequestTradeHistory {
        response_tx: oneshot::Sender<Mt5Result<String>>,
    },
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
    account_id_str: Option<String>, // Store as string, construct AccountId when needed
    pub(crate) instruments: Arc<Mutex<HashMap<String, InstrumentAny>>>,
    pub(crate) bar_types: Arc<Mutex<HashMap<(String, String), String>>>, // (symbol, timeframe) -> bar_type_str
    is_running: Arc<AtomicBool>,
    thread_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Mutex to serialize request-response cycles on shared sysSocket/dataSocket
    /// Prevents race condition when multiple clients use the same Mt5Client instance
    request_response_lock: Arc<TokioMutex<()>>,
    /// Channel for sending commands to ZMQ thread (request-response operations)
    command_tx: mpsc::UnboundedSender<ZmqCommand>,
    /// Channel for receiving commands in ZMQ thread (created but stored for cleanup)
    command_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<ZmqCommand>>>>,
}

impl Mt5Client {
    /// Create a new MT5 client
    ///
    /// # Arguments
    /// * `config` - Client configuration
    /// * `account_id` - Optional account ID (just the account number without venue prefix)
    pub fn new(config: Mt5ClientConfig, account_id: Option<String>) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        Self {
            config,
            account_id_str: account_id,
            instruments: Arc::new(Mutex::new(HashMap::new())),
            bar_types: Arc::new(Mutex::new(HashMap::new())),
            is_running: Arc::new(AtomicBool::new(false)),
            thread_handle: Arc::new(Mutex::new(None)),
            request_response_lock: Arc::new(TokioMutex::new(())),
            command_tx,
            command_rx: Arc::new(Mutex::new(Some(command_rx))),
        }
    }

    /// Create client with default configuration
    pub fn with_defaults(account_id: Option<String>) -> Self {
        Self::new(Mt5ClientConfig::default(), account_id)
    }

    /// Get the account ID with venue prefix
    ///
    /// Constructs the full AccountId from the stored account number
    pub fn account_id(&self) -> Option<AccountId> {
        self.account_id_str
            .as_ref()
            .map(|id| AccountId::new(&format!("MT5-{}", id)))
    }

    /// Add instrument to cache
    pub fn add_instrument(&mut self, instrument: InstrumentAny) {
        let symbol = instrument.id().symbol.as_str().to_string();
        self.instruments.lock().unwrap().insert(symbol, instrument);
    }

    /// Check if client is connected and running
    pub fn is_active(&self) -> bool {
        self.is_running.load(Ordering::Acquire)
    }

    /// Check if instruments have been initialized/cached
    pub fn is_initialized(&self) -> bool {
        !self.instruments.lock().unwrap().is_empty()
    }

    /// Subscribe to quote tick data for the given instrument IDs
    ///
    /// Sends CONFIG messages to MT5 to start streaming tick data (bid/ask)
    pub fn subscribe_quotes(&self, instrument_ids: Vec<InstrumentId>) -> Mt5Result<()> {
        if !self.is_active() {
            return Err(Mt5Error::NotConnected);
        }

        let symbols: Vec<String> = instrument_ids
            .iter()
            .map(|id| id.symbol.as_str().to_string())
            .collect();

        self.send_config_requests(&symbols, crate::common::Mt5TimeFrame::Tick)
    }

    /// Subscribe to bar data for the given symbol and timeframe
    ///
    /// Sends CONFIG messages to MT5 to start streaming bar data.
    ///
    /// # Arguments
    /// * `symbol` - The symbol to subscribe (e.g., "BTCUSD")
    /// * `timeframe` - MT5 timeframe string (e.g., "M1", "H1", "D1")
    /// * `bar_type_str` - Full BarType string for reconstruction (e.g., "BTCUSD.MT5-1-MINUTE-LAST-EXTERNAL")
    pub fn subscribe_bars(
        &self,
        symbol: &str,
        timeframe: &str,
        bar_type_str: &str,
    ) -> Mt5Result<()> {
        if !self.is_active() {
            return Err(Mt5Error::NotConnected);
        }

        // Parse timeframe string to Mt5TimeFrame enum
        let mt5_timeframe = match timeframe {
            "M1" => crate::common::Mt5TimeFrame::M1,
            "M5" => crate::common::Mt5TimeFrame::M5,
            "M15" => crate::common::Mt5TimeFrame::M15,
            "M30" => crate::common::Mt5TimeFrame::M30,
            "H1" => crate::common::Mt5TimeFrame::H1,
            "H4" => crate::common::Mt5TimeFrame::H4,
            "D1" => crate::common::Mt5TimeFrame::D1,
            "W1" => crate::common::Mt5TimeFrame::W1,
            "MN1" => crate::common::Mt5TimeFrame::MN1,
            _ => {
                return Err(Mt5Error::Parse(format!(
                    "Invalid MT5 timeframe: {}. Supported: M1, M5, M15, M30, H1, H4, D1, W1, MN1",
                    timeframe
                )))
            }
        };

        // Store bar type mapping for later reconstruction
        self.bar_types.lock().unwrap().insert(
            (symbol.to_string(), timeframe.to_string()),
            bar_type_str.to_string(),
        );

        tracing::info!(
            "Subscribing to {} bars with MT5 timeframe {:?}",
            symbol,
            mt5_timeframe
        );
        self.send_config_requests(&[symbol.to_string()], mt5_timeframe)
    }

    /// Internal method to send CONFIG requests to MT5
    fn send_config_requests(
        &self,
        symbols: &[String],
        timeframe: crate::common::Mt5TimeFrame,
    ) -> Mt5Result<()> {
        // Create a temporary socket for subscription
        let context = zmq::Context::new();
        let socket = context.socket(zmq::REQ)?;
        socket.connect(&format!(
            "tcp://{}:{}",
            self.config.host, self.config.sys_port
        ))?;

        for symbol in symbols {
            let config = Mt5ConfigRequest::new(symbol, timeframe);
            let json = serde_json::to_string(&config)?;

            socket.send(&json, 0)?;
            let response = socket
                .recv_string(0)?
                .map_err(|e| Mt5Error::Connection(format!("Invalid UTF-8 in response: {:?}", e)))?;
            tracing::debug!(
                "MT5 CONFIG response for {} ({:?}): {}",
                symbol,
                timeframe,
                response
            );
        }

        Ok(())
    }

    /// Ensure ZMQ thread is running (starts it if not already started)
    ///
    /// Returns a receiver for messages if this is the first call
    fn ensure_thread_running(&self) -> Option<UnboundedReceiverStream<NautilusMessage>> {
        // Check if already running
        if self.is_running.load(Ordering::Acquire) {
            return None;
        }

        let (tx, rx) = mpsc::unbounded_channel();

        // Mark as running (use Release ordering for synchronization)
        self.is_running.store(true, Ordering::Release);

        // Clone everything we need for the thread
        let config = self.config.clone();
        let instruments = Arc::clone(&self.instruments);
        let bar_types = Arc::clone(&self.bar_types);
        let is_running = Arc::clone(&self.is_running);
        let account_id = self.account_id();

        // Take command receiver (can only be done once)
        let command_rx = self
            .command_rx
            .lock()
            .unwrap()
            .take()
            .expect("ZMQ thread can only be started once");

        // Spawn dedicated thread for ZMQ operations
        let handle = std::thread::spawn(move || {
            if let Err(e) = Self::zmq_thread_loop(
                config,
                instruments,
                bar_types,
                is_running,
                tx,
                account_id,
                command_rx,
            ) {
                tracing::error!("ZMQ thread error: {}", e);
            }
        });

        *self.thread_handle.lock().unwrap() = Some(handle);

        Some(UnboundedReceiverStream::new(rx))
    }

    /// Start streaming messages
    ///
    /// Spawns a dedicated thread for ZMQ operations and returns an async stream
    pub fn stream(&self) -> UnboundedReceiverStream<NautilusMessage> {
        self.ensure_thread_running()
            .expect("stream() can only be called once")
    }

    /// Close the connection and stop streaming
    ///
    /// This must be called explicitly to clean up resources.
    /// Note: This affects all clones since they share the same is_running flag.
    pub fn close(&self) {
        self.is_running.store(false, Ordering::Release);

        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.lock().unwrap().take() {
            let _ = handle.join();
        }

        tracing::info!("Closed MT5-ZeroMQ connection");
    }

    /// Handle ACCOUNT request-response in ZMQ thread
    ///
    /// Sends ACCOUNT request, waits for ACK, then receives response from dataSocket
    fn handle_account_request(
        sys_socket: &zmq::Socket,
        data_socket: &zmq::Socket,
    ) -> Mt5Result<String> {
        let request = serde_json::json!({"action": "ACCOUNT"});
        let request_str = serde_json::to_string(&request)?;

        // Send request via REQ socket
        sys_socket.send(&request_str, 0)?;

        // Wait for ACK (blocking, should be fast)
        let ack = sys_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("ACK recv error: {:?}", e)))?;
        tracing::debug!("ACCOUNT ACK: {}", ack);

        // Receive actual response from PULL socket (blocking with timeout)
        data_socket.set_rcvtimeo(10000)?; // 10 second timeout
        let response = data_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("Data recv error: {:?}", e)))?;

        tracing::debug!("ACCOUNT response received: {} bytes", response.len());
        Ok(response)
    }

    /// Handle SYMBOL_INFO request-response in ZMQ thread
    ///
    /// Sends SYMBOL_INFO request, waits for ACK, then receives response from dataSocket
    fn handle_instruments_request(
        sys_socket: &zmq::Socket,
        data_socket: &zmq::Socket,
    ) -> Mt5Result<String> {
        let request = serde_json::json!({"action": "SYMBOL_INFO"});
        let request_str = serde_json::to_string(&request)?;

        // Send request via REQ socket
        sys_socket.send(&request_str, 0)?;

        // Wait for ACK (blocking, should be fast)
        let ack = sys_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("ACK recv error: {:?}", e)))?;
        tracing::debug!("SYMBOL_INFO ACK: {}", ack);

        // Receive actual response from PULL socket (blocking with timeout)
        data_socket.set_rcvtimeo(10000)?; // 10 second timeout
        let response = data_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("Data recv error: {:?}", e)))?;

        tracing::debug!("SYMBOL_INFO response received: {} bytes", response.len());
        Ok(response)
    }

    /// Handle HISTORY request-response in ZMQ thread
    ///
    /// Sends HISTORY request, waits for ACK, then receives response from dataSocket
    fn handle_history_request(
        sys_socket: &zmq::Socket,
        data_socket: &zmq::Socket,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        count: Option<i32>,
    ) -> Mt5Result<String> {
        // Use current time if end is not specified (to get bars up to "now")
        let end_timestamp = end.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });

        let request = serde_json::json!({
            "action": "HISTORY",
            "actionType": "DATA",
            "symbol": symbol,
            "chartTF": timeframe,
            "fromDate": start.unwrap_or(0),
            "toDate": end_timestamp,
            "count": count.unwrap_or(1000)
        });
        let request_str = serde_json::to_string(&request)?;

        tracing::debug!("Sending HISTORY request: {}", request_str);

        // Send request via REQ socket
        sys_socket.send(&request_str, 0)?;

        // Wait for ACK (blocking, should be fast)
        let ack = sys_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("ACK recv error: {:?}", e)))?;
        tracing::debug!("HISTORY ACK: {}", ack);

        // Receive actual response from PULL socket (blocking with timeout)
        // Note: dataSocket may receive multiple messages - loop to find HISTORY response
        data_socket.set_rcvtimeo(10000)?; // 10 second timeout

        // Try up to 10 times to find a valid HISTORY response
        for attempt in 1..=10 {
            let response = data_socket.recv_string(0)?.map_err(|e| {
                Mt5Error::Connection(format!("Data recv error on attempt {}: {:?}", attempt, e))
            })?;

            tracing::debug!(
                "Received message attempt {}: {} bytes",
                attempt,
                response.len()
            );

            // Check if this is a HISTORY response (has "symbol" and "timeframe" fields)
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&response) {
                if json_val.get("symbol").is_some() && json_val.get("timeframe").is_some() {
                    tracing::debug!("Found valid HISTORY response on attempt {}", attempt);
                    return Ok(response);
                } else {
                    // Log and skip non-HISTORY messages
                    let preview = &response[..response.len().min(150)];
                    tracing::debug!("Skipping non-HISTORY message: {}", preview);
                }
            }
        }

        Err(Mt5Error::Parse(
            "No valid HISTORY response received after 10 attempts".to_string(),
        ))
    }

    /// Handle HISTORY/TRADES request-response in ZMQ thread
    ///
    /// Sends HISTORY request with actionType=TRADES, waits for ACK, then receives response
    fn handle_trade_history_request(
        sys_socket: &zmq::Socket,
        data_socket: &zmq::Socket,
    ) -> Mt5Result<String> {
        let request = serde_json::json!({
            "action": "HISTORY",
            "actionType": "TRADES"
        });
        let request_str = serde_json::to_string(&request)?;

        tracing::debug!("Sending HISTORY/TRADES request: {}", request_str);

        // Send request via REQ socket
        sys_socket.send(&request_str, 0)?;

        // Wait for ACK (blocking, should be fast)
        let ack = sys_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("ACK recv error: {:?}", e)))?;
        tracing::debug!("HISTORY/TRADES ACK: {}", ack);

        // Receive actual response from PULL socket (blocking with timeout)
        data_socket.set_rcvtimeo(10000)?; // 10 second timeout
        let response = data_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("Data recv error: {:?}", e)))?;

        tracing::debug!("HISTORY/TRADES response received: {} bytes", response.len());
        Ok(response)
    }

    /// Fallback method for requesting instruments when ZMQ thread is not running
    ///
    /// Creates temporary sockets, makes blocking request, and cleans up
    fn fallback_instruments_request(config: &Mt5ClientConfig) -> Mt5Result<String> {
        tracing::debug!("Creating temporary sockets for SYMBOL_INFO fallback request");

        let context = zmq::Context::new();

        // Create temporary REQ socket for sys_port
        let sys_socket = context.socket(zmq::REQ)?;
        sys_socket.connect(&format!("tcp://{}:{}", config.host, config.sys_port))?;

        // Create temporary PULL socket for data_port
        let data_socket = context.socket(zmq::PULL)?;
        data_socket.connect(&format!("tcp://{}:{}", config.host, config.data_port))?;

        // Use the same logic as handle_instruments_request
        let result = Self::handle_instruments_request(&sys_socket, &data_socket);

        // Sockets are automatically closed when dropped
        tracing::debug!("Temporary sockets closed");

        result
    }

    /// Fallback method for requesting account state when ZMQ thread is not running
    ///
    /// Creates temporary sockets, makes blocking request, and cleans up
    fn fallback_account_request(config: &Mt5ClientConfig) -> Mt5Result<String> {
        tracing::debug!("Creating temporary sockets for ACCOUNT fallback request");

        let context = zmq::Context::new();

        // Create temporary REQ socket for sys_port
        let sys_socket = context.socket(zmq::REQ)?;
        sys_socket.connect(&format!("tcp://{}:{}", config.host, config.sys_port))?;

        // Create temporary PULL socket for data_port
        let data_socket = context.socket(zmq::PULL)?;
        data_socket.connect(&format!("tcp://{}:{}", config.host, config.data_port))?;

        // Use the same logic as handle_account_request
        let result = Self::handle_account_request(&sys_socket, &data_socket);

        // Sockets are automatically closed when dropped
        tracing::debug!("Temporary sockets closed");

        result
    }

    /// Handle order submission in ZMQ thread
    ///
    /// Sends TRADE request, waits for ACK, then receives response from dataSocket
    fn handle_submit_order(
        sys_socket: &zmq::Socket,
        data_socket: &zmq::Socket,
        symbol: &str,
        order_side: OrderSide,
        order_type: OrderType,
        volume: f64,
        price: Option<f64>,
        sl: Option<f64>,
        tp: Option<f64>,
        comment: &str,
    ) -> Mt5Result<String> {
        // Map OrderSide to MT5 action type
        let action_type = match order_side {
            OrderSide::Buy => match order_type {
                OrderType::Market => "ORDER_TYPE_BUY",
                OrderType::Limit => "ORDER_TYPE_BUY_LIMIT",
                OrderType::StopMarket => "ORDER_TYPE_BUY_STOP",
                OrderType::StopLimit => "ORDER_TYPE_BUY_STOP_LIMIT",
                _ => {
                    return Err(Mt5Error::Parse(format!(
                        "Unsupported order type: {:?}",
                        order_type
                    )))
                }
            },
            OrderSide::Sell => match order_type {
                OrderType::Market => "ORDER_TYPE_SELL",
                OrderType::Limit => "ORDER_TYPE_SELL_LIMIT",
                OrderType::StopMarket => "ORDER_TYPE_SELL_STOP",
                OrderType::StopLimit => "ORDER_TYPE_SELL_STOP_LIMIT",
                _ => {
                    return Err(Mt5Error::Parse(format!(
                        "Unsupported order type: {:?}",
                        order_type
                    )))
                }
            },
            _ => {
                return Err(Mt5Error::Parse(format!(
                    "Invalid order side: {:?}",
                    order_side
                )))
            }
        };

        // For GTC orders, set expiration to far future (90 days from now)
        // MT5 rejects 0 as it represents January 1, 1970 (past date)
        let expiration_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + (90 * 24 * 60 * 60); // 90 days from now

        let request = serde_json::json!({
            "action": "TRADE",
            "actionType": action_type,
            "symbol": symbol,
            "volume": volume,
            "price": price.unwrap_or(0.0),
            "stoploss": sl.unwrap_or(0.0),
            "takeprofit": tp.unwrap_or(0.0),
            "expiration": expiration_timestamp,
            "comment": comment
        });

        let request_str = serde_json::to_string(&request)?;
        tracing::info!("=== MT5 ORDER SUBMISSION ===");
        tracing::info!("Request: {}", request_str);

        // Send request via REQ socket
        sys_socket.send(&request_str, 0)?;

        // Wait for ACK (blocking, should be fast)
        let ack = sys_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("ACK recv error: {:?}", e)))?;
        tracing::info!("TRADE ACK: {}", ack);

        // Receive actual response from PULL socket (blocking with timeout)
        data_socket.set_rcvtimeo(10000)?; // 10 second timeout
        let response = data_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("Data recv error: {:?}", e)))?;

        tracing::info!("TRADE response: {}", response);
        tracing::info!("=== END MT5 ORDER SUBMISSION ===");
        Ok(response)
    }

    /// Handle order cancellation in ZMQ thread
    ///
    /// Sends TRADE_CLOSE request to cancel an order by ticket
    fn handle_cancel_order(
        sys_socket: &zmq::Socket,
        data_socket: &zmq::Socket,
        ticket: u64,
    ) -> Mt5Result<String> {
        let request = serde_json::json!({
            "action": "TRADE_CLOSE",
            "ticket": ticket
        });

        let request_str = serde_json::to_string(&request)?;
        tracing::debug!("Canceling order: {}", request_str);

        sys_socket.send(&request_str, 0)?;
        let ack = sys_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("ACK recv error: {:?}", e)))?;
        tracing::debug!("TRADE_CLOSE ACK: {}", ack);

        data_socket.set_rcvtimeo(10000)?;
        let response = data_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("Data recv error: {:?}", e)))?;

        tracing::debug!("TRADE_CLOSE response received: {} bytes", response.len());
        Ok(response)
    }

    /// Handle order modification in ZMQ thread
    ///
    /// Sends TRADE_MODIFY request to modify order parameters
    fn handle_modify_order(
        sys_socket: &zmq::Socket,
        data_socket: &zmq::Socket,
        ticket: u64,
        price: Option<f64>,
        sl: Option<f64>,
        tp: Option<f64>,
    ) -> Mt5Result<String> {
        let request = serde_json::json!({
            "action": "TRADE_MODIFY",
            "ticket": ticket,
            "price": price.unwrap_or(0.0),
            "stoploss": sl.unwrap_or(0.0),
            "takeprofit": tp.unwrap_or(0.0)
        });

        let request_str = serde_json::to_string(&request)?;
        tracing::debug!("Modifying order: {}", request_str);

        sys_socket.send(&request_str, 0)?;
        let ack = sys_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("ACK recv error: {:?}", e)))?;
        tracing::debug!("TRADE_MODIFY ACK: {}", ack);

        data_socket.set_rcvtimeo(10000)?;
        let response = data_socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("Data recv error: {:?}", e)))?;

        tracing::debug!("TRADE_MODIFY response received: {} bytes", response.len());
        Ok(response)
    }

    /// ZMQ thread loop (runs in dedicated OS thread)
    ///
    /// This is where ZMQ sockets live - they never leave this thread!
    fn zmq_thread_loop(
        config: Mt5ClientConfig,
        instruments: Arc<Mutex<HashMap<String, InstrumentAny>>>,
        bar_types: Arc<Mutex<HashMap<(String, String), String>>>,
        is_running: Arc<AtomicBool>,
        tx: mpsc::UnboundedSender<NautilusMessage>,
        account_id: Option<AccountId>,
        mut command_rx: mpsc::UnboundedReceiver<ZmqCommand>,
    ) -> Mt5Result<()> {
        // Create ZMQ context and sockets (they live in this thread only!)
        let context = zmq::Context::new();

        // System socket (REQ) - for sending commands and receiving ACKs
        let sys_socket = context.socket(zmq::REQ)?;
        sys_socket.connect(&format!("tcp://{}:{}", config.host, config.sys_port))?;

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
            "ZMQ thread connected to MT5-ZeroMQ at {}:{},{},{},{}",
            config.host,
            config.sys_port,
            config.data_port,
            config.live_port,
            config.stream_port
        );

        // Check initial state (use Acquire ordering for synchronization)
        let initial_running = is_running.load(Ordering::Acquire);
        tracing::info!("ZMQ thread: is_running={}", initial_running);

        // Main polling loop (use Acquire ordering to see updates from other threads)
        let mut loop_count = 0;
        while is_running.load(Ordering::Acquire) {
            loop_count += 1;

            // PRIORITY 1: Handle commands first (request-response operations)
            while let Ok(command) = command_rx.try_recv() {
                match command {
                    ZmqCommand::RequestAccountState { response_tx } => {
                        let result = Self::handle_account_request(&sys_socket, &data_socket);
                        let _ = response_tx.send(result);
                    }
                    ZmqCommand::RequestInstruments { response_tx } => {
                        let result = Self::handle_instruments_request(&sys_socket, &data_socket);
                        let _ = response_tx.send(result);
                    }
                    ZmqCommand::SubmitOrder {
                        instrument_id,
                        client_order_id: _,
                        order_side,
                        order_type,
                        quantity,
                        price,
                        sl,
                        tp,
                        comment,
                        response_tx,
                    } => {
                        let symbol = instrument_id.symbol.as_str();
                        let volume = quantity.as_f64();
                        let price_val = price.map(|p| p.as_f64());
                        let sl_val = sl.map(|s| s.as_f64());
                        let tp_val = tp.map(|t| t.as_f64());

                        let result = Self::handle_submit_order(
                            &sys_socket,
                            &data_socket,
                            symbol,
                            order_side,
                            order_type,
                            volume,
                            price_val,
                            sl_val,
                            tp_val,
                            &comment,
                        );

                        let _ = response_tx.send(result);
                    }
                    ZmqCommand::CancelOrder {
                        venue_order_id,
                        response_tx,
                    } => {
                        // Parse venue_order_id (MT5 ticket) as u64
                        let ticket = venue_order_id.as_str().parse::<u64>().unwrap_or_else(|e| {
                            tracing::error!("Failed to parse venue_order_id as u64: {}", e);
                            0
                        });

                        let result = Self::handle_cancel_order(&sys_socket, &data_socket, ticket);
                        let _ = response_tx.send(result);
                    }
                    ZmqCommand::ModifyOrder {
                        venue_order_id,
                        price,
                        quantity: _, // MT5 doesn't support volume modification
                        sl,
                        tp,
                        response_tx,
                    } => {
                        let ticket = venue_order_id.as_str().parse::<u64>().unwrap_or_else(|e| {
                            tracing::error!("Failed to parse venue_order_id as u64: {}", e);
                            0
                        });

                        let price_val = price.map(|p| p.as_f64());
                        let sl_val = sl.map(|s| s.as_f64());
                        let tp_val = tp.map(|t| t.as_f64());

                        let result = Self::handle_modify_order(
                            &sys_socket,
                            &data_socket,
                            ticket,
                            price_val,
                            sl_val,
                            tp_val,
                        );
                        let _ = response_tx.send(result);
                    }
                    ZmqCommand::RequestHistory {
                        symbol,
                        timeframe,
                        start,
                        end,
                        count,
                        response_tx,
                    } => {
                        let result = Self::handle_history_request(
                            &sys_socket,
                            &data_socket,
                            &symbol,
                            &timeframe,
                            start,
                            end,
                            count,
                        );
                        let _ = response_tx.send(result);
                    }
                    ZmqCommand::RequestTradeHistory { response_tx } => {
                        let result = Self::handle_trade_history_request(&sys_socket, &data_socket);
                        let _ = response_tx.send(result);
                    }
                }
            }

            // Poll data socket (non-blocking)
            // This receives responses to commands sent via sys_port
            if let Ok(msg_bytes) = data_socket.recv_bytes(zmq::DONTWAIT) {
                tracing::debug!("Received data message: {} bytes", msg_bytes.len());
                match Self::handle_data_message(&msg_bytes, &instruments, account_id) {
                    Ok(Some(nautilus_msg)) => {
                        if tx.send(nautilus_msg).is_err() {
                            tracing::warn!(
                                "Data channel closed after {} loops, stopping ZMQ thread",
                                loop_count
                            );
                            break;
                        }
                    }
                    Err(e) => tracing::error!("Error handling data message: {}", e),
                    _ => {}
                }
            }

            // Poll live socket (non-blocking)
            if let Ok(msg_bytes) = live_socket.recv_bytes(zmq::DONTWAIT) {
                tracing::debug!("Received live message: {} bytes", msg_bytes.len());
                match Self::handle_live_message(&msg_bytes, &instruments, &bar_types, account_id) {
                    Ok(Some(nautilus_msg)) => {
                        if tx.send(nautilus_msg).is_err() {
                            tracing::warn!(
                                "Live channel closed after {} loops, stopping ZMQ thread",
                                loop_count
                            );
                            break;
                        }
                    }
                    Err(e) => tracing::error!("Error handling live message: {}", e),
                    _ => {}
                }
            }

            // Poll stream socket (non-blocking)
            if let Ok(msg_bytes) = stream_socket.recv_bytes(zmq::DONTWAIT) {
                tracing::debug!("Received stream message: {} bytes", msg_bytes.len());
                match Self::handle_stream_message(&msg_bytes, &instruments, account_id) {
                    Ok(Some(nautilus_msg)) => {
                        if tx.send(nautilus_msg).is_err() {
                            tracing::warn!(
                                "Stream channel closed after {} loops, stopping ZMQ thread",
                                loop_count
                            );
                            break;
                        }
                    }
                    Err(e) => tracing::error!("Error handling stream message: {}", e),
                    _ => {}
                }
            }

            // Small sleep to prevent busy-waiting (1ms)
            std::thread::sleep(std::time::Duration::from_millis(1));

            // Debug: Log periodically
            if loop_count % 1000 == 0 {
                tracing::debug!("ZMQ thread alive: {} loops", loop_count);
            }
        }

        tracing::info!(
            "ZMQ thread exited after {} loops, is_running={}",
            loop_count,
            is_running.load(Ordering::Acquire)
        );

        tracing::info!("ZMQ thread stopped");
        Ok(())
    }

    /// Handle message from liveSocket (tick or bar data)
    fn handle_live_message(
        msg_bytes: &[u8],
        instruments: &Arc<Mutex<HashMap<String, InstrumentAny>>>,
        bar_types: &Arc<Mutex<HashMap<(String, String), String>>>,
        _account_id: Option<AccountId>,
    ) -> Mt5Result<Option<NautilusMessage>> {
        let json_str = String::from_utf8_lossy(msg_bytes);

        // Peek at the JSON to determine if it's tick or bar data
        let peek: serde_json::Value = serde_json::from_str(&json_str)?;
        let timeframe = peek
            .get("timeframe")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Mt5Error::Parse("Missing 'timeframe' field in live message".to_string())
            })?;

        let ts_init = get_atomic_clock_realtime().get_time_ns();

        // Route to appropriate parser based on timeframe
        if timeframe == "TICK" {
            // Parse as tick message
            let msg: Mt5LiveTickMsg = serde_json::from_str(&json_str)?;

            // Validate data array format [timestamp_ms, bid, ask]
            if msg.data.len() < 3 {
                return Err(Mt5Error::Parse(format!(
                    "Invalid tick data array length: expected 3, got {}",
                    msg.data.len()
                )));
            }

            // Get instrument from cache
            let instruments = instruments.lock().unwrap();
            let instrument = instruments.get(msg.symbol.as_str()).ok_or_else(|| {
                Mt5Error::InstrumentNotFound(parse_instrument_id(msg.symbol.as_str()).unwrap())
            })?;

            // Parse to QuoteTick
            let quote = parse_mt5_live_tick_to_quote(&msg, instrument, ts_init)
                .map_err(|e| Mt5Error::Parse(e.to_string()))?;

            Ok(Some(NautilusMessage::Data(Data::Quote(quote))))
        } else {
            // Parse as bar message
            let msg: Mt5LiveBarMsg = serde_json::from_str(&json_str)?;

            // Get instrument from cache
            let instruments_guard = instruments.lock().unwrap();
            let instrument = instruments_guard.get(msg.symbol.as_str()).ok_or_else(|| {
                Mt5Error::InstrumentNotFound(parse_instrument_id(msg.symbol.as_str()).unwrap())
            })?;

            // Look up bar_type from mapping
            let bar_types_guard = bar_types.lock().unwrap();
            let bar_type_str = bar_types_guard
                .get(&(msg.symbol.to_string(), msg.timeframe.to_string()))
                .ok_or_else(|| {
                    Mt5Error::Parse(format!(
                        "No bar_type mapping found for symbol={}, timeframe={}. Did you call subscribe_bars()?",
                        msg.symbol, msg.timeframe
                    ))
                })?;

            // Parse bar_type_str to BarType
            let bar_type = BarType::from_str(bar_type_str).map_err(|e| {
                Mt5Error::Parse(format!(
                    "Failed to parse BarType from '{}': {}",
                    bar_type_str, e
                ))
            })?;

            // Parse to Bar
            let bar = parse_mt5_live_bar(&msg, &bar_type, instrument, ts_init)
                .map_err(|e| Mt5Error::Parse(e.to_string()))?;

            Ok(Some(NautilusMessage::Data(Data::Bar(bar))))
        }
    }

    /// Handle message from dataSocket (command responses)
    ///
    /// Note: Most command responses are handled synchronously in handle_*_request functions.
    /// This handler catches any unsolicited/unexpected messages on the data socket.
    /// Following Bybit's pattern, we return Raw for unrecognized messages rather than panicking.
    fn handle_data_message(
        msg_bytes: &[u8],
        _instruments: &Arc<Mutex<HashMap<String, InstrumentAny>>>,
        _account_id: Option<AccountId>,
    ) -> Mt5Result<Option<NautilusMessage>> {
        let json_str = String::from_utf8_lossy(msg_bytes);

        // Parse JSON to inspect the message type
        let value: serde_json::Value = serde_json::from_str(&json_str)?;

        // Log unexpected data messages for debugging
        tracing::debug!("Received unsolicited data message: {}", json_str);

        // Return as Raw message - command responses are already handled synchronously
        // in handle_*_request functions (ACCOUNT, SYMBOL_INFO, HISTORY, TRADE, etc.)
        Ok(Some(NautilusMessage::Raw(value.to_string())))
    }

    /// Handle message from streamSocket (orders/positions)
    ///
    /// This handles streaming updates for orders and positions from MT5.
    /// Following Bybit's pattern, we return typed variants for known message types
    /// and Raw for unrecognized messages.
    fn handle_stream_message(
        msg_bytes: &[u8],
        _instruments: &Arc<Mutex<HashMap<String, InstrumentAny>>>,
        _account_id: Option<AccountId>,
    ) -> Mt5Result<Option<NautilusMessage>> {
        let json_str = String::from_utf8_lossy(msg_bytes);

        // Parse JSON to inspect the message type
        let value: serde_json::Value = serde_json::from_str(&json_str)?;

        // Log stream messages for debugging
        tracing::debug!("Received stream message: {}", json_str);

        // Attempt to classify the message based on its structure
        // MT5 order/position updates can be identified by specific fields

        // Check for trade response (has retcode field)
        if value.get("retcode").is_some() {
            if let Ok(trade_resp) = serde_json::from_value::<Mt5TradeResponseMsg>(value.clone()) {
                tracing::info!(
                    "Trade response: retcode={}, order={}, price={}",
                    trade_resp.retcode,
                    trade_resp.order,
                    trade_resp.price
                );
                return Ok(Some(NautilusMessage::TradeResponse(trade_resp)));
            }
        }

        // Check for order update (has ticket and volume_initial fields)
        if value.get("ticket").is_some() && value.get("volume_initial").is_some() {
            if let Ok(order_msg) = serde_json::from_value::<Mt5OrderMsg>(value.clone()) {
                tracing::info!(
                    "Order update: ticket={}, symbol={}, type={}",
                    order_msg.ticket,
                    order_msg.symbol,
                    order_msg.type_
                );
                return Ok(Some(NautilusMessage::OrderUpdate(order_msg)));
            }
        }

        // Check for position update (has id and open fields)
        if value.get("id").is_some() && value.get("open").is_some() {
            if let Ok(pos_msg) = serde_json::from_value::<Mt5PositionMsg>(value.clone()) {
                tracing::info!(
                    "Position update: id={}, symbol={}, type={}",
                    pos_msg.id,
                    pos_msg.symbol,
                    pos_msg.type_
                );
                return Ok(Some(NautilusMessage::PositionUpdate(pos_msg)));
            }
        }

        // Return as Raw message for unrecognized stream messages
        Ok(Some(NautilusMessage::Raw(value.to_string())))
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
        socket.connect(&format!(
            "tcp://{}:{}",
            self.config.host, self.config.sys_port
        ))?;

        // Set timeout to prevent hanging
        socket.set_rcvtimeo(5000)?; // 5 seconds
        socket.set_sndtimeo(5000)?;

        socket.send(request, 0)?;
        let response = socket
            .recv_string(0)?
            .map_err(|e| Mt5Error::Connection(format!("Invalid UTF-8 in response: {:?}", e)))?;

        // Check for ERROR response from MT5
        if response == "ERROR" {
            return Err(Mt5Error::Parse(
                "MT5 returned ERROR - deserialization failed on MT5 side".to_string(),
            ));
        }

        Ok(response)
    }

    /// Request available instruments from MT5
    ///
    /// Sends SYMBOL_INFO action to query symbol specifications
    pub async fn request_instruments(&self) -> Mt5Result<Vec<InstrumentAny>> {
        // Acquire lock to serialize this request-response cycle
        // This prevents race conditions when multiple clients share the same Mt5Client
        let _lock = self.request_response_lock.lock().await;

        // Get response either via command channel (if thread running) or direct call (fallback)
        let response = if self.is_running.load(Ordering::Acquire) {
            // ZMQ thread is running - use command channel
            tracing::debug!("Using command channel for SYMBOL_INFO request");

            // Create oneshot channel for response
            let (response_tx, response_rx) = oneshot::channel();

            // Send command to ZMQ thread
            self.command_tx
                .send(ZmqCommand::RequestInstruments { response_tx })
                .map_err(|_| Mt5Error::Connection("Command channel closed".to_string()))?;

            // Wait for response from ZMQ thread
            response_rx
                .await
                .map_err(|_| Mt5Error::Connection("Response channel closed".to_string()))??
        } else {
            // ZMQ thread not running yet - use fallback with temporary sockets
            tracing::debug!("Using fallback blocking call for SYMBOL_INFO request");

            let config = self.config.clone();
            tokio::task::spawn_blocking(move || Self::fallback_instruments_request(&config))
                .await
                .map_err(|e| Mt5Error::Connection(format!("Task join error: {}", e)))??
        };

        tracing::debug!("SYMBOL_INFO response received: {} bytes", response.len());

        // Parse response as Mt5SymbolInfoResponse
        let symbol_info_response: Mt5SymbolInfoResponse = serde_json::from_str(&response)
            .map_err(|e| Mt5Error::Parse(format!("Failed to parse SYMBOL_INFO response: {}", e)))?;

        if symbol_info_response.error {
            return Err(Mt5Error::Parse(
                "SYMBOL_INFO returned error=true".to_string(),
            ));
        }

        // Convert each Mt5SymbolInfo to InstrumentAny
        use nautilus_core::UnixNanos;
        use nautilus_model::identifiers::Venue;

        let ts_init = UnixNanos::default();
        let ts_event = UnixNanos::default();
        let venue = Venue::new("MT5");

        let mut instruments = Vec::new();
        for symbol_info in symbol_info_response.symbols {
            match super::parse::parse_mt5_symbol_info_to_instrument(
                &symbol_info,
                &venue,
                ts_event,
                ts_init,
            ) {
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

        tracing::info!(
            "Successfully loaded {} instruments from MT5",
            instruments.len()
        );

        Ok(instruments)
    }

    /// Request account state from MT5
    ///
    /// Sends ACCOUNT action to query balance, margin, etc.
    pub async fn request_account_state(&self) -> Mt5Result<Mt5AccountMsg> {
        // Acquire lock to serialize this request-response cycle
        // This prevents race conditions when multiple clients share the same Mt5Client
        let _lock = self.request_response_lock.lock().await;

        // Get response either via command channel (if thread running) or direct call (fallback)
        let response = if self.is_running.load(Ordering::Acquire) {
            // ZMQ thread is running - use command channel
            tracing::debug!("Using command channel for ACCOUNT request");

            // Create oneshot channel for response
            let (response_tx, response_rx) = oneshot::channel();

            // Send command to ZMQ thread
            self.command_tx
                .send(ZmqCommand::RequestAccountState { response_tx })
                .map_err(|_| Mt5Error::Connection("Command channel closed".to_string()))?;

            // Wait for response from ZMQ thread
            response_rx
                .await
                .map_err(|_| Mt5Error::Connection("Response channel closed".to_string()))??
        } else {
            // ZMQ thread not running yet - use fallback with temporary sockets
            tracing::debug!("Using fallback blocking call for ACCOUNT request");

            let config = self.config.clone();
            tokio::task::spawn_blocking(move || Self::fallback_account_request(&config))
                .await
                .map_err(|e| Mt5Error::Connection(format!("Task join error: {}", e)))??
        };

        tracing::debug!("ACCOUNT response received: {} bytes", response.len());

        // Parse response as Mt5AccountMsg
        let account_msg: Mt5AccountMsg = serde_json::from_str(&response)
            .map_err(|e| Mt5Error::Parse(format!("Failed to parse ACCOUNT response: {}", e)))?;

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

    /// Request historical bar data from MT5
    ///
    /// Sends HISTORY action to retrieve historical OHLCV bars
    ///
    /// # Parameters
    /// - `symbol`: The symbol to request (e.g., "BTCUSD")
    /// - `timeframe`: The MT5 timeframe (e.g., "M1", "H1", "D1")
    /// - `start`: Optional start timestamp in seconds (Unix epoch)
    /// - `end`: Optional end timestamp in seconds (Unix epoch)
    /// - `count`: Optional number of bars to retrieve (default 1000)
    pub async fn request_bars(
        &self,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        count: Option<i32>,
    ) -> Mt5Result<Mt5HistoryMsg> {
        // Acquire lock to serialize this request-response cycle
        let _lock = self.request_response_lock.lock().await;

        // Send command to ZMQ thread
        tracing::debug!(
            "Sending HISTORY request via command channel: symbol={}, timeframe={}",
            symbol,
            timeframe
        );

        // Create oneshot channel for response
        let (response_tx, response_rx) = oneshot::channel();

        // Send command to ZMQ thread
        self.command_tx
            .send(ZmqCommand::RequestHistory {
                symbol: symbol.to_string(),
                timeframe: timeframe.to_string(),
                start,
                end,
                count,
                response_tx,
            })
            .map_err(|_| Mt5Error::Connection("Command channel closed".to_string()))?;

        // Wait for response from ZMQ thread
        let response = response_rx
            .await
            .map_err(|_| Mt5Error::Connection("Response channel closed".to_string()))??;

        tracing::debug!("HISTORY response received: {} bytes", response.len());

        // Parse response as Mt5HistoryMsg
        // Response format: {"symbol": str, "timeframe": str, "data": [[...], [...]]}
        let history_msg: Mt5HistoryMsg = serde_json::from_str(&response).map_err(|e| {
            tracing::error!(
                "Failed to parse HISTORY response. Response was: {}",
                &response[..response.len().min(500)]
            );
            Mt5Error::Parse(format!("Failed to parse HISTORY response: {}", e))
        })?;

        tracing::info!(
            "Successfully parsed {} historical data points for {}",
            history_msg.data.len(),
            history_msg.symbol
        );

        Ok(history_msg)
    }

    /// Request trade/deal history from MT5
    ///
    /// Sends HISTORY action with actionType=TRADES to retrieve all historical deals
    pub async fn request_trade_history(&self) -> Mt5Result<Vec<Mt5DealMsg>> {
        // Acquire lock to serialize this request-response cycle
        let _lock = self.request_response_lock.lock().await;

        tracing::debug!("Sending HISTORY/TRADES request via command channel");

        // Create oneshot channel for response
        let (response_tx, response_rx) = oneshot::channel();

        // Send command to ZMQ thread
        self.command_tx
            .send(ZmqCommand::RequestTradeHistory { response_tx })
            .map_err(|_| Mt5Error::Connection("Command channel closed".to_string()))?;

        // Wait for response from ZMQ thread
        let response = response_rx
            .await
            .map_err(|_| Mt5Error::Connection("Response channel closed".to_string()))??;

        tracing::debug!("HISTORY/TRADES response received: {} bytes", response.len());

        // Parse response as Mt5TradesResponse
        let trades_response: Mt5TradesResponse = serde_json::from_str(&response)
            .map_err(|e| Mt5Error::Parse(format!("Failed to parse TRADES response: {}", e)))?;

        tracing::info!(
            "Successfully parsed {} historical trades from MT5",
            trades_response.trades.len()
        );

        Ok(trades_response.trades)
    }

    /// Submit a trade order to MT5
    ///
    /// Sends TRADE action with order parameters via command channel
    pub async fn submit_order(
        &self,
        instrument_id: InstrumentId,
        client_order_id: ClientOrderId,
        order_side: OrderSide,
        order_type: OrderType,
        quantity: Quantity,
        price: Option<Price>,
        sl: Option<Price>,
        tp: Option<Price>,
        comment: String,
    ) -> Mt5Result<Mt5TradeResponse> {
        // Acquire lock to serialize this request-response cycle
        let _lock = self.request_response_lock.lock().await;

        // Create oneshot channel for response
        let (response_tx, response_rx) = oneshot::channel();

        // Send command to ZMQ thread
        self.command_tx
            .send(ZmqCommand::SubmitOrder {
                instrument_id,
                client_order_id,
                order_side,
                order_type,
                quantity,
                price,
                sl,
                tp,
                comment,
                response_tx,
            })
            .map_err(|_| Mt5Error::Connection("Command channel closed".to_string()))?;

        // Wait for response from ZMQ thread
        let response = response_rx
            .await
            .map_err(|_| Mt5Error::Connection("Response channel closed".to_string()))??;

        // Parse response as Mt5TradeResponse
        let trade_response: Mt5TradeResponse = serde_json::from_str(&response)
            .map_err(|e| Mt5Error::Parse(format!("Failed to parse TRADE response: {}", e)))?;

        Ok(trade_response)
    }

    /// Cancel an order in MT5
    ///
    /// Sends TRADE_CLOSE action with order ticket via command channel
    pub async fn cancel_order(&self, venue_order_id: VenueOrderId) -> Mt5Result<Mt5TradeResponse> {
        // Acquire lock to serialize this request-response cycle
        let _lock = self.request_response_lock.lock().await;

        // Create oneshot channel for response
        let (response_tx, response_rx) = oneshot::channel();

        // Send command to ZMQ thread
        self.command_tx
            .send(ZmqCommand::CancelOrder {
                venue_order_id,
                response_tx,
            })
            .map_err(|_| Mt5Error::Connection("Command channel closed".to_string()))?;

        // Wait for response from ZMQ thread
        let response = response_rx
            .await
            .map_err(|_| Mt5Error::Connection("Response channel closed".to_string()))??;

        tracing::debug!("TRADE_CLOSE response received: {} bytes", response.len());

        // Parse response as Mt5TradeResponse
        let trade_response: Mt5TradeResponse = serde_json::from_str(&response)
            .map_err(|e| Mt5Error::Parse(format!("Failed to parse TRADE_CLOSE response: {}", e)))?;

        Ok(trade_response)
    }

    /// Modify an order in MT5
    ///
    /// Sends TRADE_MODIFY action to update order parameters via command channel
    pub async fn modify_order(
        &self,
        venue_order_id: VenueOrderId,
        price: Option<Price>,
        quantity: Option<Quantity>,
        sl: Option<Price>,
        tp: Option<Price>,
    ) -> Mt5Result<Mt5TradeResponse> {
        // Acquire lock to serialize this request-response cycle
        let _lock = self.request_response_lock.lock().await;

        // Create oneshot channel for response
        let (response_tx, response_rx) = oneshot::channel();

        // Send command to ZMQ thread
        self.command_tx
            .send(ZmqCommand::ModifyOrder {
                venue_order_id,
                price,
                quantity,
                sl,
                tp,
                response_tx,
            })
            .map_err(|_| Mt5Error::Connection("Command channel closed".to_string()))?;

        // Wait for response from ZMQ thread
        let response = response_rx
            .await
            .map_err(|_| Mt5Error::Connection("Response channel closed".to_string()))??;

        tracing::debug!("TRADE_MODIFY response received: {} bytes", response.len());

        // Parse response as Mt5TradeResponse
        let trade_response: Mt5TradeResponse = serde_json::from_str(&response).map_err(|e| {
            Mt5Error::Parse(format!("Failed to parse TRADE_MODIFY response: {}", e))
        })?;

        Ok(trade_response)
    }
}

// Drop implementation removed - use explicit close() instead
// This prevents premature shutdown when temporary clones are dropped
// (matches pattern used by CoinbaseIntx and Bybit adapters)

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
