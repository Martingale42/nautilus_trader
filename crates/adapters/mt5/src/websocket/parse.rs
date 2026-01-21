//! MT5 message parsers - convert MT5 format to Nautilus domain types.

use std::str::FromStr;

use nautilus_core::{uuid::UUID4, UnixNanos};
use nautilus_model::{
    data::{Bar, BarType, QuoteTick},
    enums::{OrderSide, OrderStatus, OrderType, TimeInForce},
    identifiers::{AccountId, ClientOrderId, InstrumentId, Symbol, Venue, VenueOrderId},
    instruments::{CurrencyPair, Instrument, InstrumentAny},
    reports::OrderStatusReport,
    types::{Currency, Price, Quantity},
};

use super::messages::*;
use crate::common::parse_timestamp_ms;

/// Parse MT5 live tick message to Nautilus QuoteTick
///
/// Parses the actual MT5-ZeroMQ live tick format: {"status": "CONNECTED", "symbol": "...", "timeframe": "TICK", "data": [timestamp_ms, bid, ask]}
///
/// # Arguments
/// * `msg` - MT5 live tick message
/// * `instrument` - Instrument definition
/// * `ts_init` - Timestamp when message was received
pub fn parse_mt5_live_tick_to_quote(
    msg: &Mt5LiveTickMsg,
    instrument: &InstrumentAny,
    ts_init: UnixNanos,
) -> anyhow::Result<QuoteTick> {
    let instrument_id = instrument.id();

    // Extract data: [timestamp_ms, bid, ask]
    let timestamp_ms = msg.data[0] as i64;
    let bid = msg.data[1];
    let ask = msg.data[2];

    let bid_price = Price::new(bid, instrument.price_precision());
    let ask_price = Price::new(ask, instrument.price_precision());

    // MT5 doesn't provide bid/ask sizes in tick data, use minimum size as default
    let default_size = instrument.size_increment();
    let bid_size = Quantity::new(default_size.as_f64(), instrument.size_precision());
    let ask_size = Quantity::new(default_size.as_f64(), instrument.size_precision());

    let ts_event = UnixNanos::from(parse_timestamp_ms(timestamp_ms));

    Ok(QuoteTick::new(
        instrument_id,
        bid_price,
        ask_price,
        bid_size,
        ask_size,
        ts_event,
        ts_init,
    ))
}

/// Parse MT5 live bar message to Nautilus Bar
///
/// Parses the actual MT5-ZeroMQ live bar format
///
/// # Arguments
/// * `msg` - MT5 live bar message
/// * `bar_type` - The bar type specification
/// * `instrument` - Instrument definition
/// * `ts_init` - Timestamp when message was received
pub fn parse_mt5_live_bar(
    msg: &Mt5LiveBarMsg,
    bar_type: &BarType,
    instrument: &InstrumentAny,
    ts_init: UnixNanos,
) -> anyhow::Result<Bar> {
    // Validate data array format [timestamp_sec, open, high, low, close, volume]
    if msg.data.len() < 6 {
        anyhow::bail!(
            "Invalid bar data array length: expected 6, got {}",
            msg.data.len()
        );
    }

    // Extract data: [timestamp_sec, open, high, low, close, volume]
    let timestamp_sec = msg.data[0] as i64;
    let open = msg.data[1];
    let high = msg.data[2];
    let low = msg.data[3];
    let close = msg.data[4];
    let volume = msg.data[5];

    let open_price = Price::new(open, instrument.price_precision());
    let high_price = Price::new(high, instrument.price_precision());
    let low_price = Price::new(low, instrument.price_precision());
    let close_price = Price::new(close, instrument.price_precision());
    let volume_qty = Quantity::new(volume, instrument.size_precision());

    // MT5 sends timestamp in SECONDS, not milliseconds
    let ts_event = UnixNanos::from((timestamp_sec * 1_000_000_000) as u64);

    Ok(Bar::new(
        *bar_type,
        open_price,
        high_price,
        low_price,
        close_price,
        volume_qty,
        ts_event,
        ts_init,
    ))
}

/// Parse MT5 order message to Nautilus OrderStatusReport
///
/// # Arguments
/// * `msg` - MT5 order message
/// * `instrument` - Instrument definition
/// * `account_id` - Account identifier
/// * `ts_init` - Timestamp when message was received
pub fn parse_mt5_order_status(
    msg: &Mt5OrderMsg,
    instrument: &InstrumentAny,
    account_id: AccountId,
    ts_init: UnixNanos,
) -> anyhow::Result<OrderStatusReport> {
    let instrument_id = instrument.id();

    // Parse venue order ID
    let venue_order_id = VenueOrderId::new(&msg.ticket.to_string());

    // Parse client order ID from comment if available
    let client_order_id = if !msg.comment.is_empty() {
        ClientOrderId::new(&msg.comment)
    } else {
        ClientOrderId::new(&format!("mt5-{}", msg.ticket))
    };

    // Map MT5 order type to Nautilus OrderType (MT5 sends as string)
    let type_str = msg.type_.as_str();
    let order_side = if type_str.contains("BUY") {
        OrderSide::Buy
    } else if type_str.contains("SELL") {
        OrderSide::Sell
    } else {
        OrderSide::NoOrderSide
    };

    let order_type = if type_str.contains("LIMIT") && type_str.contains("STOP") {
        OrderType::StopLimit
    } else if type_str.contains("STOP") {
        OrderType::StopMarket
    } else if type_str.contains("LIMIT") {
        OrderType::Limit
    } else {
        OrderType::Market
    };

    // Map MT5 order state to Nautilus OrderStatus (MT5 sends as string)
    let state_str = msg.state.as_str();
    let order_status = if state_str.contains("FILLED") || state_str.contains("PLACED") {
        OrderStatus::Filled
    } else if state_str.contains("CANCELED") {
        OrderStatus::Canceled
    } else if state_str.contains("REJECTED") {
        OrderStatus::Rejected
    } else if state_str.contains("EXPIRED") {
        OrderStatus::Expired
    } else if state_str.contains("PARTIAL") {
        OrderStatus::PartiallyFilled
    } else if state_str.contains("ACCEPTED") || state_str.contains("STARTED") {
        OrderStatus::Accepted
    } else {
        OrderStatus::Initialized
    };

    let price = Price::new(msg.price_open, instrument.price_precision());
    let quantity = Quantity::new(msg.volume_initial, instrument.size_precision());

    // Calculate filled quantity based on current volume
    let filled_qty = if msg.volume_current < msg.volume_initial {
        Quantity::new(
            msg.volume_initial - msg.volume_current,
            instrument.size_precision(),
        )
    } else {
        Quantity::zero(instrument.size_precision())
    };

    let ts_accepted = UnixNanos::from(parse_timestamp_ms(msg.time_setup));
    let ts_last = ts_init; // Use current time as last update

    // Default to GTC for time in force
    let time_in_force = TimeInForce::Gtc;

    let mut report = OrderStatusReport::new(
        account_id,
        instrument_id,
        None, // venue_client_order_id
        venue_order_id,
        order_side,
        order_type,
        time_in_force,
        order_status,
        quantity,
        filled_qty,
        ts_accepted,
        ts_last,
        ts_init,
        Some(UUID4::new()),
    );

    // Set price fields based on order type
    if msg.price_open > 0.0 {
        match order_type {
            OrderType::StopMarket => {
                // For stop market orders, price_open IS the trigger/activation price
                report = report.with_trigger_price(price);
            }
            OrderType::Limit => {
                report = report.with_price(price);
            }
            OrderType::StopLimit => {
                // price_open is the limit price for stop-limit orders
                report = report.with_price(price);
                // StopLimit orders need trigger_price from MT5's price_stoplimit field
                // which is not yet in Mt5OrderMsg struct
                todo!("Add price_stoplimit field to Mt5OrderMsg for StopLimit trigger_price");
            }
            _ => {
                // Market orders: price_open may be 0 or execution price
            }
        }
    }

    // Set client order ID if available
    if !msg.comment.is_empty() {
        report = report.with_client_order_id(client_order_id);
    }

    // Set average fill price when order has fills
    if filled_qty.as_f64() > 0.0 {
        // MT5 doesn't provide avg_px directly in order messages
        // Need to fetch from deal history
        todo!("Calculate avg_px from MT5 deal history for filled orders");
    }

    Ok(report)
}

/// Parse MT5 symbol info to Nautilus InstrumentAny
///
/// Converts MT5 SymbolInfo response into a proper Nautilus instrument
pub fn parse_mt5_symbol_info_to_instrument(
    symbol_info: &Mt5SymbolInfo,
    venue: &Venue,
    ts_event: UnixNanos,
    ts_init: UnixNanos,
) -> anyhow::Result<InstrumentAny> {
    // Parse numeric fields from strings
    let digits = symbol_info.digits.parse::<u8>()?;
    let tick_size = symbol_info.tick_size.parse::<f64>()?;
    let volume_step = symbol_info.volume_step.parse::<f64>()?;

    // Create instrument ID and symbol
    let raw_symbol = Symbol::new(&symbol_info.symbol);
    let instrument_id = InstrumentId::new(raw_symbol, *venue);

    // Parse currencies
    let base_currency = Currency::from_str(&symbol_info.base_currency)?;
    let quote_currency = Currency::from_str(&symbol_info.quote_currency)?;

    // Calculate size precision from volume_step
    let size_precision = if volume_step > 0.0 {
        let decimal_str = format!("{:.10}", volume_step);
        if let Some(dot_pos) = decimal_str.find('.') {
            let after_dot = &decimal_str[dot_pos + 1..];
            after_dot.trim_end_matches('0').len() as u8
        } else {
            0
        }
    } else {
        2 // default
    };

    // Create CurrencyPair instrument
    let instrument = CurrencyPair::new(
        instrument_id,
        raw_symbol, // raw_symbol
        base_currency,
        quote_currency,
        digits, // price_precision
        size_precision,
        Price::new(tick_size, digits),              // price_increment
        Quantity::new(volume_step, size_precision), // size_increment
        None,                                       // multiplier
        None,                                       // lot_size
        None,                                       // max_quantity
        None,                                       // min_quantity
        None,                                       // max_notional
        None,                                       // min_notional
        None,                                       // max_price
        None,                                       // min_price
        None,                                       // margin_init
        None,                                       // margin_maint
        None,                                       // maker_fee
        None,                                       // taker_fee
        ts_event,
        ts_init,
    );

    Ok(InstrumentAny::CurrencyPair(instrument))
}

////////////////////////////////////////////////////////////////////////////////
// Tests
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use nautilus_model::{
        identifiers::{Symbol, Venue},
        instruments::CurrencyPair,
        types::Currency,
    };

    use super::*;
    use crate::common::testing::load_test_json;

    // Live streaming format tests (Mt5LiveTickMsg, Mt5LiveBarMsg)

    #[test]
    fn test_parse_live_tick_from_fixture() {
        // Load real MT5 tick message from fixture
        let json = load_test_json("ws_tick_single.json");
        let msg: Mt5LiveTickMsg = serde_json::from_str(&json).unwrap();

        // Verify message structure
        assert_eq!(msg.status, "CONNECTED");
        assert_eq!(msg.symbol, "BTCUSD");
        assert_eq!(msg.timeframe, "TICK");
        assert_eq!(msg.data.len(), 3);

        // Create test instrument (BTCUSD with 2 decimal precision)
        let raw_symbol = Symbol::new("BTCUSD");
        let instrument_id = InstrumentId::new(raw_symbol, Venue::new("MT5"));
        let base_currency = Currency::BTC();
        let quote_currency = Currency::USD();

        let instrument = InstrumentAny::CurrencyPair(CurrencyPair::new(
            instrument_id,
            raw_symbol,
            base_currency,
            quote_currency,
            2,                      // price_precision
            2,                      // size_precision
            Price::new(0.01, 2),    // price_increment
            Quantity::new(0.01, 2), // size_increment
            None, None, None, None, None, None, None, None, None, None, None, None,
            UnixNanos::default(),
            UnixNanos::default(),
        ));

        // Parse to QuoteTick
        let tick = parse_mt5_live_tick_to_quote(&msg, &instrument, UnixNanos::default()).unwrap();

        // Verify parsed fields
        assert_eq!(tick.instrument_id, instrument_id);
        assert_eq!(tick.bid_price.as_f64(), 86835.0);
        assert_eq!(tick.ask_price.as_f64(), 86879.0);
        assert_eq!(tick.ts_event, 1764162271192000000); // ms -> ns conversion
    }

    #[test]
    fn test_parse_live_bar_from_fixture() {
        // Load real MT5 bar message from fixture
        let json = load_test_json("ws_bar_m1_single.json");
        let msg: Mt5LiveBarMsg = serde_json::from_str(&json).unwrap();

        // Verify message structure
        assert_eq!(msg.status, "CONNECTED");
        assert_eq!(msg.symbol, "XAUUSD.sml");
        assert_eq!(msg.timeframe, "M1");
        assert_eq!(msg.data.len(), 6); // timestamp, OHLCV

        // Create test instrument (XAUUSD with 2 decimal precision)
        let raw_symbol = Symbol::new("XAUUSD");
        let instrument_id = InstrumentId::new(raw_symbol, Venue::new("MT5"));
        let base_currency = Currency::XAU();
        let quote_currency = Currency::USD();

        let instrument = InstrumentAny::CurrencyPair(CurrencyPair::new(
            instrument_id,
            raw_symbol,
            base_currency,
            quote_currency,
            2,                      // price_precision
            2,                      // size_precision
            Price::new(0.01, 2),    // price_increment
            Quantity::new(0.01, 2), // size_increment
            None, None, None, None, None, None, None, None, None, None, None, None,
            UnixNanos::default(),
            UnixNanos::default(),
        ));

        // Parse to Bar
        let bar_type = BarType::from_str("XAUUSD.MT5-1-MINUTE-LAST-EXTERNAL").unwrap();
        let bar = parse_mt5_live_bar(&msg, &bar_type, &instrument, UnixNanos::default()).unwrap();

        // Verify parsed fields (rounded to 2 decimal places due to price_precision)
        assert_eq!(bar.open.as_f64(), 4160.12);
        assert_eq!(bar.high.as_f64(), 4160.18); // 4160.175 rounds to 4160.18 with precision=2
        assert_eq!(bar.low.as_f64(), 4159.72);  // 4159.715 rounds to 4159.72 with precision=2
        assert_eq!(bar.close.as_f64(), 4160.18); // 4160.175 rounds to 4160.18 with precision=2
        assert_eq!(bar.volume.as_f64(), 105.0);
        assert_eq!(bar.ts_event, 1764162180000000000); // seconds -> ns
    }

    #[test]
    fn test_parse_multiple_ticks_from_fixture() {
        // Load fixture with multiple ticks
        let json = load_test_json("ws_tick.json");
        let messages: Vec<Mt5LiveTickMsg> = serde_json::from_str(&json).unwrap();

        assert!(!messages.is_empty());
        assert!(messages.len() > 10); // Should have ~17 ticks

        // Create test instrument
        let raw_symbol = Symbol::new("BTCUSD");
        let instrument_id = InstrumentId::new(raw_symbol, Venue::new("MT5"));
        let base_currency = Currency::BTC();
        let quote_currency = Currency::USD();

        let instrument = InstrumentAny::CurrencyPair(CurrencyPair::new(
            instrument_id,
            raw_symbol,
            base_currency,
            quote_currency,
            2, 2,
            Price::new(0.01, 2),
            Quantity::new(0.01, 2),
            None, None, None, None, None, None, None, None, None, None, None, None,
            UnixNanos::default(),
            UnixNanos::default(),
        ));

        // Verify all messages parse correctly
        for msg in messages.iter() {
            let tick = parse_mt5_live_tick_to_quote(msg, &instrument, UnixNanos::default()).unwrap();
            assert_eq!(tick.instrument_id, instrument_id);
            assert!(tick.bid_price.as_f64() > 0.0);
            assert!(tick.ask_price.as_f64() > 0.0);
            assert!(tick.ask_price.as_f64() >= tick.bid_price.as_f64()); // Spread check
        }
    }

    #[test]
    fn test_parse_instruments_from_fixture() {
        // Load real MT5 instruments response
        let json = load_test_json("http_get_instruments_minimal.json");
        let response: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(response["error"], false);

        let symbols = response["symbols"].as_array().unwrap();
        assert_eq!(symbols.len(), 2); // EURUSD, BTCUSD

        // Verify EURUSD structure
        let eurusd = &symbols[0];
        assert_eq!(eurusd["symbol"], "EURUSD");
        assert_eq!(eurusd["base_currency"], "EUR");
        assert_eq!(eurusd["quote_currency"], "USD");
        assert_eq!(eurusd["digits"], "5");

        // Verify BTCUSD structure
        let btcusd = &symbols[1];
        assert_eq!(btcusd["symbol"], "BTCUSD");
        assert_eq!(btcusd["base_currency"], "BTC");
        assert_eq!(btcusd["quote_currency"], "USD");
        assert_eq!(btcusd["digits"], "2");
    }

    #[test]
    fn test_parse_account_from_fixture() {
        // Load real MT5 account response
        let json = load_test_json("http_get_account.json");
        let account: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(account["error"], false);
        assert!(account["balance"].is_number());
        assert!(account["equity"].is_number());
        assert!(account["margin"].is_number());
        assert!(account["margin_free"].is_number());
        assert!(account["margin_level"].is_number());
        assert_eq!(account["currency"], "USD");
        assert_eq!(account["trading_allowed"], 1);
    }

    #[test]
    fn test_parse_positions_from_fixture() {
        // Load real MT5 positions response
        let json = load_test_json("http_get_positions.json");
        let response: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(response["error"], false);
        assert!(response["server_time"].is_number());

        let positions = response["positions"].as_array().unwrap();
        assert!(!positions.is_empty());

        // Verify first position structure
        let pos = &positions[0];
        assert!(pos["id"].is_number());
        assert_eq!(pos["symbol"], "BTCUSD");
        assert!(pos["type"].is_string());
        assert!(pos["volume"].is_number());
        assert!(pos["open"].is_number());
        assert!(pos["time_setup"].is_number());
    }

    #[test]
    fn test_parse_positions_empty_from_fixture() {
        // Test empty positions response
        let json = load_test_json("http_get_positions_empty.json");
        let response: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(response["error"], false);
        let positions = response["positions"].as_array().unwrap();
        assert_eq!(positions.len(), 0);
    }

    #[test]
    fn test_parse_error_response_from_fixture() {
        // Test error response handling
        let json = load_test_json("http_error_response.json");
        let error: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(error["error"], true);
        assert_eq!(error["lastError"], "65538");
        assert_eq!(error["description"], "ERR_WRONG_ACTION");
        assert_eq!(error["function"], "RequestHandler");
    }
}
