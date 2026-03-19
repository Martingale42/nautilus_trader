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
//! Parsers for Sinopac WebSocket market data messages.

use chrono::NaiveDateTime;
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{QuoteTick, TradeTick},
    enums::AggressorSide,
    identifiers::{InstrumentId, TradeId},
    types::{Price, Quantity},
};

use super::messages::{WsBidAskMsg, WsTickMsg};

/// Parse a Taiwan local-time timestamp string to `UnixNanos`.
///
/// Format: "YYYY-MM-DD HH:MM:SS.ffffff" (UTC+8)
/// The fractional seconds part is optional.
pub fn parse_taiwan_timestamp(ts: &str) -> anyhow::Result<UnixNanos> {
    let dt = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S"))?;
    // Taiwan is UTC+8
    let utc = dt - chrono::TimeDelta::hours(8);
    let nanos = utc
        .and_utc()
        .timestamp_nanos_opt()
        .ok_or_else(|| anyhow::anyhow!("Timestamp overflow: {ts}"))?;
    Ok(UnixNanos::from(nanos as u64))
}

/// Parse a WS tick message into a `TradeTick`.
///
/// `tick_type`: 1 = Buy (aggressor = buyer), 2 = Sell (aggressor = seller).
/// For futures/options where `tick_type` is absent, defaults to `NoAggressor`.
pub fn parse_ws_tick_to_trade_tick(
    msg: &WsTickMsg,
    instrument_id: InstrumentId,
    price_precision: u8,
    size_precision: u8,
    ts_event: UnixNanos,
    ts_init: UnixNanos,
) -> anyhow::Result<TradeTick> {
    let aggressor_side = match msg.data.tick_type {
        Some(1) => AggressorSide::Buyer,
        Some(2) => AggressorSide::Seller,
        _ => AggressorSide::NoAggressor,
    };

    TradeTick::new_checked(
        instrument_id,
        Price::new(msg.data.close, price_precision),
        Quantity::new(msg.data.volume as f64, size_precision),
        aggressor_side,
        TradeId::new(format!("{}-{}", msg.code, msg.data.timestamp)),
        ts_event,
        ts_init,
    )
}

/// Parse a WS bidask message into a `QuoteTick` (top of book).
///
/// Uses `bid_price[0]`/`bid_volume[0]` and `ask_price[0]`/`ask_volume[0]`.
pub fn parse_ws_bidask_to_quote_tick(
    msg: &WsBidAskMsg,
    instrument_id: InstrumentId,
    price_precision: u8,
    size_precision: u8,
    ts_event: UnixNanos,
    ts_init: UnixNanos,
) -> anyhow::Result<QuoteTick> {
    if msg.data.bid_price.is_empty() || msg.data.ask_price.is_empty() {
        anyhow::bail!("Empty bid/ask price arrays for {}", msg.code);
    }

    QuoteTick::new_checked(
        instrument_id,
        Price::new(msg.data.bid_price[0], price_precision),
        Price::new(msg.data.ask_price[0], price_precision),
        Quantity::new(msg.data.bid_volume[0] as f64, size_precision),
        Quantity::new(msg.data.ask_volume[0] as f64, size_precision),
        ts_event,
        ts_init,
    )
}

#[cfg(test)]
mod tests {
    use nautilus_model::identifiers::{Symbol, Venue};
    use rstest::rstest;

    use super::*;
    use crate::common::testing::load_test_json_as;
    use crate::websocket::messages::WsIncomingMsg;

    #[rstest]
    fn test_parse_taiwan_timestamp_with_microseconds() {
        let ts = parse_taiwan_timestamp("2026-03-02 09:30:00.123456").unwrap();
        assert!(ts.as_u64() > 0);
    }

    #[rstest]
    fn test_parse_taiwan_timestamp_without_fractional() {
        let ts = parse_taiwan_timestamp("2026-03-02 09:30:00").unwrap();
        assert!(ts.as_u64() > 0);
    }

    #[rstest]
    fn test_parse_taiwan_timestamp_utc_offset() {
        // 2026-03-02 00:00:00 Taiwan = 2026-03-01 16:00:00 UTC
        let ts = parse_taiwan_timestamp("2026-03-02 00:00:00").unwrap();
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 3, 1)
            .unwrap()
            .and_hms_opt(16, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_nanos_opt()
            .unwrap() as u64;
        assert_eq!(ts.as_u64(), expected);
    }

    fn test_instrument_id() -> InstrumentId {
        InstrumentId::new(Symbol::new("2330"), Venue::new("SINOPAC"))
    }

    #[rstest]
    fn test_parse_ws_tick_buy_aggressor() {
        let msg: WsIncomingMsg = load_test_json_as("ws_tick_stock.json");
        if let WsIncomingMsg::Tick(tick) = msg {
            let trade = parse_ws_tick_to_trade_tick(
                &tick,
                test_instrument_id(),
                1,
                0,
                UnixNanos::from(1_740_900_000_000_000_000u64),
                UnixNanos::default(),
            )
            .unwrap();

            assert_eq!(trade.instrument_id, test_instrument_id());
            assert_eq!(trade.price, Price::new(580.0, 1));
            assert_eq!(trade.aggressor_side, AggressorSide::Buyer);
        } else {
            panic!("Expected Tick message");
        }
    }

    #[rstest]
    fn test_parse_ws_tick_futures_no_aggressor() {
        let msg: WsIncomingMsg = load_test_json_as("ws_tick_futures.json");
        if let WsIncomingMsg::Tick(tick) = msg {
            let instrument_id =
                InstrumentId::new(Symbol::new("TXFC6"), Venue::new("SINOPAC"));
            let trade = parse_ws_tick_to_trade_tick(
                &tick,
                instrument_id,
                0,
                0,
                UnixNanos::from(1_740_900_000_000_000_000u64),
                UnixNanos::default(),
            )
            .unwrap();

            assert_eq!(trade.aggressor_side, AggressorSide::NoAggressor);
            assert_eq!(trade.price, Price::new(20050.0, 0));
        } else {
            panic!("Expected Tick message");
        }
    }

    #[rstest]
    fn test_parse_ws_bidask_top_of_book() {
        let msg: WsIncomingMsg = load_test_json_as("ws_bidask.json");
        if let WsIncomingMsg::BidAsk(ba) = msg {
            let quote = parse_ws_bidask_to_quote_tick(
                &ba,
                test_instrument_id(),
                1,
                0,
                UnixNanos::from(1_740_900_000_000_000_000u64),
                UnixNanos::default(),
            )
            .unwrap();

            assert_eq!(quote.instrument_id, test_instrument_id());
            assert_eq!(quote.bid_price, Price::new(580.0, 1));
            assert_eq!(quote.ask_price, Price::new(581.0, 1));
            assert_eq!(quote.bid_size, Quantity::new(120.0, 0));
            assert_eq!(quote.ask_size, Quantity::new(85.0, 0));
        } else {
            panic!("Expected BidAsk message");
        }
    }
}
