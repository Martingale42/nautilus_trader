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

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use nautilus_network::{
    mode::ConnectionMode,
    websocket::{WebSocketClient, WebSocketConfig, channel_message_handler},
};
use tokio::sync::mpsc;

use super::{
    error::SinopacWsError,
    handler::feed_handler,
    messages::{WsIncomingMsg, WsSubscribeMsg},
};
use crate::common::{consts::SINOPAC_GATEWAY_WS_URL, enums::SinopacQuoteType};

/// WebSocket client for streaming market data and order updates
/// from the Sinopac FastAPI gateway.
///
/// Wraps `nautilus_network::WebSocketClient` for automatic reconnection
/// support. Uses interior mutability for connection state so that
/// PyO3 `#[pymethods]` (which receive `&self`) can connect/disconnect.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_pyo3.sinopac", skip_from_py_object)
)]
pub struct SinopacWebSocketClient {
    url: String,
    ws_client: Arc<tokio::sync::Mutex<Option<WebSocketClient>>>,
    msg_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<WsIncomingMsg>>>>,
    subscriptions: Arc<Mutex<HashSet<(String, SinopacQuoteType)>>>,
    feed_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl Clone for SinopacWebSocketClient {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            ws_client: Arc::clone(&self.ws_client),
            msg_rx: Arc::clone(&self.msg_rx),
            subscriptions: Arc::clone(&self.subscriptions),
            feed_handle: Arc::clone(&self.feed_handle),
        }
    }
}

impl SinopacWebSocketClient {
    /// Creates a new [`SinopacWebSocketClient`].
    #[must_use]
    pub fn new(url: Option<String>) -> Self {
        let url = url.unwrap_or_else(|| SINOPAC_GATEWAY_WS_URL.to_string());
        Self {
            url,
            ws_client: Arc::new(tokio::sync::Mutex::new(None)),
            msg_rx: Arc::new(Mutex::new(None)),
            subscriptions: Arc::new(Mutex::new(HashSet::new())),
            feed_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Returns the WS URL.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Connects to the gateway WebSocket endpoint.
    ///
    /// Creates a `nautilus_network::WebSocketClient` in handler mode with
    /// automatic reconnection, spawns a feed handler task that deserializes
    /// raw frames into `WsIncomingMsg`, and re-subscribes after reconnections.
    pub async fn connect(&self) -> Result<(), SinopacWsError> {
        if self.is_connected().await {
            return Ok(());
        }

        log::info!("Connecting to WebSocket: {}", self.url);

        let (raw_handler, raw_rx) = channel_message_handler();

        let config = WebSocketConfig {
            url: self.url.clone(),
            headers: vec![],
            heartbeat: Some(30),
            heartbeat_msg: None,
            reconnect_timeout_ms: Some(5_000),
            reconnect_delay_initial_ms: Some(500),
            reconnect_delay_max_ms: Some(5_000),
            reconnect_backoff_factor: Some(1.5),
            reconnect_jitter_ms: Some(250),
            reconnect_max_attempts: None,
            idle_timeout_ms: None,
        };

        let client = WebSocketClient::connect(
            config,
            Some(raw_handler),
            None,
            None, // Reconnection re-subscribe handled by feed handler
            vec![],
            None,
        )
        .await
        .map_err(|e| SinopacWsError::Connection(e.to_string()))?;

        // Spawn the feed handler that deserializes raw messages and
        // re-subscribes on reconnection
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let ws_client_ref = Arc::clone(&self.ws_client);
        let subs_ref = Arc::clone(&self.subscriptions);
        let handle = tokio::spawn(async move {
            feed_handler(raw_rx, msg_tx, ws_client_ref, subs_ref).await;
        });

        *self.ws_client.lock().await = Some(client);
        *self.msg_rx.lock().unwrap() = Some(msg_rx);
        *self.feed_handle.lock().unwrap() = Some(handle);

        log::debug!("WebSocket connected");
        Ok(())
    }

    /// Disconnects from the gateway.
    pub async fn disconnect(&self) -> Result<(), SinopacWsError> {
        // Abort the feed handler task
        if let Some(handle) = self.feed_handle.lock().unwrap().take() {
            handle.abort();
        }

        // Disconnect the network client
        if let Some(client) = self.ws_client.lock().await.take() {
            client.disconnect().await;
        }

        *self.msg_rx.lock().unwrap() = None;

        log::debug!("WebSocket disconnected");
        Ok(())
    }

    /// Returns whether the client is currently connected.
    pub async fn is_connected(&self) -> bool {
        let guard = self.ws_client.lock().await;
        guard
            .as_ref()
            .is_some_and(|c| c.connection_mode() == ConnectionMode::Active)
    }

    /// Subscribes to quote data for a contract.
    pub async fn subscribe(
        &self,
        code: &str,
        quote_type: SinopacQuoteType,
    ) -> Result<(), SinopacWsError> {
        let msg = WsSubscribeMsg {
            action: "subscribe".to_string(),
            contract_code: code.to_string(),
            quote_type,
        };
        let text = serde_json::to_string(&msg).map_err(|e| SinopacWsError::Json(e.to_string()))?;

        {
            let guard = self.ws_client.lock().await;
            let client = guard.as_ref().ok_or(SinopacWsError::NotConnected)?;
            client
                .send_text(text, None)
                .await
                .map_err(|e| SinopacWsError::Send(e.to_string()))?;
        }

        // Track the subscription for post-reconnection re-subscribe
        self.subscriptions
            .lock()
            .unwrap()
            .insert((code.to_string(), quote_type));

        Ok(())
    }

    /// Unsubscribes from quote data for a contract.
    pub async fn unsubscribe(
        &self,
        code: &str,
        quote_type: SinopacQuoteType,
    ) -> Result<(), SinopacWsError> {
        let msg = WsSubscribeMsg {
            action: "unsubscribe".to_string(),
            contract_code: code.to_string(),
            quote_type,
        };
        let text = serde_json::to_string(&msg).map_err(|e| SinopacWsError::Json(e.to_string()))?;

        {
            let guard = self.ws_client.lock().await;
            let client = guard.as_ref().ok_or(SinopacWsError::NotConnected)?;
            client
                .send_text(text, None)
                .await
                .map_err(|e| SinopacWsError::Send(e.to_string()))?;
        }

        // Remove from tracked subscriptions
        self.subscriptions
            .lock()
            .unwrap()
            .remove(&(code.to_string(), quote_type));

        Ok(())
    }

    /// Takes the message receiver out of the client.
    ///
    /// Returns the parsed message receiver for use by the PyO3 connect
    /// method to move into a spawned callback task. Returns `None` if
    /// already taken.
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
