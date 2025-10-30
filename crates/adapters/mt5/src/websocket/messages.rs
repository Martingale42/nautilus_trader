//! MT5 message structures.
//!
//! These structs represent the native JSON format sent by MT5-ZeroMQ (JsonAPI.mq5).

use crate::common::{Mt5OrderType, Mt5TimeFrame};
use serde::{Deserialize, Serialize};
use ustr::Ustr;

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

/// MT5 tick message (from liveSocket)
///
/// Sent when market data updates occur
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Account login
    pub login: i64,
    /// Account name
    pub name: Ustr,
    /// Broker name
    pub broker: Ustr,
    /// Account currency
    pub currency: Ustr,
    /// Server name
    pub server: Ustr,
    /// Whether trading is allowed
    pub trading_allowed: bool,
    /// Whether bot trading is enabled
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
    /// Profit
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
