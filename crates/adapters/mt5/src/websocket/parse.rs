//! MT5 message parsers - convert MT5 format to Nautilus domain types.

use super::messages::*;
use crate::common::{parse_timestamp_ms, Mt5TickFlags};
use nautilus_model::{
    data::{QuoteTick, TradeTick},
    enums::{AggressorSide, OrderSide, OrderStatus, OrderType, TimeInForce},
    identifiers::{AccountId, ClientOrderId, TradeId, VenueOrderId},
    instruments::{Instrument, InstrumentAny},
    reports::OrderStatusReport,
    types::{Price, Quantity},
};
use nautilus_core::{uuid::UUID4, UnixNanos};

/// Parse MT5 tick message to Nautilus TradeTick
///
/// # Arguments
/// * `msg` - MT5 tick message
/// * `instrument` - Instrument definition
/// * `ts_init` - Timestamp when message was received
pub fn parse_mt5_tick_to_trade(
    msg: &Mt5TickMsg,
    instrument: &InstrumentAny,
    ts_init: UnixNanos,
) -> anyhow::Result<TradeTick> {
    let instrument_id = instrument.id();

    // Determine aggressor side from flags
    let flags = Mt5TickFlags(msg.flags);
    let aggressor_side = if flags.is_buy() {
        AggressorSide::Buyer
    } else if flags.is_sell() {
        AggressorSide::Seller
    } else {
        AggressorSide::NoAggressor
    };

    // Use last price if available, otherwise mid price
    let price_value = if msg.last > 0.0 {
        msg.last
    } else {
        (msg.bid + msg.ask) / 2.0
    };

    let price = Price::new(price_value, instrument.price_precision());
    let size = Quantity::new(msg.volume, instrument.size_precision());

    // Generate unique trade ID
    let trade_id = TradeId::new(&format!("mt5-{}-{}", msg.symbol, msg.time));

    // Convert timestamp from milliseconds to nanoseconds
    let ts_event = UnixNanos::from(parse_timestamp_ms(msg.time));

    Ok(TradeTick::new(
        instrument_id,
        price,
        size,
        aggressor_side,
        trade_id,
        ts_event,
        ts_init,
    ))
}

/// Parse MT5 tick message to Nautilus QuoteTick
///
/// QuoteTicks are better for forex/CFD where bid/ask is more relevant than trades
pub fn parse_mt5_tick_to_quote(
    msg: &Mt5TickMsg,
    instrument: &InstrumentAny,
    ts_init: UnixNanos,
) -> anyhow::Result<QuoteTick> {
    let instrument_id = instrument.id();

    let bid_price = Price::new(msg.bid, instrument.price_precision());
    let ask_price = Price::new(msg.ask, instrument.price_precision());

    // MT5 doesn't provide bid/ask sizes directly, use volume or default
    let bid_size = Quantity::new(msg.volume, instrument.size_precision());
    let ask_size = Quantity::new(msg.volume, instrument.size_precision());

    let ts_event = UnixNanos::from(parse_timestamp_ms(msg.time));

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

    // Map MT5 order type to Nautilus OrderType
    let order_side = match msg.type_order {
        0 | 2 | 4 | 6 => OrderSide::Buy,  // BUY, BUY_LIMIT, BUY_STOP, BUY_STOP_LIMIT
        1 | 3 | 5 | 7 => OrderSide::Sell, // SELL, SELL_LIMIT, SELL_STOP, SELL_STOP_LIMIT
        _ => OrderSide::NoOrderSide,
    };

    let order_type = match msg.type_order {
        0 | 1 => OrderType::Market,         // Market orders
        2 | 3 => OrderType::Limit,          // Limit orders
        4 | 5 => OrderType::StopMarket,     // Stop orders
        6 | 7 => OrderType::StopLimit,      // Stop limit orders
        _ => OrderType::Market,             // Default
    };

    // Map MT5 order state to Nautilus OrderStatus
    let order_status = match msg.state {
        0 => OrderStatus::Initialized, // Started
        1 => OrderStatus::Accepted,     // Placed
        2 => OrderStatus::Canceled,     // Canceled
        3 => OrderStatus::PartiallyFilled, // Partial
        4 => OrderStatus::Filled,       // Filled
        5 => OrderStatus::Rejected,     // Rejected
        6 => OrderStatus::Expired,      // Expired
        _ => OrderStatus::Initialized,
    };

    let price = Price::new(msg.price_open, instrument.price_precision());
    let quantity = Quantity::new(msg.volume, instrument.size_precision());

    // For now, assume filled_qty is 0 for pending orders and volume for filled orders
    let filled_qty = if msg.state == 4 {
        quantity
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

    // Set client order ID if available
    if !msg.comment.is_empty() {
        report = report.with_client_order_id(client_order_id);
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        identifiers::{Symbol, Venue},
        instruments::CurrencyPair,
        types::Currency,
    };

    fn create_test_instrument() -> InstrumentAny {
        let instrument_id = InstrumentId::new(Symbol::new("EURUSD"), Venue::new("MT5"));
        let base_currency = Currency::USD();
        let quote_currency = Currency::USD();

        InstrumentAny::CurrencyPair(
            CurrencyPair::new(
                instrument_id,
                base_currency,
                quote_currency,
                5,              // price_precision
                2,              // size_precision
                Price::new(0.00001, 5), // price_increment
                Quantity::new(0.01, 2), // size_increment
                None, // lot_size
                None, // max_quantity
                None, // min_quantity
                None, // max_notional
                None, // min_notional
                None, // max_price
                None, // min_price
                None, // margin_init
                None, // margin_maint
                None, // maker_fee
                None, // taker_fee
                0,    // ts_event
                0,    // ts_init
            )
        )
    }

    #[test]
    fn test_parse_tick_to_trade() {
        let msg = Mt5TickMsg {
            symbol: "EURUSD".into(),
            bid: 1.08500,
            ask: 1.08510,
            last: 1.08505,
            volume: 100.0,
            time: 1709891679000,
            flags: Mt5TickFlags::BUY,
        };

        let instrument = create_test_instrument();
        let trade = parse_mt5_tick_to_trade(&msg, &instrument, 0).unwrap();

        assert_eq!(trade.price.as_f64(), 1.08505);
        assert_eq!(trade.size.as_f64(), 100.0);
        assert_eq!(trade.aggressor_side, AggressorSide::Buyer);
    }

    #[test]
    fn test_parse_tick_to_quote() {
        let msg = Mt5TickMsg {
            symbol: "EURUSD".into(),
            bid: 1.08500,
            ask: 1.08510,
            last: 0.0,
            volume: 100.0,
            time: 1709891679000,
            flags: 0,
        };

        let instrument = create_test_instrument();
        let quote = parse_mt5_tick_to_quote(&msg, &instrument, 0).unwrap();

        assert_eq!(quote.bid_price.as_f64(), 1.08500);
        assert_eq!(quote.ask_price.as_f64(), 1.08510);
    }
}
