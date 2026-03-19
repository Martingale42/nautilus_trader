// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
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
//! WebSocket client for Sinopac gateway streaming data.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use nautilus_common::live::get_runtime;
use tokio::sync::mpsc;

use super::{WsCommand, error::SinopacWsError, handler::ws_handler_loop, messages::WsIncomingMsg};
use crate::common::consts::SINOPAC_GATEWAY_WS_URL;

/// WebSocket client for streaming market data and order updates
/// from the Sinopac FastAPI gateway.
///
/// Uses interior mutability (`Mutex`) for connection state so that
/// PyO3 `#[pymethods]` (which receive `&self`) can connect/disconnect.
#[derive(Clone)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_pyo3.sinopac", skip_from_py_object)
)]
pub struct SinopacWebSocketClient {
    url: String,
    cmd_tx: Arc<Mutex<Option<mpsc::UnboundedSender<WsCommand>>>>,
    msg_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<WsIncomingMsg>>>>,
    is_connected: Arc<AtomicBool>,
    task_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl SinopacWebSocketClient {
    /// Creates a new [`SinopacWebSocketClient`].
    #[must_use]
    pub fn new(url: Option<String>) -> Self {
        let url = url.unwrap_or_else(|| SINOPAC_GATEWAY_WS_URL.to_string());
        Self {
            url,
            cmd_tx: Arc::new(Mutex::new(None)),
            msg_rx: Arc::new(Mutex::new(None)),
            is_connected: Arc::new(AtomicBool::new(false)),
            task_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Returns the WS URL.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Connects to the gateway WebSocket endpoint.
    pub async fn connect(&self) -> Result<(), SinopacWsError> {
        if self.is_connected() {
            return Ok(());
        }

        log::info!("Connecting to WebSocket: {}", self.url);

        let (ws_stream, _response) = tokio_tungstenite::connect_async(&self.url)
            .await
            .map_err(|e| SinopacWsError::Connection(e.to_string()))?;

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let is_connected = self.is_connected.clone();

        let handle = get_runtime().spawn(async move {
            ws_handler_loop(ws_stream, cmd_rx, msg_tx, is_connected).await;
        });

        *self.cmd_tx.lock().unwrap() = Some(cmd_tx);
        *self.msg_rx.lock().unwrap() = Some(msg_rx);
        *self.task_handle.lock().unwrap() = Some(handle);

        log::debug!("WebSocket connected");
        Ok(())
    }

    /// Disconnects from the gateway.
    pub async fn disconnect(&self) -> Result<(), SinopacWsError> {
        if let Some(tx) = self.cmd_tx.lock().unwrap().take() {
            let _ = tx.send(WsCommand::Close);
        }
        let handle = self.task_handle.lock().unwrap().take();
        if let Some(handle) = handle {
            let _ = handle.await;
        }
        *self.msg_rx.lock().unwrap() = None;
        log::debug!("WebSocket disconnected");
        Ok(())
    }

    /// Returns whether the client is currently connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    /// Subscribes to quote data for a contract.
    pub fn subscribe(&self, code: &str, quote_type: &str) -> Result<(), SinopacWsError> {
        let guard = self.cmd_tx.lock().unwrap();
        let tx = guard.as_ref().ok_or(SinopacWsError::NotConnected)?;
        tx.send(WsCommand::Subscribe {
            code: code.to_string(),
            quote_type: quote_type.to_string(),
        })
        .map_err(|e| SinopacWsError::Send(e.to_string()))
    }

    /// Unsubscribes from quote data for a contract.
    pub fn unsubscribe(&self, code: &str, quote_type: &str) -> Result<(), SinopacWsError> {
        let guard = self.cmd_tx.lock().unwrap();
        let tx = guard.as_ref().ok_or(SinopacWsError::NotConnected)?;
        tx.send(WsCommand::Unsubscribe {
            code: code.to_string(),
            quote_type: quote_type.to_string(),
        })
        .map_err(|e| SinopacWsError::Send(e.to_string()))
    }

    /// Takes the message receiver out of the client.
    ///
    /// This is used by the PyO3 connect method to move `msg_rx`
    /// into a spawned callback task. Returns `None` if already taken.
    pub fn take_msg_rx(&self) -> Option<mpsc::UnboundedReceiver<WsIncomingMsg>> {
        self.msg_rx.lock().unwrap().take()
    }

    /// Reads the next parsed message from the WebSocket.
    pub async fn next_message(&self) -> Option<WsIncomingMsg> {
        let rx = {
            let mut guard = self.msg_rx.lock().unwrap();
            guard.take()
        };
        let mut rx = rx?;
        let msg = rx.recv().await;
        self.msg_rx.lock().unwrap().replace(rx);
        msg
    }
}
