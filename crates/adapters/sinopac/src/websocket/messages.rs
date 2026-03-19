// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------
//! WebSocket message types for the Sinopac gateway.

use serde::{Deserialize, Serialize};

/// WebSocket subscribe/unsubscribe command message.
#[derive(Debug, Serialize)]
pub struct WsSubscribeMsg {
    pub action: String,
    pub contract_code: String,
    pub quote_type: String,
}

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

/// WebSocket tick message.
#[derive(Debug, Clone, Deserialize)]
pub struct WsTickMsg {
    pub code: String,
    pub data: WsTickData,
}

/// WebSocket tick data payload.
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

/// WebSocket bid/ask message.
#[derive(Debug, Clone, Deserialize)]
pub struct WsBidAskMsg {
    pub code: String,
    pub data: WsBidAskData,
}

/// WebSocket bid/ask data payload.
#[derive(Debug, Clone, Deserialize)]
pub struct WsBidAskData {
    pub bid_price: Vec<f64>,
    pub bid_volume: Vec<i64>,
    pub ask_price: Vec<f64>,
    pub ask_volume: Vec<i64>,
    pub timestamp: String,
}

/// Raw order update envelope. The `event` field discriminates the data type.
#[derive(Debug, Clone, Deserialize)]
pub struct WsOrderUpdateMsg {
    pub event: String,
    pub data: serde_json::Value,
}

impl WsOrderUpdateMsg {
    /// Parses the data field into a typed event based on the event string.
    pub fn parse_event(&self) -> anyhow::Result<OrderEvent> {
        match self.event.as_str() {
            "OrderState.StockOrder" => {
                let data: StockOrderEventData = serde_json::from_value(self.data.clone())?;
                Ok(OrderEvent::StockOrder(data))
            }
            "OrderState.StockDeal" => {
                let data: StockDealEventData = serde_json::from_value(self.data.clone())?;
                Ok(OrderEvent::StockDeal(data))
            }
            "OrderState.FuturesOrder" => {
                let data: FuturesOrderEventData = serde_json::from_value(self.data.clone())?;
                Ok(OrderEvent::FuturesOrder(data))
            }
            "OrderState.FuturesDeal" => {
                let data: FuturesDealEventData = serde_json::from_value(self.data.clone())?;
                Ok(OrderEvent::FuturesDeal(data))
            }
            other => anyhow::bail!("Unknown order event type: {other}"),
        }
    }
}

/// Typed order event after parsing the `data` field.
#[derive(Debug, Clone)]
pub enum OrderEvent {
    StockOrder(StockOrderEventData),
    StockDeal(StockDealEventData),
    FuturesOrder(FuturesOrderEventData),
    FuturesDeal(FuturesDealEventData),
}

/// Operation information for order events.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OperationInfo {
    pub op_type: String,
    pub op_code: String,
    pub op_msg: String,
}

/// Order status information from the gateway.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrderStatusInfo {
    pub id: String,
    pub exchange_ts: f64,
    pub modified_price: f64,
    pub cancel_quantity: i64,
    pub order_quantity: i64,
}

/// Stock contract information from order events.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StockContractInfo {
    pub security_type: String,
    pub exchange: String,
    pub code: String,
    pub symbol: String,
    pub name: String,
}

/// Futures contract information from order events.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FuturesContractInfo {
    pub security_type: String,
    pub code: String,
    pub exchange: String,
    #[serde(default)]
    pub delivery_month: Option<String>,
    #[serde(default)]
    pub strike_price: Option<f64>,
    #[serde(default)]
    pub option_right: Option<String>,
}

/// Stock order information from order events.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StockOrderInfo {
    pub id: String,
    pub seqno: String,
    pub ordno: String,
    pub action: String,
    pub price: f64,
    pub quantity: i64,
    pub order_type: String,
    pub price_type: String,
    #[serde(default)]
    pub order_cond: Option<String>,
    #[serde(default)]
    pub order_lot: Option<String>,
}

/// Futures order information from order events.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FuturesOrderInfo {
    pub id: String,
    pub seqno: String,
    pub ordno: String,
    pub action: String,
    pub price: f64,
    pub quantity: i64,
    pub order_type: String,
    pub price_type: String,
    #[serde(default)]
    pub market_type: Option<String>,
    #[serde(default)]
    pub oc_type: Option<String>,
    #[serde(default)]
    pub combo: Option<bool>,
}

/// Stock order event data from the gateway.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StockOrderEventData {
    pub operation: OperationInfo,
    pub order: StockOrderInfo,
    pub status: OrderStatusInfo,
    pub contract: StockContractInfo,
}

/// Stock deal event data from the gateway.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StockDealEventData {
    pub trade_id: String,
    pub seqno: String,
    pub ordno: String,
    #[serde(default)]
    pub exchange_seq: Option<String>,
    pub broker_id: String,
    pub account_id: String,
    pub action: String,
    pub code: String,
    pub price: f64,
    pub quantity: i64,
    pub ts: f64,
    #[serde(default)]
    pub order_cond: Option<String>,
    #[serde(default)]
    pub order_lot: Option<String>,
}

/// Futures order event data from the gateway.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FuturesOrderEventData {
    pub operation: OperationInfo,
    pub order: FuturesOrderInfo,
    pub status: OrderStatusInfo,
    pub contract: FuturesContractInfo,
}

/// Futures deal event data from the gateway.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FuturesDealEventData {
    pub trade_id: String,
    pub seqno: String,
    pub ordno: String,
    #[serde(default)]
    pub exchange_seq: Option<String>,
    pub broker_id: String,
    pub account_id: String,
    pub action: String,
    pub code: String,
    pub price: f64,
    pub quantity: i64,
    pub ts: f64,
    #[serde(default)]
    pub security_type: Option<String>,
    #[serde(default)]
    pub market_type: Option<String>,
    #[serde(default)]
    pub combo: Option<bool>,
}

/// WebSocket subscription confirmation message.
#[derive(Debug, Clone, Deserialize)]
pub struct WsConfirmMsg {
    pub code: String,
    pub quote_type: String,
}

/// WebSocket error message from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct WsErrorMsg {
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::common::testing::load_test_json_as;

    #[rstest]
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

    #[rstest]
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

    #[rstest]
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

    #[rstest]
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

    #[rstest]
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

    #[rstest]
    fn test_deserialize_ws_error() {
        let msg: WsIncomingMsg = load_test_json_as("ws_error.json");
        match msg {
            WsIncomingMsg::Error(err) => {
                assert!(err.detail.contains("Missing"));
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[rstest]
    fn test_parse_order_event_stock_order() {
        let msg: WsIncomingMsg = load_test_json_as("ws_order_stock.json");
        match msg {
            WsIncomingMsg::OrderUpdate(update) => {
                assert_eq!(update.event, "OrderState.StockOrder");
                let event = update.parse_event().expect("parse_event failed");
                match event {
                    OrderEvent::StockOrder(data) => {
                        assert_eq!(data.operation.op_type, "New");
                        assert_eq!(data.operation.op_code, "00");
                        assert_eq!(data.order.id, "abc123");
                        assert_eq!(data.order.action, "Buy");
                        assert_eq!(data.order.price, 580.0);
                        assert_eq!(data.order.quantity, 1);
                        assert_eq!(data.order.order_type, "ROD");
                        assert_eq!(data.order.price_type, "LMT");
                        assert_eq!(data.status.order_quantity, 1);
                        assert_eq!(data.contract.code, "2330");
                        assert_eq!(data.contract.security_type, "STK");
                    }
                    _ => panic!("Expected StockOrder event"),
                }
            }
            _ => panic!("Expected OrderUpdate message"),
        }
    }

    #[rstest]
    fn test_parse_order_event_stock_deal() {
        let msg: WsIncomingMsg = load_test_json_as("ws_deal_stock.json");
        match msg {
            WsIncomingMsg::OrderUpdate(update) => {
                assert_eq!(update.event, "OrderState.StockDeal");
                let event = update.parse_event().expect("parse_event failed");
                match event {
                    OrderEvent::StockDeal(data) => {
                        assert_eq!(data.trade_id, "abc123");
                        assert_eq!(data.ordno, "A1234");
                        assert_eq!(data.action, "Buy");
                        assert_eq!(data.code, "2330");
                        assert_eq!(data.price, 580.0);
                        assert_eq!(data.quantity, 1);
                        assert_eq!(data.ts, 1709352601.0);
                    }
                    _ => panic!("Expected StockDeal event"),
                }
            }
            _ => panic!("Expected OrderUpdate message"),
        }
    }

    #[rstest]
    fn test_parse_order_event_futures_order() {
        let msg: WsIncomingMsg = load_test_json_as("ws_order_futures.json");
        match msg {
            WsIncomingMsg::OrderUpdate(update) => {
                assert_eq!(update.event, "OrderState.FuturesOrder");
                let event = update.parse_event().expect("parse_event failed");
                match event {
                    OrderEvent::FuturesOrder(data) => {
                        assert_eq!(data.operation.op_type, "New");
                        assert_eq!(data.operation.op_code, "00");
                        assert_eq!(data.order.id, "fut001");
                        assert_eq!(data.order.action, "Buy");
                        assert_eq!(data.order.price, 18000.0);
                        assert_eq!(data.order.quantity, 2);
                        assert_eq!(data.order.market_type.as_deref(), Some("Day"));
                        assert_eq!(data.order.oc_type.as_deref(), Some("New"));
                        assert_eq!(data.contract.code, "TXFC6");
                        assert_eq!(data.contract.security_type, "FUT");
                    }
                    _ => panic!("Expected FuturesOrder event"),
                }
            }
            _ => panic!("Expected OrderUpdate message"),
        }
    }

    #[rstest]
    fn test_parse_order_event_futures_deal() {
        let msg: WsIncomingMsg = load_test_json_as("ws_deal_futures.json");
        match msg {
            WsIncomingMsg::OrderUpdate(update) => {
                assert_eq!(update.event, "OrderState.FuturesDeal");
                let event = update.parse_event().expect("parse_event failed");
                match event {
                    OrderEvent::FuturesDeal(data) => {
                        assert_eq!(data.trade_id, "fut001");
                        assert_eq!(data.ordno, "F5678");
                        assert_eq!(data.action, "Buy");
                        assert_eq!(data.code, "TXFC6");
                        assert_eq!(data.price, 18000.0);
                        assert_eq!(data.quantity, 2);
                        assert_eq!(data.ts, 1709352701.0);
                        assert_eq!(data.security_type.as_deref(), Some("FUT"));
                    }
                    _ => panic!("Expected FuturesDeal event"),
                }
            }
            _ => panic!("Expected OrderUpdate message"),
        }
    }

    #[rstest]
    fn test_parse_order_event_unknown_type() {
        let msg = WsOrderUpdateMsg {
            event: "OrderState.Unknown".to_string(),
            data: serde_json::json!({}),
        };
        assert!(msg.parse_event().is_err());
    }
}
