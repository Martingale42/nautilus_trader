use serde::{Deserialize, Serialize};

/// Trading action (buy/sell).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShioajiAction {
    Buy,
    Sell,
}

/// Price type for order submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShioajiPriceType {
    LMT,
    MKT,
    MKP,
}

/// Order duration (time in force).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShioajiOrderType {
    ROD,
    IOC,
    FOK,
}

/// Stock order credit condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShioajiOrderCond {
    Cash,
    MarginTrading,
    ShortSelling,
}

/// Stock lot size type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShioajiOrderLot {
    Common,
    Odd,
    IntradayOdd,
    Fixing,
}

/// Quote subscription type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShioajiQuoteType {
    Tick,
    BidAsk,
}

/// Market type for endpoint routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShioajiMarket {
    Stock,
    Futures,
    Options,
}

/// Exchange code for Taiwan markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShioajiExchange {
    TSE,
    OTC,
}

/// Order update event types from gateway WS.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShioajiOrderEvent {
    #[serde(rename = "OrderState.StockOrder")]
    StockOrder,
    #[serde(rename = "OrderState.StockDeal")]
    StockDeal,
    #[serde(rename = "OrderState.FuturesOrder")]
    FuturesOrder,
    #[serde(rename = "OrderState.FuturesDeal")]
    FuturesDeal,
}
