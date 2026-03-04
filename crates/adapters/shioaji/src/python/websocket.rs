use nautilus_core::python::to_pyruntime_err;
use pyo3::prelude::*;

use crate::websocket::client::ShioajiWebSocketClient;

#[pymethods]
impl ShioajiWebSocketClient {
    #[new]
    #[pyo3(signature = (url=None))]
    fn py_new(url: Option<String>) -> Self {
        Self::new(url)
    }

    #[pyo3(name = "is_connected")]
    fn py_is_connected(&self) -> bool {
        self.is_connected()
    }

    #[pyo3(name = "subscribe")]
    fn py_subscribe(&self, code: String, quote_type: String) -> PyResult<()> {
        self.subscribe(&code, &quote_type).map_err(to_pyruntime_err)
    }

    #[pyo3(name = "unsubscribe")]
    fn py_unsubscribe(&self, code: String, quote_type: String) -> PyResult<()> {
        self.unsubscribe(&code, &quote_type).map_err(to_pyruntime_err)
    }
}
