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
//! REST API request and response models for the Sinopac gateway.

use serde::{Deserialize, Serialize};

/// Login request payload.
#[derive(Debug, Serialize)]
pub struct LoginRequest {
    pub api_key: String,
    pub secret_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_passwd: Option<String>,
    #[serde(default)]
    pub simulation: bool,
}

/// Login response containing account information.
#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub accounts: Vec<AccountInfo>,
}

/// Account information from the gateway.
#[derive(Debug, Deserialize)]
pub struct AccountInfo {
    pub account_type: String,
    pub account_id: String,
}

/// Gateway status response.
#[derive(Debug, Deserialize)]
pub struct StatusResponse {
    pub connected: bool,
    pub simulation: bool,
}

/// Generic message response from the gateway.
#[derive(Debug, Deserialize)]
pub struct MessageResponse {
    pub status: String,
}

/// Response containing a trade ID.
#[derive(Debug, Deserialize)]
pub struct TradeIdResponse {
    pub status: String,
    pub trade_id: String,
}

/// Stock contract from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct StockContract {
    pub code: String,
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub category: String,
    pub limit_up: f64,
    pub limit_down: f64,
    pub reference: f64,
    pub update_date: String,
    pub day_trade: String,
}

/// Futures contract from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct FuturesContract {
    pub code: String,
    pub symbol: String,
    pub name: String,
    pub category: String,
    pub delivery_month: String,
    pub delivery_date: String,
    pub underlying_kind: String,
    pub limit_up: f64,
    pub limit_down: f64,
    pub reference: f64,
    pub update_date: String,
}

/// Options contract from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct OptionsContract {
    pub code: String,
    pub symbol: String,
    pub name: String,
    pub category: String,
    pub delivery_month: String,
    pub delivery_date: String,
    pub strike_price: f64,
    pub option_right: String,
    pub underlying_kind: String,
    pub limit_up: f64,
    pub limit_down: f64,
    pub reference: f64,
    pub update_date: String,
}

/// Market snapshot data from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotData {
    pub code: String,
    pub exchange: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: i64,
    pub total_volume: i64,
    pub buy_price: f64,
    pub buy_volume: f64,
    pub sell_price: f64,
    pub sell_volume: f64,
    pub change_price: f64,
    pub change_rate: f64,
    pub ts: u64,
}

/// Historical tick data response.
#[derive(Debug, Clone, Deserialize)]
pub struct TicksResponse {
    pub code: String,
    pub ts: Vec<u64>,
    pub close: Vec<f64>,
    pub volume: Vec<i64>,
    pub bid_price: Vec<f64>,
    pub ask_price: Vec<f64>,
    pub tick_type: Vec<i32>,
}

/// Historical OHLCV bar data response.
#[derive(Debug, Clone, Deserialize)]
pub struct KBarsResponse {
    pub code: String,
    pub ts: Vec<u64>,
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub volume: Vec<i64>,
}

/// Place order request payload.
#[derive(Debug, Serialize)]
pub struct PlaceOrderRequest {
    pub code: String,
    pub action: String,
    pub price: f64,
    pub quantity: i64,
    pub price_type: String,
    pub order_type: String,
    pub order_cond: String,
    pub order_lot: String,
    pub market: String,
}

/// Update order request payload.
#[derive(Debug, Serialize)]
pub struct UpdateOrderRequest {
    pub trade_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<i64>,
}

/// Cancel order request payload.
#[derive(Debug, Serialize)]
pub struct CancelOrderRequest {
    pub trade_id: String,
}

/// Place order response from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct PlaceOrderResponse {
    pub trade_id: String,
    pub code: String,
    pub action: String,
    pub status: String,
}

/// Active trade information from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct TradeInfo {
    pub trade_id: String,
    pub code: String,
    pub action: String,
    pub price: f64,
    pub quantity: i64,
    pub status: String,
    pub order_type: String,
    pub price_type: String,
}

/// Account position from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct Position {
    pub code: String,
    pub direction: String,
    pub quantity: i64,
    pub price: f64,
    pub last_price: f64,
    pub pnl: f64,
    pub yd_quantity: i64,
}

/// Account balance from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountBalance {
    pub date: String,
    pub balance: f64,
}

/// Margin information from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct MarginInfo {
    pub yesterday_balance: f64,
    pub today_balance: f64,
    pub available_margin: f64,
    pub risk_indicator: f64,
}

/// Profit and loss record from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct ProfitLoss {
    pub code: String,
    pub quantity: i64,
    pub buy_price: f64,
    pub sell_price: f64,
    pub pnl: f64,
    pub pr_ratio: f64,
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::common::testing::load_test_json_as;

    #[rstest]
    fn test_deserialize_stock_contracts() {
        let contracts: Vec<StockContract> = load_test_json_as("contracts_stocks.json");
        assert_eq!(contracts.len(), 2);
        assert_eq!(contracts[0].code, "2330");
        assert_eq!(contracts[0].name, "台積電");
        assert_eq!(contracts[0].exchange, "TSE");
        assert_eq!(contracts[0].limit_up, 638.0);
        assert_eq!(contracts[1].code, "2317");
    }

    #[rstest]
    fn test_deserialize_futures_contracts() {
        let contracts: Vec<FuturesContract> = load_test_json_as("contracts_futures.json");
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].code, "TXFC6");
        assert_eq!(contracts[0].delivery_month, "2026/06");
        assert_eq!(contracts[0].underlying_kind, "I");
    }

    #[rstest]
    fn test_deserialize_options_contracts() {
        let contracts: Vec<OptionsContract> = load_test_json_as("contracts_options.json");
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].code, "TXO20000C6");
        assert_eq!(contracts[0].strike_price, 20000.0);
        assert_eq!(contracts[0].option_right, "Call");
    }

    #[rstest]
    fn test_deserialize_snapshots() {
        let snapshots: Vec<SnapshotData> = load_test_json_as("market_snapshots.json");
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].code, "2330");
        assert_eq!(snapshots[0].close, 580.0);
        assert_eq!(snapshots[0].buy_price, 580.0);
        assert_eq!(snapshots[0].sell_price, 581.0);
        assert_eq!(snapshots[0].buy_volume, 120.0);
    }

    #[rstest]
    fn test_deserialize_ticks() {
        let ticks: TicksResponse = load_test_json_as("market_ticks.json");
        assert_eq!(ticks.code, "2330");
        assert_eq!(ticks.ts.len(), 2);
        assert_eq!(ticks.close, vec![580.0, 581.0]);
        assert_eq!(ticks.volume, vec![100, 200]);
        assert_eq!(ticks.tick_type, vec![1, 2]);
    }

    #[rstest]
    fn test_deserialize_kbars() {
        let kbars: KBarsResponse = load_test_json_as("market_kbars.json");
        assert_eq!(kbars.code, "2330");
        assert_eq!(kbars.ts.len(), 2);
        assert_eq!(kbars.open, vec![578.0, 580.0]);
        assert_eq!(kbars.volume, vec![5000, 3000]);
    }

    #[rstest]
    fn test_deserialize_account_balance() {
        let balance: AccountBalance = load_test_json_as("account_balance.json");
        assert_eq!(balance.date, "2026-03-02");
        assert_eq!(balance.balance, 1_500_000.0);
    }

    #[rstest]
    fn test_deserialize_positions() {
        let positions: Vec<Position> = load_test_json_as("account_positions.json");
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].code, "2330");
        assert_eq!(positions[0].direction, "Buy");
        assert_eq!(positions[0].quantity, 1000);
        assert_eq!(positions[0].pnl, 5000.0);
    }

    #[rstest]
    fn test_deserialize_margin() {
        let margin: MarginInfo = load_test_json_as("account_margin.json");
        assert_eq!(margin.yesterday_balance, 2_000_000.0);
        assert_eq!(margin.available_margin, 1_800_000.0);
    }

    #[rstest]
    fn test_deserialize_profit_loss() {
        let pnl: Vec<ProfitLoss> = load_test_json_as("account_pnl.json");
        assert_eq!(pnl.len(), 1);
        assert_eq!(pnl[0].code, "2330");
        assert_eq!(pnl[0].pnl, 5000.0);
        assert_eq!(pnl[0].pr_ratio, 0.87);
    }

    #[rstest]
    fn test_deserialize_trades() {
        let trades: Vec<TradeInfo> = load_test_json_as("orders_trades.json");
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade_id, "trade-001");
        assert_eq!(trades[0].code, "2330");
        assert_eq!(trades[0].action, "Buy");
        assert_eq!(trades[0].status, "Filled");
    }
}
