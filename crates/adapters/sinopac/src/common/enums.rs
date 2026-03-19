use serde::{Deserialize, Serialize};

/// Trading action (buy/sell).
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        eq,
        eq_int,
        module = "nautilus_trader.core.nautilus_pyo3.sinopac",
        from_py_object
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SinopacAction {
    Buy,
    Sell,
}

/// Price type for order submission.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        eq,
        eq_int,
        module = "nautilus_trader.core.nautilus_pyo3.sinopac",
        from_py_object
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SinopacPriceType {
    LMT,
    MKT,
    MKP,
}

/// Order duration (time in force).
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        eq,
        eq_int,
        module = "nautilus_trader.core.nautilus_pyo3.sinopac",
        from_py_object
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SinopacOrderType {
    ROD,
    IOC,
    FOK,
}

/// Stock order credit condition.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        eq,
        eq_int,
        module = "nautilus_trader.core.nautilus_pyo3.sinopac",
        from_py_object
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SinopacOrderCond {
    Cash,
    MarginTrading,
    ShortSelling,
}

/// Stock lot size type.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        eq,
        eq_int,
        module = "nautilus_trader.core.nautilus_pyo3.sinopac",
        from_py_object
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SinopacOrderLot {
    Common,
    Odd,
    IntradayOdd,
    Fixing,
}

/// Quote subscription type.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        eq,
        eq_int,
        module = "nautilus_trader.core.nautilus_pyo3.sinopac",
        from_py_object
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SinopacQuoteType {
    Tick,
    BidAsk,
}

/// Market type for endpoint routing.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        eq,
        eq_int,
        module = "nautilus_trader.core.nautilus_pyo3.sinopac",
        from_py_object
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SinopacMarket {
    Stock,
    Futures,
    Options,
}

/// Exchange code for Taiwan markets.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        eq,
        eq_int,
        module = "nautilus_trader.core.nautilus_pyo3.sinopac",
        from_py_object
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SinopacExchange {
    TSE,
    OTC,
}

/// Order update event types from gateway WS.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SinopacOrderEvent {
    #[serde(rename = "OrderState.StockOrder")]
    StockOrder,
    #[serde(rename = "OrderState.StockDeal")]
    StockDeal,
    #[serde(rename = "OrderState.FuturesOrder")]
    FuturesOrder,
    #[serde(rename = "OrderState.FuturesDeal")]
    FuturesDeal,
}
