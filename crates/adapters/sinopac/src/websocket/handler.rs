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

//! WebSocket feed handler for the Sinopac adapter.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use nautilus_network::websocket::WebSocketClient;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::messages::{WsIncomingMsg, WsSubscribeMsg};
use crate::common::enums::SinopacQuoteType;

/// Receives raw WebSocket frames from the `nautilus_network` channel handler,
/// deserializes them into `WsIncomingMsg`, and forwards to the message channel.
///
/// Detects the `__RECONNECTED__` sentinel from `nautilus_network` and
/// re-subscribes all active subscriptions after reconnection.
/// Ping/Pong and reconnection are handled by `nautilus_network::WebSocketClient`.
pub(crate) async fn feed_handler(
    mut raw_rx: mpsc::UnboundedReceiver<Message>,
    msg_tx: mpsc::UnboundedSender<WsIncomingMsg>,
    ws_client: Arc<tokio::sync::Mutex<Option<WebSocketClient>>>,
    subscriptions: Arc<Mutex<HashSet<(String, SinopacQuoteType)>>>,
) {
    log::debug!("Feed handler started");

    while let Some(raw_msg) = raw_rx.recv().await {
        match raw_msg {
            Message::Text(text) => {
                // Check for reconnection sentinel
                if text.as_str() == nautilus_network::RECONNECTED {
                    log::info!("Received WebSocket reconnected signal");
                    resubscribe_all(&ws_client, &subscriptions).await;
                    continue;
                }

                match serde_json::from_str::<WsIncomingMsg>(&text) {
                    Ok(msg) => {
                        if msg_tx.send(msg).is_err() {
                            log::debug!("Message receiver dropped, shutting down feed handler");
                            break;
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to deserialize WS message: {e}, raw: {text}");
                    }
                }
            }
            Message::Close(_) => {
                log::info!("WebSocket close frame received");
                break;
            }
            _ => {} // Ping/Pong handled by nautilus_network
        }
    }

    log::debug!("Feed handler ended");
}

/// Re-subscribes all tracked subscriptions after a reconnection.
async fn resubscribe_all(
    ws_client: &Arc<tokio::sync::Mutex<Option<WebSocketClient>>>,
    subscriptions: &Arc<Mutex<HashSet<(String, SinopacQuoteType)>>>,
) {
    let subs_snapshot: Vec<(String, SinopacQuoteType)> = {
        let guard = subscriptions.lock().unwrap();
        guard.iter().cloned().collect()
    };

    if subs_snapshot.is_empty() {
        return;
    }

    log::info!(
        "Re-subscribing {} topics after reconnection",
        subs_snapshot.len()
    );

    let guard = ws_client.lock().await;
    if let Some(client) = guard.as_ref() {
        for (code, quote_type) in subs_snapshot {
            let msg = WsSubscribeMsg {
                action: "subscribe".to_string(),
                contract_code: code.clone(),
                quote_type,
            };
            let text = serde_json::to_string(&msg).expect("serialize subscribe");

            if let Err(e) = client.send_text(text, None).await {
                log::error!("Failed to re-subscribe {code}: {e}");
            }
        }
    }
}
