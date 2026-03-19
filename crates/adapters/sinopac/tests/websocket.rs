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

//! Integration tests for the Sinopac WebSocket client using a mock Axum server.

use std::{net::SocketAddr, path::PathBuf};

use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use nautilus_sinopac::websocket::{client::SinopacWebSocketClient, messages::WsIncomingMsg};
use rstest::rstest;

fn load_test_json(filename: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join(filename);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Failed to load test fixture: {}", path.display()))
}

/// Mock WebSocket handler that:
/// - Responds to subscribe commands with an ack
/// - Sends tick/bidask data from test fixtures based on the quote_type
async fn ws_handler(ws: WebSocket) {
    use futures_util::{SinkExt, StreamExt};

    let (mut sink, mut stream) = ws.split();

    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(text) => {
                if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                    let action = cmd.get("action").and_then(|v| v.as_str()).unwrap_or("");
                    let code = cmd
                        .get("contract_code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("2330");
                    let quote_type = cmd
                        .get("quote_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if action == "subscribe" {
                        // Send subscription confirmation
                        let ack = serde_json::json!({
                            "type": "subscribed",
                            "code": code,
                            "quote_type": quote_type
                        });
                        let _ = sink.send(Message::Text(ack.to_string().into())).await;

                        // Send data based on quote_type
                        let data = match quote_type {
                            "tick" => load_test_json("ws_tick_stock.json"),
                            "bidask" => load_test_json("ws_bidask.json"),
                            _ => continue,
                        };
                        let _ = sink.send(Message::Text(data.into())).await;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

async fn ws_upgrade(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(ws_handler)
}

async fn start_ws_server() -> SocketAddr {
    let router = Router::new().route("/ws", get(ws_upgrade));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });

    addr
}

fn create_ws_url(addr: SocketAddr) -> String {
    format!("ws://{addr}/ws")
}

#[rstest]
#[tokio::test]
async fn test_connect_disconnect() {
    let addr = start_ws_server().await;
    let url = create_ws_url(addr);
    let client = SinopacWebSocketClient::new(Some(url));

    assert!(!client.is_connected());

    client.connect().await.expect("connect failed");

    // The handler loop sets is_connected in a spawned task; wait for it.
    for _ in 0..50 {
        if client.is_connected() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(client.is_connected());

    client.disconnect().await.expect("disconnect failed");
    assert!(!client.is_connected());
}

#[rstest]
#[tokio::test]
async fn test_subscribe_tick() {
    let addr = start_ws_server().await;
    let url = create_ws_url(addr);
    let client = SinopacWebSocketClient::new(Some(url));

    client.connect().await.expect("connect failed");
    client
        .subscribe("2330", "tick")
        .expect("subscribe failed");

    // First message should be the subscription confirmation
    let msg = client.next_message().await.expect("expected a message");
    match msg {
        WsIncomingMsg::Subscribed(confirm) => {
            assert_eq!(confirm.code, "2330");
            assert_eq!(confirm.quote_type, "tick");
        }
        other => panic!("Expected Subscribed, got: {other:?}"),
    }

    // Second message should be tick data
    let msg = client.next_message().await.expect("expected tick data");
    match msg {
        WsIncomingMsg::Tick(tick) => {
            assert_eq!(tick.code, "2330");
            assert_eq!(tick.data.close, 580.0);
        }
        other => panic!("Expected Tick, got: {other:?}"),
    }

    client.disconnect().await.expect("disconnect failed");
}

#[rstest]
#[tokio::test]
async fn test_subscribe_bidask() {
    let addr = start_ws_server().await;
    let url = create_ws_url(addr);
    let client = SinopacWebSocketClient::new(Some(url));

    client.connect().await.expect("connect failed");
    client
        .subscribe("2330", "bidask")
        .expect("subscribe failed");

    // First message should be the subscription confirmation
    let msg = client.next_message().await.expect("expected a message");
    match msg {
        WsIncomingMsg::Subscribed(confirm) => {
            assert_eq!(confirm.code, "2330");
            assert_eq!(confirm.quote_type, "bidask");
        }
        other => panic!("Expected Subscribed, got: {other:?}"),
    }

    // Second message should be bidask data
    let msg = client.next_message().await.expect("expected bidask data");
    match msg {
        WsIncomingMsg::BidAsk(ba) => {
            assert_eq!(ba.code, "2330");
            assert_eq!(ba.data.bid_price.len(), 5);
            assert_eq!(ba.data.ask_price.len(), 5);
        }
        other => panic!("Expected BidAsk, got: {other:?}"),
    }

    client.disconnect().await.expect("disconnect failed");
}
