pub mod http;
pub mod websocket;

use pyo3::prelude::*;

use crate::common::enums::*;

/// Loaded as `nautilus_pyo3.shioaji`.
#[pymodule]
pub fn shioaji(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Constants
    m.add("SINOPAC", crate::common::consts::SINOPAC)?;

    // Enums
    m.add_class::<ShioajiAction>()?;
    m.add_class::<ShioajiPriceType>()?;
    m.add_class::<ShioajiOrderType>()?;
    m.add_class::<ShioajiOrderCond>()?;
    m.add_class::<ShioajiOrderLot>()?;
    m.add_class::<ShioajiQuoteType>()?;
    m.add_class::<ShioajiMarket>()?;
    m.add_class::<ShioajiExchange>()?;

    // Clients
    m.add_class::<crate::http::client::ShioajiHttpClient>()?;
    m.add_class::<crate::websocket::client::ShioajiWebSocketClient>()?;

    Ok(())
}
