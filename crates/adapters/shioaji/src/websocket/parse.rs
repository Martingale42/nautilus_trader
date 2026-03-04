use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{QuoteTick, TradeTick},
    enums::AggressorSide,
    identifiers::{InstrumentId, TradeId},
    types::{Price, Quantity},
};

use super::messages::{WsBidAskMsg, WsTickMsg};

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

    use super::*;
    use crate::common::testing::load_test_json_as;
    use crate::websocket::messages::WsIncomingMsg;

    fn test_instrument_id() -> InstrumentId {
        InstrumentId::new(Symbol::new("2330"), Venue::new("SINOPAC"))
    }

    #[test]
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

    #[test]
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

    #[test]
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
