use nautilus_core::python::to_pyruntime_err;
use nautilus_model::python::instruments::instrument_any_to_pyobject;
use pyo3::{prelude::*, types::PyList};

use crate::http::{
    client::ShioajiHttpClient,
    models::LoginRequest,
    parse::{parse_futures_to_contract, parse_options_to_contract, parse_stock_to_equity},
};

#[pymethods]
impl ShioajiHttpClient {
    #[new]
    #[pyo3(signature = (base_url=None))]
    fn py_new(base_url: Option<String>) -> PyResult<Self> {
        Self::new(base_url).map_err(to_pyruntime_err)
    }

    #[getter]
    #[pyo3(name = "base_url")]
    fn py_base_url(&self) -> &str {
        self.base_url()
    }

    #[pyo3(name = "status")]
    fn py_status<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let status = client.status().await.map_err(to_pyruntime_err)?;
            Ok((status.connected, status.simulation))
        })
    }

    #[pyo3(name = "login")]
    #[pyo3(signature = (api_key, secret_key, ca_path=None, ca_passwd=None, simulation=false))]
    fn py_login<'py>(
        &self,
        py: Python<'py>,
        api_key: String,
        secret_key: String,
        ca_path: Option<String>,
        ca_passwd: Option<String>,
        simulation: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let request = LoginRequest {
                api_key,
                secret_key,
                ca_path,
                ca_passwd,
                simulation,
            };
            let response = client.login(&request).await.map_err(to_pyruntime_err)?;
            let accounts: Vec<(String, String)> = response
                .accounts
                .into_iter()
                .map(|a| (a.account_type, a.account_id))
                .collect();
            Ok(accounts)
        })
    }

    #[pyo3(name = "logout")]
    fn py_logout<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            client.logout().await.map_err(to_pyruntime_err)?;
            Ok(())
        })
    }

    /// Fetch all stock contracts and return as Nautilus Equity instruments.
    #[pyo3(name = "request_stock_instruments")]
    fn py_request_stock_instruments<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let contracts = client.list_stocks().await.map_err(to_pyruntime_err)?;
            let ts = nautilus_core::UnixNanos::default();
            let instruments: Vec<_> = contracts
                .iter()
                .filter_map(|c| parse_stock_to_equity(c, ts, ts).ok())
                .collect();
            Python::attach(|py| {
                let py_instruments: PyResult<Vec<_>> = instruments
                    .into_iter()
                    .map(|inst| instrument_any_to_pyobject(py, inst))
                    .collect();
                let pylist = PyList::new(py, py_instruments?)
                    .unwrap()
                    .into_any()
                    .unbind();
                Ok(pylist)
            })
        })
    }

    /// Fetch all futures contracts and return as Nautilus FuturesContract instruments.
    #[pyo3(name = "request_futures_instruments")]
    fn py_request_futures_instruments<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let contracts = client.list_futures().await.map_err(to_pyruntime_err)?;
            let ts = nautilus_core::UnixNanos::default();
            let instruments: Vec<_> = contracts
                .iter()
                .filter_map(|c| parse_futures_to_contract(c, ts, ts).ok())
                .collect();
            Python::attach(|py| {
                let py_instruments: PyResult<Vec<_>> = instruments
                    .into_iter()
                    .map(|inst| instrument_any_to_pyobject(py, inst))
                    .collect();
                let pylist = PyList::new(py, py_instruments?)
                    .unwrap()
                    .into_any()
                    .unbind();
                Ok(pylist)
            })
        })
    }

    /// Fetch all options contracts and return as Nautilus OptionContract instruments.
    #[pyo3(name = "request_options_instruments")]
    fn py_request_options_instruments<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let contracts = client.list_options().await.map_err(to_pyruntime_err)?;
            let ts = nautilus_core::UnixNanos::default();
            let instruments: Vec<_> = contracts
                .iter()
                .filter_map(|c| parse_options_to_contract(c, ts, ts).ok())
                .collect();
            Python::attach(|py| {
                let py_instruments: PyResult<Vec<_>> = instruments
                    .into_iter()
                    .map(|inst| instrument_any_to_pyobject(py, inst))
                    .collect();
                let pylist = PyList::new(py, py_instruments?)
                    .unwrap()
                    .into_any()
                    .unbind();
                Ok(pylist)
            })
        })
    }
}
