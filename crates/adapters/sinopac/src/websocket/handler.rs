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
//! WebSocket message handler loop for the Sinopac adapter.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsFrame;

use super::{
    WsCommand,
    messages::{WsIncomingMsg, WsSubscribeMsg},
};

/// Background task that manages the WebSocket connection.
///
/// Reads frames from the WS stream, deserializes to `WsIncomingMsg`,
/// and forwards to the message channel. Also processes subscribe/unsubscribe
/// commands from the command channel.
pub(crate) async fn ws_handler_loop<S>(
    ws_stream: S,
    mut cmd_rx: mpsc::UnboundedReceiver<WsCommand>,
    msg_tx: mpsc::UnboundedSender<WsIncomingMsg>,
    is_connected: Arc<AtomicBool>,
) where
    S: StreamExt<Item = Result<WsFrame, tokio_tungstenite::tungstenite::Error>>
        + SinkExt<WsFrame>
        + Unpin,
    <S as futures_util::Sink<WsFrame>>::Error: std::fmt::Display,
{
    let (mut sink, mut stream) = ws_stream.split();
    is_connected.store(true, Ordering::SeqCst);
    log::debug!("WebSocket handler loop started");

    loop {
        tokio::select! {
            // Process incoming WS frames
            frame = stream.next() => {
                match frame {
                    Some(Ok(WsFrame::Text(text))) => {
                        match serde_json::from_str::<WsIncomingMsg>(&text) {
                            Ok(msg) => {
                                if msg_tx.send(msg).is_err() {
                                    log::debug!("Message receiver dropped, shutting down");
                                    break;
                                }
                            }
                            Err(e) => {
                                log::warn!("Failed to deserialize WS message: {e}, raw: {text}");
                            }
                        }
                    }
                    Some(Ok(WsFrame::Close(_))) => {
                        log::debug!("Received WS close frame");
                        break;
                    }
                    Some(Ok(WsFrame::Ping(data))) => {
                        if let Err(e) = sink.send(WsFrame::Pong(data)).await {
                            log::error!("Failed to send pong: {e}");
                            break;
                        }
                    }
                    Some(Ok(_)) => {} // Ignore other frame types
                    Some(Err(e)) => {
                        log::error!("WebSocket error: {e}");
                        break;
                    }
                    None => {
                        log::debug!("WebSocket stream ended");
                        break;
                    }
                }
            }

            // Process outgoing commands
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(WsCommand::Subscribe { code, quote_type }) => {
                        let msg = WsSubscribeMsg {
                            action: "subscribe".to_string(),
                            contract_code: code,
                            quote_type,
                        };
                        let text = serde_json::to_string(&msg).expect("serialize subscribe");
                        if let Err(e) = sink.send(WsFrame::Text(text.into())).await {
                            log::error!("Failed to send subscribe: {e}");
                            break;
                        }
                    }
                    Some(WsCommand::Unsubscribe { code, quote_type }) => {
                        let msg = WsSubscribeMsg {
                            action: "unsubscribe".to_string(),
                            contract_code: code,
                            quote_type,
                        };
                        let text = serde_json::to_string(&msg).expect("serialize unsubscribe");
                        if let Err(e) = sink.send(WsFrame::Text(text.into())).await {
                            log::error!("Failed to send unsubscribe: {e}");
                            break;
                        }
                    }
                    Some(WsCommand::Close) | None => {
                        log::debug!("Close command received");
                        let _ = sink.send(WsFrame::Close(None)).await;
                        break;
                    }
                }
            }
        }
    }

    is_connected.store(false, Ordering::SeqCst);
    log::debug!("WebSocket handler loop ended");
}
