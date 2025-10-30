// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2025 Nautech Systems Pty Ltd. All rights reserved.
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

//! Python bindings for the MT5 ZeroMQ client.
//!
//! ## Design Pattern: Clone and Share State
//!
//! The client must be cloned for async operations because PyO3's `future_into_py`
//! requires `'static` futures. The client uses Arc internally to share state across clones.

use futures_util::StreamExt;
use nautilus_core::python::to_pyruntime_err;
use nautilus_model::{
    identifiers::AccountId,
    instruments::Instrument,
    python::{
        data::data_to_pycapsule,
        instruments::{instrument_any_to_pyobject, pyobject_to_instrument_any},
    },
};
use pyo3::{conversion::IntoPyObjectExt, prelude::*, types::{PyDict, PyList}};

use crate::websocket::client::{Mt5Client, Mt5ClientConfig, NautilusMessage};

#[pymethods]
impl Mt5Client {
    #[new]
    #[pyo3(signature = (host=None, live_port=None, stream_port=None, sys_port=None, account_id=None))]
    fn py_new(
        host: Option<String>,
        live_port: Option<u16>,
        stream_port: Option<u16>,
        sys_port: Option<u16>,
        account_id: Option<String>,
    ) -> PyResult<Self> {
        let config = if let Some(host) = host {
            Mt5ClientConfig {
                host,
                live_port: live_port.unwrap_or(2203),
                stream_port: stream_port.unwrap_or(2204),
                sys_port: sys_port.unwrap_or(2201),
            }
        } else {
            Mt5ClientConfig::default()
        };

        let account_id = account_id.map(|id| AccountId::new(&id));

        Ok(Self::new(config, account_id))
    }

    #[getter]
    #[pyo3(name = "host")]
    #[must_use]
    pub fn py_host(&self) -> String {
        self.config.host.clone()
    }

    #[getter]
    #[pyo3(name = "live_port")]
    #[must_use]
    pub const fn py_live_port(&self) -> u16 {
        self.config.live_port
    }

    #[getter]
    #[pyo3(name = "stream_port")]
    #[must_use]
    pub const fn py_stream_port(&self) -> u16 {
        self.config.stream_port
    }

    #[getter]
    #[pyo3(name = "sys_port")]
    #[must_use]
    pub const fn py_sys_port(&self) -> u16 {
        self.config.sys_port
    }

    #[pyo3(name = "is_active")]
    fn py_is_active(&self) -> bool {
        self.is_active()
    }

    #[pyo3(name = "add_instrument")]
    fn py_add_instrument(&self, py: Python, instrument: Py<PyAny>) -> PyResult<()> {
        let instrument_any = pyobject_to_instrument_any(py, instrument)?;
        let symbol = instrument_any.id().symbol.as_str().to_string();
        self.instruments
            .lock()
            .unwrap()
            .insert(symbol, instrument_any);
        Ok(())
    }

    #[pyo3(name = "subscribe")]
    fn py_subscribe(&self, symbols: Vec<String>) -> PyResult<()> {
        self.subscribe(symbols).map_err(to_pyruntime_err)
    }

    #[pyo3(name = "connect")]
    fn py_connect<'py>(
        &self,
        py: Python<'py>,
        instruments: Vec<Py<PyAny>>,
        callback: Py<PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Add instruments to cache
        for inst in instruments {
            let inst_any = pyobject_to_instrument_any(py, inst)?;
            let symbol = inst_any.id().symbol.as_str().to_string();
            self.instruments.lock().unwrap().insert(symbol, inst_any);
        }

        let client = self.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Start streaming
            let stream = client.stream();

            // Spawn background task to process messages
            tokio::spawn(async move {
                tokio::pin!(stream);

                while let Some(msg) = stream.next().await {
                    match msg {
                        NautilusMessage::Data(data) => Python::attach(|py| {
                            let py_obj = data_to_pycapsule(py, data);
                            call_python(py, &callback, py_obj);
                        }),
                        NautilusMessage::Raw(raw_json) => {
                            tracing::debug!("Raw MT5 message: {}", raw_json);
                            // TODO: Parse and handle other message types (orders, positions, etc.)
                        }
                    }
                }
            });

            Ok(())
        })
    }

    #[pyo3(name = "disconnect")]
    fn py_disconnect(&self) {
        self.disconnect();
    }

    #[pyo3(name = "wait_until_active")]
    fn py_wait_until_active<'py>(
        &self,
        py: Python<'py>,
        timeout_secs: f64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            client
                .wait_until_active(timeout_secs)
                .await
                .map_err(to_pyruntime_err)?;
            Ok(())
        })
    }

    #[pyo3(name = "request_instruments")]
    fn py_request_instruments<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let instruments = client
                .request_instruments()
                .await
                .map_err(to_pyruntime_err)?;

            Python::with_gil(|py| {
                // Convert instruments to PyObjects
                let py_instruments: PyResult<Vec<Py<PyAny>>> = instruments
                    .into_iter()
                    .map(|inst| instrument_any_to_pyobject(py, inst))
                    .collect();

                PyList::new(py, py_instruments?)
                    .unwrap()
                    .into_any()
                    .into_py_any(py)
            })
        })
    }

    #[pyo3(name = "request_account_state")]
    fn py_request_account_state<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let account = client
                .request_account_state()
                .await
                .map_err(to_pyruntime_err)?;

            Python::with_gil(|py| {
                // Convert to Python dict
                let dict = PyDict::new(py);
                dict.set_item("login", account.login)?;
                dict.set_item("name", account.name.as_str())?;
                dict.set_item("broker", account.broker.as_str())?;
                dict.set_item("currency", account.currency.as_str())?;
                dict.set_item("server", account.server.as_str())?;
                dict.set_item("trading_allowed", account.trading_allowed)?;
                dict.set_item("bot_trading", account.bot_trading)?;
                dict.set_item("balance", account.balance)?;
                dict.set_item("equity", account.equity)?;
                dict.set_item("margin", account.margin)?;
                dict.set_item("margin_free", account.margin_free)?;
                dict.set_item("margin_level", account.margin_level)?;
                dict.set_item("profit", account.profit)?;
                dict.into_py_any(py)
            })
        })
    }

    #[pyo3(name = "request_orders")]
    fn py_request_orders<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let orders = client.request_orders().await.map_err(to_pyruntime_err)?;

            Python::with_gil(|py| {
                let py_orders: PyResult<Vec<Py<PyAny>>> = orders
                    .into_iter()
                    .map(|order| {
                        let dict = PyDict::new(py);
                        dict.set_item("ticket", order.ticket)?;
                        dict.set_item("symbol", order.symbol.as_str())?;
                        dict.set_item("type_order", order.type_order)?;
                        dict.set_item("state", order.state)?;
                        dict.set_item("volume", order.volume)?;
                        dict.set_item("price_open", order.price_open)?;
                        dict.set_item("sl", order.sl)?;
                        dict.set_item("tp", order.tp)?;
                        dict.set_item("time_setup", order.time_setup)?;
                        dict.set_item("comment", order.comment.as_str())?;
                        dict.into_py_any(py)
                    })
                    .collect();

                PyList::new(py, py_orders?)
                    .unwrap()
                    .into_any()
                    .into_py_any(py)
            })
        })
    }

    #[pyo3(name = "request_positions")]
    fn py_request_positions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let positions = client
                .request_positions()
                .await
                .map_err(to_pyruntime_err)?;

            Python::with_gil(|py| {
                let py_positions: PyResult<Vec<Py<PyAny>>> = positions
                    .into_iter()
                    .map(|pos| {
                        let dict = PyDict::new(py);
                        dict.set_item("ticket", pos.ticket)?;
                        dict.set_item("symbol", pos.symbol.as_str())?;
                        dict.set_item("type_position", pos.type_position)?;
                        dict.set_item("volume", pos.volume)?;
                        dict.set_item("price_open", pos.price_open)?;
                        dict.set_item("sl", pos.sl)?;
                        dict.set_item("tp", pos.tp)?;
                        dict.set_item("profit", pos.profit)?;
                        dict.set_item("comment", pos.comment.as_str())?;
                        dict.into_py_any(py)
                    })
                    .collect();

                PyList::new(py, py_positions?)
                    .unwrap()
                    .into_any()
                    .into_py_any(py)
            })
        })
    }

    #[pyo3(name = "submit_order")]
    fn py_submit_order<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
        order_type: String,
        volume: f64,
        price: f64,
        sl: f64,
        tp: f64,
        comment: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        use crate::common::Mt5OrderType;
        use crate::websocket::messages::Mt5TradeRequest;

        let client = self.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Convert order type string to enum
            let mt5_order_type = match order_type.as_str() {
                "ORDER_TYPE_BUY" => Mt5OrderType::OrderTypeBuy,
                "ORDER_TYPE_SELL" => Mt5OrderType::OrderTypeSell,
                "ORDER_TYPE_BUY_LIMIT" => Mt5OrderType::OrderTypeBuyLimit,
                "ORDER_TYPE_SELL_LIMIT" => Mt5OrderType::OrderTypeSellLimit,
                "ORDER_TYPE_BUY_STOP" => Mt5OrderType::OrderTypeBuyStop,
                "ORDER_TYPE_SELL_STOP" => Mt5OrderType::OrderTypeSellStop,
                _ => {
                    return Err(to_pyruntime_err(crate::websocket::client::Mt5Error::Parse(
                        format!("Unknown order type: {}", order_type),
                    )))
                }
            };

            let request = Mt5TradeRequest {
                action: ustr::Ustr::from("TRADE"),
                action_type: mt5_order_type,
                symbol: symbol.into(),
                volume,
                price,
                stoploss: sl,
                takeprofit: tp,
                deviation: 0.0,
                comment: comment.into(),
                expiration: 0,
            };

            let response = client.submit_order(request).await.map_err(to_pyruntime_err)?;

            Python::with_gil(|py| {
                let dict = PyDict::new(py);
                dict.set_item("error", response.error)?;
                dict.set_item("retcode", response.retcode)?;
                dict.set_item("description", response.description.as_str())?;
                dict.set_item("order", response.order)?;
                dict.set_item("volume", response.volume)?;
                dict.set_item("price", response.price)?;
                dict.set_item("bid", response.bid)?;
                dict.set_item("ask", response.ask)?;
                dict.set_item("function", response.function.as_str())?;
                dict.into_py_any(py)
            })
        })
    }

    #[pyo3(name = "cancel_order")]
    fn py_cancel_order<'py>(
        &self,
        py: Python<'py>,
        ticket: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let response = client.cancel_order(ticket).await.map_err(to_pyruntime_err)?;

            Python::with_gil(|py| {
                let dict = PyDict::new(py);
                dict.set_item("error", response.error)?;
                dict.set_item("retcode", response.retcode)?;
                dict.set_item("description", response.description.as_str())?;
                dict.set_item("order", response.order)?;
                dict.into_py_any(py)
            })
        })
    }
}

pub fn call_python(py: Python, callback: &Py<PyAny>, py_obj: Py<PyAny>) {
    if let Err(e) = callback.call1(py, (py_obj,)) {
        tracing::error!("Error calling Python: {e}");
    }
}
