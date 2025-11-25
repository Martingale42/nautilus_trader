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
    enums::{OrderSide, OrderType},
    identifiers::{ClientOrderId, InstrumentId},
    instruments::Instrument,
    python::{
        data::data_to_pycapsule,
        instruments::{instrument_any_to_pyobject, pyobject_to_instrument_any},
    },
    types::{Price, Quantity},
};
use pyo3::{
    conversion::IntoPyObjectExt,
    prelude::*,
    types::{PyDict, PyList},
};

use crate::websocket::client::{Mt5Client, Mt5ClientConfig, NautilusMessage};

#[pymethods]
impl Mt5Client {
    #[new]
    #[pyo3(signature = (host=None, data_port=None, live_port=None, stream_port=None, sys_port=None, account_id=None))]
    fn py_new(
        host: Option<String>,
        data_port: Option<u16>,
        live_port: Option<u16>,
        stream_port: Option<u16>,
        sys_port: Option<u16>,
        account_id: Option<String>,
    ) -> PyResult<Self> {
        let config = if let Some(host) = host {
            Mt5ClientConfig {
                host,
                data_port: data_port.unwrap_or(2202),
                live_port: live_port.unwrap_or(2203),
                stream_port: stream_port.unwrap_or(2204),
                sys_port: sys_port.unwrap_or(2201),
            }
        } else {
            Mt5ClientConfig::default()
        };

        // Account ID is now stored as a string, the client will add the venue prefix
        Ok(Self::new(config, account_id))
    }

    #[getter]
    #[pyo3(name = "host")]
    #[must_use]
    pub fn py_host(&self) -> String {
        self.config.host.clone()
    }

    #[getter]
    #[pyo3(name = "data_port")]
    #[must_use]
    pub const fn py_data_port(&self) -> u16 {
        self.config.data_port
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

    #[pyo3(name = "is_initialized")]
    fn py_is_initialized(&self) -> bool {
        self.is_initialized()
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

    #[pyo3(name = "subscribe_quotes")]
    fn py_subscribe_quotes(&self, py: Python, instrument_ids: Vec<Py<PyAny>>) -> PyResult<()> {
        let ids: PyResult<Vec<nautilus_model::identifiers::InstrumentId>> = instrument_ids
            .into_iter()
            .map(|id| {
                let id_str: String = id.extract(py)?;
                nautilus_model::identifiers::InstrumentId::from_as_ref(&id_str)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
            })
            .collect();
        self.subscribe_quotes(ids?).map_err(to_pyruntime_err)
    }

    #[pyo3(name = "subscribe_bars")]
    fn py_subscribe_bars(
        &self,
        symbol: String,
        timeframe: String,
        bar_type_str: String,
    ) -> PyResult<()> {
        self.subscribe_bars(&symbol, &timeframe, &bar_type_str)
            .map_err(to_pyruntime_err)
    }

    #[pyo3(name = "subscribe")]
    fn py_subscribe(&self, symbols: Vec<String>) -> PyResult<()> {
        #[allow(deprecated)]
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

        // Clone client to keep it alive during async operation
        // This prevents Drop from being called and disconnecting the ZMQ thread
        let client = self.clone();

        // Start streaming BEFORE creating the async future
        // This ensures is_running is set to true immediately (fixes race condition)
        let stream = client.stream();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tracing::info!("py_connect: async block executing");

            // Spawn background task to process messages
            let task_handle = tokio::spawn(async move {
                // CRITICAL: Capture client first to keep it alive for the entire task
                // This prevents Drop from being called while the stream is active
                let _client_guard = client;

                tracing::info!("Tokio task started - processing messages");
                tokio::pin!(stream);

                let mut msg_count = 0;
                while let Some(msg) = stream.next().await {
                    msg_count += 1;
                    tracing::debug!("Received message #{} from stream", msg_count);

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

                tracing::warn!("Tokio task: stream ended after {} messages", msg_count);
                // _client_guard is dropped here, calling disconnect() and cleaning up
            });

            tracing::info!("py_connect: tokio task spawned, returning Ok");
            Ok(())
        })
    }

    #[pyo3(name = "close")]
    fn py_close(&self) {
        self.close();
    }

    #[pyo3(name = "disconnect")]
    fn py_disconnect(&self) {
        // Deprecated: Use close() instead
        self.close();
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

            Python::attach(|py| {
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

            Python::attach(|py| {
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

            Python::attach(|py| {
                let py_orders: PyResult<Vec<Py<PyAny>>> = orders
                    .into_iter()
                    .map(|order| {
                        let dict = PyDict::new(py);
                        dict.set_item("ticket", order.ticket)?;
                        dict.set_item("symbol", order.symbol.as_str())?;
                        dict.set_item("type", order.type_.as_str())?;
                        dict.set_item("state", order.state.as_str())?;
                        dict.set_item("volume_initial", order.volume_initial)?;
                        dict.set_item("volume_current", order.volume_current)?;
                        dict.set_item("price_open", order.price_open)?;
                        dict.set_item("stoploss", order.stoploss)?;
                        dict.set_item("takeprofit", order.takeprofit)?;
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
            let positions = client.request_positions().await.map_err(to_pyruntime_err)?;

            Python::attach(|py| {
                let py_positions: PyResult<Vec<Py<PyAny>>> = positions
                    .into_iter()
                    .map(|pos| {
                        let dict = PyDict::new(py);
                        dict.set_item("id", pos.id)?;
                        dict.set_item("magic", pos.magic)?;
                        dict.set_item("symbol", pos.symbol.as_str())?;
                        dict.set_item("type", pos.type_.as_str())?;
                        dict.set_item("time_setup", pos.time_setup)?;
                        dict.set_item("open", pos.open)?;
                        dict.set_item("volume", pos.volume)?;
                        dict.set_item("stoploss", pos.stoploss)?;
                        dict.set_item("takeprofit", pos.takeprofit)?;
                        dict.set_item("profit", pos.profit)?;
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

    #[pyo3(name = "request_bars")]
    fn py_request_bars<'py>(
        &self,
        py: Python<'py>,
        symbol: String,
        timeframe: String,
        start: Option<i64>,
        end: Option<i64>,
        count: Option<i32>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let history = client
                .request_bars(&symbol, &timeframe, start, end, count)
                .await
                .map_err(to_pyruntime_err)?;

            Python::attach(|py| {
                // Convert Mt5HistoryMsg to Python dict
                // Format matches actual MT5 response: {"symbol": str, "timeframe": str, "data": [[...], [...]]}
                let dict = PyDict::new(py);
                dict.set_item("symbol", history.symbol.as_str())?;
                dict.set_item("timeframe", history.timeframe.as_str())?;

                // Convert data arrays (Vec<Vec<f64>>) to Python list of lists
                // For bars: [timestamp_sec, open, high, low, close, volume]
                // For ticks: [timestamp_ms, bid, ask]
                let py_data: PyResult<Vec<Py<PyAny>>> = history
                    .data
                    .into_iter()
                    .map(|array| PyList::new(py, array).unwrap().into_any().into_py_any(py))
                    .collect();

                dict.set_item("data", PyList::new(py, py_data?).unwrap())?;
                dict.into_py_any(py)
            })
        })
    }

    #[pyo3(name = "submit_order")]
    fn py_submit_order<'py>(
        &self,
        py: Python<'py>,
        instrument_id: InstrumentId,
        client_order_id: ClientOrderId,
        order_side: OrderSide,
        order_type: OrderType,
        quantity: Quantity,
        price: Option<Price>,
        sl: Option<Price>,
        tp: Option<Price>,
        comment: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let response = client
                .submit_order(
                    instrument_id,
                    client_order_id,
                    order_side,
                    order_type,
                    quantity,
                    price,
                    sl,
                    tp,
                    comment,
                )
                .await
                .map_err(to_pyruntime_err)?;

            Python::attach(|py| {
                let dict = PyDict::new(py);
                dict.set_item("error", response.error)?;
                dict.set_item("retcode", response.retcode)?;
                dict.set_item("description", response.desription.as_str())?; // MT5's typo
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
    fn py_cancel_order<'py>(&self, py: Python<'py>, ticket: i64) -> PyResult<Bound<'py, PyAny>> {
        use nautilus_model::identifiers::VenueOrderId;

        let client = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let venue_order_id = VenueOrderId::new(&ticket.to_string());
            let response = client
                .cancel_order(venue_order_id)
                .await
                .map_err(to_pyruntime_err)?;

            Python::attach(|py| {
                let dict = PyDict::new(py);
                dict.set_item("error", response.error)?;
                dict.set_item("retcode", response.retcode)?;
                dict.set_item("description", response.desription.as_str())?; // MT5's typo
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
