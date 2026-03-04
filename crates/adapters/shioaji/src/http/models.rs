use serde::{Deserialize, Serialize};

// ─── Auth ────────────────────────────────────────

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

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub accounts: Vec<AccountInfo>,
}

#[derive(Debug, Deserialize)]
pub struct AccountInfo {
    pub account_type: String,
    pub account_id: String,
}

#[derive(Debug, Deserialize)]
pub struct StatusResponse {
    pub connected: bool,
    pub simulation: bool,
}

#[derive(Debug, Deserialize)]
pub struct MessageResponse {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct TradeIdResponse {
    pub status: String,
    pub trade_id: String,
}

// ─── Contracts ───────────────────────────────────

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

// ─── Market Data ─────────────────────────────────

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

// ─── Orders ──────────────────────────────────────

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

#[derive(Debug, Serialize)]
pub struct UpdateOrderRequest {
    pub trade_id: String,
    pub price: f64,
    pub quantity: i64,
}

#[derive(Debug, Serialize)]
pub struct CancelOrderRequest {
    pub trade_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlaceOrderResponse {
    pub trade_id: String,
    pub code: String,
    pub action: String,
    pub status: String,
}

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

// ─── Account ─────────────────────────────────────

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

#[derive(Debug, Clone, Deserialize)]
pub struct AccountBalance {
    pub date: String,
    pub balance: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarginInfo {
    pub yesterday_balance: f64,
    pub today_balance: f64,
    pub available_margin: f64,
    pub risk_indicator: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfitLoss {
    pub code: String,
    pub quantity: i64,
    pub buy_price: f64,
    pub sell_price: f64,
    pub pnl: f64,
    pub pr_ratio: f64,
}
