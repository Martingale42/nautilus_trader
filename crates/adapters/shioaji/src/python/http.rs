use nautilus_core::python::to_pyruntime_err;
use pyo3::prelude::*;

use crate::http::{client::ShioajiHttpClient, models::LoginRequest};

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
}
