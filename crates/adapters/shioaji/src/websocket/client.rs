use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::mpsc;
use tracing::{debug, info};

use super::{
    error::ShioajiWsError,
    handler::ws_handler_loop,
    messages::WsIncomingMsg,
    WsCommand,
};
use crate::common::consts::SHIOAJI_GATEWAY_WS_URL;

/// WebSocket client for streaming market data and order updates
/// from the Shioaji FastAPI gateway.
pub struct ShioajiWebSocketClient {
    url: String,
    cmd_tx: Option<mpsc::UnboundedSender<WsCommand>>,
    msg_rx: Option<mpsc::UnboundedReceiver<WsIncomingMsg>>,
    is_connected: Arc<AtomicBool>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ShioajiWebSocketClient {
    /// Creates a new [`ShioajiWebSocketClient`].
    #[must_use]
    pub fn new(url: Option<String>) -> Self {
        let url = url.unwrap_or_else(|| SHIOAJI_GATEWAY_WS_URL.to_string());
        Self {
            url,
            cmd_tx: None,
            msg_rx: None,
            is_connected: Arc::new(AtomicBool::new(false)),
            task_handle: None,
        }
    }

    /// Connects to the gateway WebSocket endpoint.
    pub async fn connect(&mut self) -> Result<(), ShioajiWsError> {
        if self.is_connected() {
            return Ok(());
        }

        info!("Connecting to WebSocket: {}", self.url);

        let (ws_stream, _response) = tokio_tungstenite::connect_async(&self.url)
            .await
            .map_err(|e| ShioajiWsError::Connection(e.to_string()))?;

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let is_connected = self.is_connected.clone();

        let handle = tokio::spawn(async move {
            ws_handler_loop(ws_stream, cmd_rx, msg_tx, is_connected).await;
        });

        self.cmd_tx = Some(cmd_tx);
        self.msg_rx = Some(msg_rx);
        self.task_handle = Some(handle);

        debug!("WebSocket connected");
        Ok(())
    }

    /// Disconnects from the gateway.
    pub async fn disconnect(&mut self) -> Result<(), ShioajiWsError> {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(WsCommand::Close);
        }
        if let Some(handle) = self.task_handle.take() {
            let _ = handle.await;
        }
        self.msg_rx = None;
        debug!("WebSocket disconnected");
        Ok(())
    }

    /// Returns whether the client is currently connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    /// Subscribes to quote data for a contract.
    pub fn subscribe(&self, code: &str, quote_type: &str) -> Result<(), ShioajiWsError> {
        let tx = self.cmd_tx.as_ref().ok_or(ShioajiWsError::NotConnected)?;
        tx.send(WsCommand::Subscribe {
            code: code.to_string(),
            quote_type: quote_type.to_string(),
        })
        .map_err(|e| ShioajiWsError::Send(e.to_string()))
    }

    /// Unsubscribes from quote data for a contract.
    pub fn unsubscribe(&self, code: &str, quote_type: &str) -> Result<(), ShioajiWsError> {
        let tx = self.cmd_tx.as_ref().ok_or(ShioajiWsError::NotConnected)?;
        tx.send(WsCommand::Unsubscribe {
            code: code.to_string(),
            quote_type: quote_type.to_string(),
        })
        .map_err(|e| ShioajiWsError::Send(e.to_string()))
    }

    /// Reads the next parsed message from the WebSocket.
    pub async fn next_message(&mut self) -> Option<WsIncomingMsg> {
        self.msg_rx.as_mut()?.recv().await
    }
}
