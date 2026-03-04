use serde::{Deserialize, Serialize};

// ─── Client → Server ─────────────────────────────

#[derive(Debug, Serialize)]
pub struct WsSubscribeMsg {
    pub action: String,
    pub contract_code: String,
    pub quote_type: String,
}

// ─── Server → Client (envelope) ──────────────────

/// Raw WS message envelope. The `type` field determines the payload shape.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum WsIncomingMsg {
    #[serde(rename = "tick")]
    Tick(WsTickMsg),
    #[serde(rename = "bidask")]
    BidAsk(WsBidAskMsg),
    #[serde(rename = "order_update")]
    OrderUpdate(WsOrderUpdateMsg),
    #[serde(rename = "subscribed")]
    Subscribed(WsConfirmMsg),
    #[serde(rename = "unsubscribed")]
    Unsubscribed(WsConfirmMsg),
    #[serde(rename = "error")]
    Error(WsErrorMsg),
}

// ─── Market Data Payloads ────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct WsTickMsg {
    pub code: String,
    pub data: WsTickData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsTickData {
    pub close: f64,
    pub volume: i64,
    pub total_volume: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub bid_side_total_vol: i64,
    pub ask_side_total_vol: i64,
    pub timestamp: String,
    // Stock-only fields (None for futures/options)
    pub tick_type: Option<i32>,
    pub avg_price: Option<f64>,
    pub amount: Option<f64>,
    pub pct_chg: Option<f64>,
    // Futures/options-only field
    pub underlying_price: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsBidAskMsg {
    pub code: String,
    pub data: WsBidAskData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsBidAskData {
    pub bid_price: Vec<f64>,
    pub bid_volume: Vec<i64>,
    pub ask_price: Vec<f64>,
    pub ask_volume: Vec<i64>,
    pub timestamp: String,
}

// ─── Order Update Payloads ───────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct WsOrderUpdateMsg {
    pub event: String,
    pub data: serde_json::Value,
}

// ─── Confirmation & Error ────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct WsConfirmMsg {
    pub code: String,
    pub quote_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsErrorMsg {
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::testing::load_test_json_as;

    #[test]
    fn test_deserialize_ws_tick_stock() {
        let msg: WsIncomingMsg = load_test_json_as("ws_tick_stock.json");
        match msg {
            WsIncomingMsg::Tick(tick) => {
                assert_eq!(tick.code, "2330");
                assert_eq!(tick.data.close, 580.0);
                assert!(tick.data.tick_type.is_some());
                assert!(tick.data.underlying_price.is_none());
            }
            _ => panic!("Expected Tick message"),
        }
    }

    #[test]
    fn test_deserialize_ws_tick_futures() {
        let msg: WsIncomingMsg = load_test_json_as("ws_tick_futures.json");
        match msg {
            WsIncomingMsg::Tick(tick) => {
                assert_eq!(tick.code, "TXFC6");
                assert!(tick.data.tick_type.is_none());
                assert!(tick.data.underlying_price.is_some());
            }
            _ => panic!("Expected Tick message"),
        }
    }

    #[test]
    fn test_deserialize_ws_bidask() {
        let msg: WsIncomingMsg = load_test_json_as("ws_bidask.json");
        match msg {
            WsIncomingMsg::BidAsk(ba) => {
                assert_eq!(ba.code, "2330");
                assert_eq!(ba.data.bid_price.len(), 5);
                assert_eq!(ba.data.ask_price.len(), 5);
                assert_eq!(ba.data.bid_volume.len(), 5);
                assert_eq!(ba.data.ask_volume.len(), 5);
            }
            _ => panic!("Expected BidAsk message"),
        }
    }

    #[test]
    fn test_deserialize_ws_order_update() {
        let msg: WsIncomingMsg = load_test_json_as("ws_order_update.json");
        match msg {
            WsIncomingMsg::OrderUpdate(update) => {
                assert_eq!(update.event, "Submitted");
                assert!(update.data.is_object());
            }
            _ => panic!("Expected OrderUpdate message"),
        }
    }

    #[test]
    fn test_deserialize_ws_subscribed() {
        let msg: WsIncomingMsg = load_test_json_as("ws_subscribed.json");
        match msg {
            WsIncomingMsg::Subscribed(confirm) => {
                assert_eq!(confirm.code, "2330");
                assert_eq!(confirm.quote_type, "tick");
            }
            _ => panic!("Expected Subscribed message"),
        }
    }

    #[test]
    fn test_deserialize_ws_error() {
        let msg: WsIncomingMsg = load_test_json_as("ws_error.json");
        match msg {
            WsIncomingMsg::Error(err) => {
                assert!(err.detail.contains("Missing"));
            }
            _ => panic!("Expected Error message"),
        }
    }
}
