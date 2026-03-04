use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsFrame;
use tracing::{debug, error, warn};

use super::{messages::WsIncomingMsg, WsCommand};

/// Background task that manages the WebSocket connection.
///
/// Reads frames from the WS stream, deserializes to `WsIncomingMsg`,
/// and forwards to the message channel. Also processes subscribe/unsubscribe
/// commands from the command channel.
pub async fn ws_handler_loop<S>(
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
    debug!("WebSocket handler loop started");

    loop {
        tokio::select! {
            // Process incoming WS frames
            frame = stream.next() => {
                match frame {
                    Some(Ok(WsFrame::Text(text))) => {
                        match serde_json::from_str::<WsIncomingMsg>(&text) {
                            Ok(msg) => {
                                if msg_tx.send(msg).is_err() {
                                    debug!("Message receiver dropped, shutting down");
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("Failed to deserialize WS message: {e}, raw: {text}");
                            }
                        }
                    }
                    Some(Ok(WsFrame::Close(_))) => {
                        debug!("Received WS close frame");
                        break;
                    }
                    Some(Ok(WsFrame::Ping(data))) => {
                        if let Err(e) = sink.send(WsFrame::Pong(data)).await {
                            error!("Failed to send pong: {e}");
                            break;
                        }
                    }
                    Some(Ok(_)) => {} // Ignore other frame types
                    Some(Err(e)) => {
                        error!("WebSocket error: {e}");
                        break;
                    }
                    None => {
                        debug!("WebSocket stream ended");
                        break;
                    }
                }
            }

            // Process outgoing commands
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(WsCommand::Subscribe { code, quote_type }) => {
                        let msg = serde_json::json!({
                            "action": "subscribe",
                            "contract_code": code,
                            "quote_type": quote_type,
                        });
                        if let Err(e) = sink.send(WsFrame::Text(msg.to_string().into())).await {
                            error!("Failed to send subscribe: {e}");
                            break;
                        }
                    }
                    Some(WsCommand::Unsubscribe { code, quote_type }) => {
                        let msg = serde_json::json!({
                            "action": "unsubscribe",
                            "contract_code": code,
                            "quote_type": quote_type,
                        });
                        if let Err(e) = sink.send(WsFrame::Text(msg.to_string().into())).await {
                            error!("Failed to send unsubscribe: {e}");
                            break;
                        }
                    }
                    Some(WsCommand::Close) | None => {
                        debug!("Close command received");
                        let _ = sink.send(WsFrame::Close(None)).await;
                        break;
                    }
                }
            }
        }
    }

    is_connected.store(false, Ordering::SeqCst);
    debug!("WebSocket handler loop ended");
}
