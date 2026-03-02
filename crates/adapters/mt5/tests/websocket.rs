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

//! Integration tests for MT5 WebSocket message handling.

use std::{path::PathBuf, str::FromStr};

use nautilus_core::UnixNanos;
use nautilus_model::{
    data::BarType,
    identifiers::{InstrumentId, Symbol, Venue},
    instruments::{CurrencyPair, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use nautilus_mt5::websocket::{
    messages::{Mt5LiveBarMsg, Mt5LiveTickMsg},
    parse::{parse_mt5_live_bar, parse_mt5_live_tick_to_quote},
};

/// Helper to load test fixtures
fn load_fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join(name);
    std::fs::read_to_string(path).expect("Failed to load fixture")
}

/// Create a test instrument for parsing
fn create_test_instrument(symbol: &str, price_precision: u8) -> InstrumentAny {
    let raw_symbol = Symbol::new(symbol);
    let instrument_id = InstrumentId::new(raw_symbol, Venue::new("MT5"));
    let base_currency = Currency::USD();
    let quote_currency = Currency::USD();

    // Calculate appropriate price increment based on precision
    // precision=2 -> 0.01, precision=5 -> 0.00001
    let price_increment_value = 1.0 / 10_f64.powi(price_precision as i32);

    InstrumentAny::CurrencyPair(CurrencyPair::new(
        instrument_id,
        raw_symbol,
        base_currency,
        quote_currency,
        price_precision,
        2,
        Price::new(price_increment_value, price_precision),
        Quantity::new(0.01, 2),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        UnixNanos::default(),
        UnixNanos::default(),
    ))
}

////////////////////////////////////////////////////////////////////////////////
// Message Deserialization Tests
////////////////////////////////////////////////////////////////////////////////

#[test]
fn test_deserialize_live_tick_message() {
    let json = load_fixture("ws_tick_single.json");
    let msg: Mt5LiveTickMsg = serde_json::from_str(&json).expect("Failed to parse tick message");

    assert_eq!(msg.status, "CONNECTED");
    assert_eq!(msg.symbol, "BTCUSD");
    assert_eq!(msg.timeframe, "TICK");
    assert_eq!(msg.data.len(), 3);
    assert!(msg.data[0] > 0.0); // timestamp
    assert!(msg.data[1] > 0.0); // bid
    assert!(msg.data[2] > 0.0); // ask
}

#[test]
fn test_deserialize_live_bar_message() {
    let json = load_fixture("ws_bar_m1_single.json");
    let msg: Mt5LiveBarMsg = serde_json::from_str(&json).expect("Failed to parse bar message");

    assert_eq!(msg.status, "CONNECTED");
    assert_eq!(msg.symbol, "XAUUSD.sml");
    assert_eq!(msg.timeframe, "M1");
    assert_eq!(msg.data.len(), 6);
    assert!(msg.data[0] > 0.0); // timestamp
}

#[test]
fn test_deserialize_multiple_tick_messages() {
    let json = load_fixture("ws_tick.json");
    let messages: Vec<Mt5LiveTickMsg> =
        serde_json::from_str(&json).expect("Failed to parse tick messages");

    assert!(!messages.is_empty());
    assert!(messages.len() > 10);

    for msg in messages.iter() {
        assert_eq!(msg.status, "CONNECTED");
        assert!(!msg.symbol.is_empty());
        assert_eq!(msg.timeframe, "TICK");
        assert_eq!(msg.data.len(), 3);
    }
}

#[test]
fn test_deserialize_account_response() {
    let json = load_fixture("http_get_account.json");
    let response: serde_json::Value =
        serde_json::from_str(&json).expect("Failed to parse account response");

    assert_eq!(response["error"], false);
    assert!(response["balance"].is_number());
    assert!(response["equity"].is_number());
    assert!(response["margin"].is_number());
}

#[test]
fn test_deserialize_positions_response() {
    let json = load_fixture("http_get_positions.json");
    let response: serde_json::Value =
        serde_json::from_str(&json).expect("Failed to parse positions response");

    assert_eq!(response["error"], false);
    let positions = response["positions"].as_array().expect("positions not array");
    assert!(!positions.is_empty());
}

#[test]
fn test_deserialize_instruments_response() {
    let json = load_fixture("http_get_instruments_minimal.json");
    let response: serde_json::Value =
        serde_json::from_str(&json).expect("Failed to parse instruments response");

    assert_eq!(response["error"], false);
    let symbols = response["symbols"].as_array().expect("symbols not array");
    assert_eq!(symbols.len(), 2);
}

#[test]
fn test_deserialize_error_response() {
    let json = load_fixture("http_error_response.json");
    let response: serde_json::Value =
        serde_json::from_str(&json).expect("Failed to parse error response");

    assert_eq!(response["error"], true);
    assert!(response["lastError"].is_string());
    assert!(response["description"].is_string());
}

////////////////////////////////////////////////////////////////////////////////
// End-to-End Parsing Pipeline Tests
////////////////////////////////////////////////////////////////////////////////

#[test]
fn test_tick_parsing_pipeline() {
    // Load fixture -> Deserialize -> Parse -> Validate
    let json = load_fixture("ws_tick_single.json");
    let msg: Mt5LiveTickMsg = serde_json::from_str(&json).expect("Deserialize failed");

    let instrument = create_test_instrument("BTCUSD", 2);
    let tick = parse_mt5_live_tick_to_quote(&msg, &instrument, UnixNanos::default())
        .expect("Parse failed");

    // Validate end-to-end result
    assert_eq!(tick.instrument_id.symbol.as_str(), "BTCUSD");
    assert_eq!(tick.instrument_id.venue.as_str(), "MT5");
    assert!(tick.bid_price.as_f64() > 0.0);
    assert!(tick.ask_price.as_f64() > 0.0);
    assert!(tick.ask_price.as_f64() >= tick.bid_price.as_f64());
    assert_eq!(tick.ts_event, 1764162271192000000);
}

#[test]
fn test_bar_parsing_pipeline() {
    // Load fixture -> Deserialize -> Parse -> Validate
    let json = load_fixture("ws_bar_m1_single.json");
    let msg: Mt5LiveBarMsg = serde_json::from_str(&json).expect("Deserialize failed");

    let instrument = create_test_instrument("XAUUSD", 2);
    let bar_type = BarType::from_str("XAUUSD.MT5-1-MINUTE-LAST-EXTERNAL").expect("BarType parse failed");
    let bar = parse_mt5_live_bar(&msg, &bar_type, &instrument, UnixNanos::default())
        .expect("Parse failed");

    // Validate end-to-end result
    assert_eq!(bar.bar_type.instrument_id().symbol.as_str(), "XAUUSD");
    assert!(bar.open.as_f64() > 0.0);
    assert!(bar.high.as_f64() >= bar.low.as_f64());
    assert!(bar.high.as_f64() >= bar.open.as_f64());
    assert!(bar.high.as_f64() >= bar.close.as_f64());
    assert!(bar.low.as_f64() <= bar.open.as_f64());
    assert!(bar.low.as_f64() <= bar.close.as_f64());
}

#[test]
fn test_batch_tick_parsing_pipeline() {
    // Test parsing multiple messages in sequence
    let json = load_fixture("ws_tick.json");
    let messages: Vec<Mt5LiveTickMsg> = serde_json::from_str(&json).expect("Deserialize failed");

    let instrument = create_test_instrument("BTCUSD", 2);

    let mut prev_timestamp = 0i64;
    for msg in messages.iter() {
        let tick = parse_mt5_live_tick_to_quote(msg, &instrument, UnixNanos::default())
            .expect("Parse failed");

        // Validate tick
        assert!(tick.bid_price.as_f64() > 0.0);
        assert!(tick.ask_price.as_f64() > 0.0);

        // Timestamps should be monotonically increasing (or equal)
        let current_timestamp = tick.ts_event.as_i64();
        assert!(current_timestamp >= prev_timestamp);
        prev_timestamp = current_timestamp;
    }
}

////////////////////////////////////////////////////////////////////////////////
// Data Integrity Tests
////////////////////////////////////////////////////////////////////////////////

#[test]
fn test_tick_data_integrity() {
    let json = load_fixture("ws_tick.json");
    let messages: Vec<Mt5LiveTickMsg> = serde_json::from_str(&json).expect("Deserialize failed");

    for msg in messages.iter() {
        // Validate timestamp is reasonable (after 2020)
        assert!(msg.data[0] > 1577836800000.0); // 2020-01-01 in ms

        // Bid and ask must be positive
        assert!(msg.data[1] > 0.0);
        assert!(msg.data[2] > 0.0);

        // Ask must be >= bid (spread check)
        assert!(msg.data[2] >= msg.data[1]);

        // Spread should be reasonable (< 10% of price)
        let spread = msg.data[2] - msg.data[1];
        let mid_price = (msg.data[1] + msg.data[2]) / 2.0;
        assert!(spread < mid_price * 0.1);
    }
}

#[test]
fn test_bar_data_integrity() {
    let json = load_fixture("ws_bar_m1_single.json");
    let msg: Mt5LiveBarMsg = serde_json::from_str(&json).expect("Deserialize failed");

    // Validate timestamp is reasonable (after 2020)
    assert!(msg.data[0] > 1577836800.0); // 2020-01-01 in seconds

    let open = msg.data[1];
    let high = msg.data[2];
    let low = msg.data[3];
    let close = msg.data[4];
    let volume = msg.data[5];

    // OHLC relationships
    assert!(high >= open);
    assert!(high >= low);
    assert!(high >= close);
    assert!(low <= open);
    assert!(low <= close);

    // Volume must be non-negative
    assert!(volume >= 0.0);

    // Prices must be positive
    assert!(open > 0.0);
    assert!(high > 0.0);
    assert!(low > 0.0);
    assert!(close > 0.0);
}

////////////////////////////////////////////////////////////////////////////////
// Error Handling Tests
////////////////////////////////////////////////////////////////////////////////

#[test]
fn test_invalid_json_handling() {
    let invalid_json = "{ invalid json }";
    let result: Result<Mt5LiveTickMsg, _> = serde_json::from_str(invalid_json);
    assert!(result.is_err());
}

#[test]
fn test_missing_fields_handling() {
    let incomplete_json = r#"{"status": "CONNECTED", "symbol": "BTCUSD"}"#;
    let result: Result<Mt5LiveTickMsg, _> = serde_json::from_str(incomplete_json);
    assert!(result.is_err());
}

#[test]
#[should_panic(expected = "index out of bounds")]
fn test_invalid_data_array_length() {
    // Tick message must have 3 elements in data array
    // This test verifies the parser panics on invalid data (rather than silently succeeding)
    let invalid_json = r#"{"status": "CONNECTED", "symbol": "BTCUSD", "timeframe": "TICK", "data": [123456789]}"#;
    let msg: Mt5LiveTickMsg = serde_json::from_str(invalid_json).expect("Deserialize failed");

    let instrument = create_test_instrument("BTCUSD", 2);
    // This should panic due to insufficient data array length (expected 3, got 1)
    let _result = parse_mt5_live_tick_to_quote(&msg, &instrument, UnixNanos::default());
}

////////////////////////////////////////////////////////////////////////////////
// Response Structure Tests
////////////////////////////////////////////////////////////////////////////////

#[test]
fn test_account_response_structure() {
    let json = load_fixture("http_get_account.json");
    let response: serde_json::Value = serde_json::from_str(&json).expect("Parse failed");

    // Verify all required fields exist
    assert!(response.get("error").is_some());
    assert!(response.get("balance").is_some());
    assert!(response.get("equity").is_some());
    assert!(response.get("margin").is_some());
    assert!(response.get("margin_free").is_some());
    assert!(response.get("margin_level").is_some());
    assert!(response.get("currency").is_some());
}

#[test]
fn test_positions_response_structure() {
    let json = load_fixture("http_get_positions.json");
    let response: serde_json::Value = serde_json::from_str(&json).expect("Parse failed");

    assert!(response.get("error").is_some());
    assert!(response.get("positions").is_some());
    assert!(response.get("server_time").is_some());

    let positions = response["positions"].as_array().expect("Not an array");
    if !positions.is_empty() {
        let pos = &positions[0];
        assert!(pos.get("id").is_some());
        assert!(pos.get("symbol").is_some());
        assert!(pos.get("type").is_some());
        assert!(pos.get("volume").is_some());
        assert!(pos.get("open").is_some());
    }
}

#[test]
fn test_empty_positions_response() {
    let json = load_fixture("http_get_positions_empty.json");
    let response: serde_json::Value = serde_json::from_str(&json).expect("Parse failed");

    assert_eq!(response["error"], false);
    let positions = response["positions"].as_array().expect("Not an array");
    assert_eq!(positions.len(), 0);
}
