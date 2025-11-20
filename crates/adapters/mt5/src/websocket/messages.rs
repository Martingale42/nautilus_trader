//! MT5 message structures.
//!
//! These structs represent the native JSON format sent by MT5-ZeroMQ (JsonAPI.mq5).

use crate::common::{Mt5OrderType, Mt5TimeFrame};
use serde::{Deserialize, Deserializer, Serialize};
use ustr::Ustr;

/// Deserialize integer (0/1) as boolean
///
/// MT5 sends boolean fields as integers (0 = false, 1 = true)
fn deserialize_int_as_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match u8::deserialize(deserializer)? {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(serde::de::Error::custom(format!(
            "Expected 0 or 1 for boolean, got {}",
            other
        ))),
    }
}

/// Top-level MT5 message enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Mt5Message {
    Tick(Mt5TickMsg),
    Trade(Mt5TradeResponseMsg),
    Order(Mt5OrderMsg),
    Position(Mt5PositionMsg),
    History(Mt5HistoryMsg),
    Account(Mt5AccountMsg),
    Error(Mt5ErrorMsg),
}

/// MT5 live tick message (from liveSocket)
///
/// Actual format sent by MT5-ZeroMQ for live tick data
/// Format: {"status": "CONNECTED", "symbol": "BTCUSD", "timeframe": "TICK", "data": [timestamp_ms, bid, ask]}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5LiveTickMsg {
    /// Connection status (e.g., "CONNECTED")
    pub status: Ustr,
    /// Symbol name (e.g., "BTCUSD")
    pub symbol: Ustr,
    /// Timeframe (always "TICK" for tick data)
    pub timeframe: Ustr,
    /// Tick data: [timestamp_ms, bid, ask]
    pub data: Vec<f64>,
}

/// MT5 live bar message (from liveSocket)
///
/// Actual format sent by MT5-ZeroMQ for live bar data
/// Format: {"status": "CONNECTED", "symbol": "BTCUSD", "timeframe": "M1", "data": [timestamp_sec, open, high, low, close, volume]}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5LiveBarMsg {
    /// Connection status (e.g., "CONNECTED")
    pub status: Ustr,
    /// Symbol name (e.g., "BTCUSD")
    pub symbol: Ustr,
    /// Timeframe (e.g., "M1", "H1", "D1")
    pub timeframe: Ustr,
    /// Bar data: [timestamp_sec, open, high, low, close, volume]
    pub data: Vec<f64>,
}

/// Legacy MT5 tick message format (not actually used by MT5-ZeroMQ)
///
/// Kept for compatibility with old code/tests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Mt5TickMsg {
    /// Symbol name (e.g., "EURUSD")
    pub symbol: Ustr,
    /// Bid price
    pub bid: f64,
    /// Ask price
    pub ask: f64,
    /// Last price
    pub last: f64,
    /// Volume
    pub volume: f64,
    /// Timestamp in milliseconds
    pub time: i64,
    /// Tick flags (optional)
    #[serde(default)]
    pub flags: u32,
}

/// MT5 trade response message (from streamSocket after order submission)
///
/// Response to TRADE action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5TradeResponseMsg {
    /// Whether the trade resulted in an error
    pub error: bool,
    /// MT5 return code (10009 = success, etc.)
    pub retcode: i32,
    /// Human-readable description
    #[serde(rename = "desription")] // Note: typo in JsonAPI.mq5
    pub description: Ustr,
    /// Order ticket number
    pub order: i64,
    /// Executed volume
    pub volume: f64,
    /// Executed price
    pub price: f64,
    /// Current bid
    pub bid: f64,
    /// Current ask
    pub ask: f64,
    /// Function that processed the request
    pub function: Ustr,
}

/// MT5 order message
///
/// Represents a pending or historical order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5OrderMsg {
    /// Order ticket
    pub ticket: i64,
    /// Symbol
    pub symbol: Ustr,
    /// Order type (0=BUY, 1=SELL, etc.)
    pub type_order: i32,
    /// Order state
    pub state: i32,
    /// Volume
    pub volume: f64,
    /// Opening price
    pub price_open: f64,
    /// Stop loss
    #[serde(default)]
    pub sl: f64,
    /// Take profit
    #[serde(default)]
    pub tp: f64,
    /// Time when order was set up
    pub time_setup: i64,
    /// Order comment (used for client order ID)
    #[serde(default)]
    pub comment: Ustr,
}

/// MT5 position message
///
/// Represents an open position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5PositionMsg {
    /// Position ticket
    pub ticket: i64,
    /// Symbol
    pub symbol: Ustr,
    /// Position type (0=BUY, 1=SELL)
    pub type_position: i32,
    /// Volume
    pub volume: f64,
    /// Opening price
    pub price_open: f64,
    /// Stop loss
    #[serde(default)]
    pub sl: f64,
    /// Take profit
    #[serde(default)]
    pub tp: f64,
    /// Current profit
    pub profit: f64,
    /// Position comment
    #[serde(default)]
    pub comment: Ustr,
}

/// MT5 historical data message
///
/// Response to HISTORY action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5HistoryMsg {
    /// Symbol
    pub symbol: Ustr,
    /// Timeframe
    #[serde(rename = "chartTF")]
    pub chart_tf: Mt5TimeFrame,
    /// Historical data points
    pub data: Vec<Mt5BarData>,
}

/// MT5 bar data (OHLCV)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5BarData {
    /// Bar timestamp (seconds)
    pub time: i64,
    /// Open price
    pub open: f64,
    /// High price
    pub high: f64,
    /// Low price
    pub low: f64,
    /// Close price
    pub close: f64,
    /// Tick volume
    pub tick_volume: i64,
    /// Real volume
    #[serde(default)]
    pub real_volume: i64,
    /// Spread
    #[serde(default)]
    pub spread: i32,
}

/// MT5 account information message
///
/// Response to ACCOUNT or BALANCE action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5AccountMsg {
    /// Account login (optional, may not be in all responses)
    #[serde(default)]
    pub login: i64,
    /// Account name (optional, may not be in all responses)
    #[serde(default)]
    pub name: Ustr,
    /// Broker name
    pub broker: Ustr,
    /// Account currency
    pub currency: Ustr,
    /// Server name
    pub server: Ustr,
    /// Whether trading is allowed (MT5 sends as integer 0/1)
    #[serde(deserialize_with = "deserialize_int_as_bool")]
    pub trading_allowed: bool,
    /// Whether bot trading is enabled (MT5 sends as integer 0/1)
    #[serde(deserialize_with = "deserialize_int_as_bool")]
    pub bot_trading: bool,
    /// Account balance
    pub balance: f64,
    /// Account equity
    pub equity: f64,
    /// Margin used
    pub margin: f64,
    /// Free margin
    pub margin_free: f64,
    /// Margin level (percentage)
    pub margin_level: f64,
    /// Profit (optional, may not be in all responses)
    #[serde(default)]
    pub profit: f64,
}

/// MT5 error message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5ErrorMsg {
    /// Error code
    pub error_code: i32,
    /// Error description
    pub error_description: Ustr,
    /// Function where error occurred
    #[serde(default)]
    pub function: Ustr,
}

/// MT5 symbol information response
///
/// Response to SYMBOL_INFO action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5SymbolInfoResponse {
    /// Whether the response contains an error
    pub error: bool,
    /// List of symbol specifications
    pub symbols: Vec<Mt5SymbolInfo>,
}

/// MT5 symbol information
///
/// Complete symbol specifications from MT5's SymbolInfo* functions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5SymbolInfo {
    /// Symbol name (e.g., "EURAUD")
    pub symbol: Ustr,
    /// Symbol description
    pub description: Ustr,
    /// Base currency
    pub base_currency: Ustr,
    /// Quote/profit currency
    pub quote_currency: Ustr,
    /// Profit currency (same as quote)
    pub profit_currency: Ustr,
    /// Margin currency
    pub margin_currency: Ustr,

    // Precision
    /// Price decimal digits
    pub digits: String,
    /// Minimum price change
    pub point: String,
    /// Current spread in points
    pub spread: String,
    /// Minimum distance of SL/TP from price
    pub stops_level: String,

    // Contract specifications
    /// Standard lot size
    pub contract_size: String,
    /// Tick value in account currency
    pub tick_value: String,
    /// Minimum price change
    pub tick_size: String,

    // Volume limits
    /// Minimum lot size
    pub volume_min: String,
    /// Maximum lot size
    pub volume_max: String,
    /// Lot size increment
    pub volume_step: String,
    /// Maximum aggregate volume
    pub volume_limit: String,

    // Swap
    /// Long position swap
    pub swap_long: String,
    /// Short position swap
    pub swap_short: String,
    /// Swap calculation mode
    pub swap_mode: String,

    // Trade modes
    /// Trade allowed mode
    pub trade_mode: String,
    /// Order execution mode
    pub trade_execution: String,
    /// Margin calculation mode
    pub trade_calc_mode: String,

    // Order/execution modes
    /// Order expiration modes
    pub expiration_mode: String,
    /// Order filling modes
    pub filling_mode: String,
    /// Allowed order types
    pub order_mode: String,

    // Margin
    /// Initial margin
    pub margin_initial: String,
    /// Maintenance margin
    pub margin_maintenance: String,
    /// Hedged margin
    pub margin_hedged: String,

    // Status
    /// Whether symbol is selected in MarketWatch
    pub select: String,
    /// Whether symbol is visible
    pub visible: String,

    // Session statistics
    /// Number of deals in current session
    pub session_deals: String,
    /// Number of buy orders
    pub session_buy_orders: String,
    /// Number of sell orders
    pub session_sell_orders: String,

    // Current prices
    /// Last quote time
    pub time: String,
    /// Current bid price
    pub bid: String,
    /// Session low bid
    pub bidlow: String,
    /// Session high bid
    pub bidhigh: String,
    /// Current ask price
    pub ask: String,
    /// Session low ask
    pub asklow: String,
    /// Session high ask
    pub askhigh: String,
    /// Last deal price
    pub last: String,

    // Trade-specific values
    /// Tick value for profit calculation
    pub trade_tick_value_profit: String,
    /// Tick value for loss calculation
    pub trade_tick_value_loss: String,

    // Freeze/stops levels
    /// Freeze level for orders
    pub trade_freeze_level: String,
}

/// MT5 action request (for sending to MT5)
///
/// This is what we send to MT5-ZeroMQ via sysSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5ActionRequest {
    /// Action type (e.g., "CONFIG", "TRADE", "POSITIONS")
    pub action: Ustr,
    /// Action-specific fields (optional)
    #[serde(flatten)]
    pub fields: serde_json::Value,
}

/// MT5 CONFIG action request
///
/// Subscribe to symbol for live data streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5ConfigRequest {
    pub action: Ustr, // "CONFIG"
    pub symbol: Ustr,
    #[serde(rename = "chartTF")]
    pub chart_tf: Mt5TimeFrame,
}

impl Mt5ConfigRequest {
    pub fn new(symbol: impl Into<Ustr>, timeframe: Mt5TimeFrame) -> Self {
        Self {
            action: Ustr::from("CONFIG"),
            symbol: symbol.into(),
            chart_tf: timeframe,
        }
    }
}

/// MT5 TRADE action request
///
/// Submit a trade order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5TradeRequest {
    pub action: Ustr, // "TRADE"
    #[serde(rename = "actionType")]
    pub action_type: Mt5OrderType,
    pub symbol: Ustr,
    pub volume: f64,
    pub price: f64,
    pub stoploss: f64,
    pub takeprofit: f64,
    #[serde(default)]
    pub deviation: f64,
    #[serde(default)]
    pub comment: Ustr,
    #[serde(default)]
    pub expiration: i64, // Unix timestamp for pending orders
}

impl Mt5TradeRequest {
    pub fn new_market_order(
        symbol: impl Into<Ustr>,
        order_type: Mt5OrderType,
        volume: f64,
        comment: impl Into<Ustr>,
    ) -> Self {
        Self {
            action: Ustr::from("TRADE"),
            action_type: order_type,
            symbol: symbol.into(),
            volume,
            price: 0.0,
            stoploss: 0.0,
            takeprofit: 0.0,
            deviation: 0.0,
            comment: comment.into(),
            expiration: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_tick_msg() {
        let json = r#"{
            "symbol": "EURUSD",
            "bid": 1.08500,
            "ask": 1.08510,
            "last": 1.08505,
            "volume": 100.0,
            "time": 1709891679000,
            "flags": 32
        }"#;

        let msg: Mt5TickMsg = serde_json::from_str(json).unwrap();
        assert_eq!(msg.symbol.as_str(), "EURUSD");
        assert_eq!(msg.bid, 1.08500);
        assert_eq!(msg.ask, 1.08510);
    }

    #[test]
    fn test_deserialize_trade_response() {
        let json = r#"{
            "error": false,
            "retcode": 10009,
            "desription": "TRADE_RETCODE_DONE",
            "order": 123456789,
            "volume": 0.01,
            "price": 1.08510,
            "bid": 1.08500,
            "ask": 1.08510,
            "function": "TradingModule"
        }"#;

        let msg: Mt5TradeResponseMsg = serde_json::from_str(json).unwrap();
        assert!(!msg.error);
        assert_eq!(msg.retcode, 10009);
        assert_eq!(msg.order, 123456789);
    }

    #[test]
    fn test_serialize_config_request() {
        let req = Mt5ConfigRequest::new("EURUSD", Mt5TimeFrame::M5);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("CONFIG"));
        assert!(json.contains("EURUSD"));
        assert!(json.contains("M5"));
    }

    #[test]
    fn test_serialize_trade_request() {
        let req = Mt5TradeRequest::new_market_order(
            "EURUSD",
            Mt5OrderType::OrderTypeBuy,
            0.01,
            "test-order",
        );
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("TRADE"));
        assert!(json.contains("ORDER_TYPE_BUY"));
        assert!(json.contains("EURUSD"));
    }
}
