pub mod http;
pub mod websocket;

use pyo3::prelude::*;

use crate::common::enums::*;

/// Loaded as `nautilus_pyo3.sinopac`.
#[pymodule]
pub fn sinopac(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Constants
    m.add("SINOPAC", crate::common::consts::SINOPAC)?;

    // Enums
    m.add_class::<SinopacAction>()?;
    m.add_class::<SinopacPriceType>()?;
    m.add_class::<SinopacOrderType>()?;
    m.add_class::<SinopacOrderCond>()?;
    m.add_class::<SinopacOrderLot>()?;
    m.add_class::<SinopacQuoteType>()?;
    m.add_class::<SinopacMarket>()?;
    m.add_class::<SinopacExchange>()?;

    // Clients
    m.add_class::<crate::http::client::SinopacHttpClient>()?;
    m.add_class::<crate::websocket::client::SinopacWebSocketClient>()?;

    Ok(())
}
