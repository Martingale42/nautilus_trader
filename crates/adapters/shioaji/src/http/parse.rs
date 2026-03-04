use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Bar, BarType, QuoteTick, TradeTick},
    enums::AggressorSide,
    identifiers::{InstrumentId, Symbol, TradeId},
    instruments::{Equity, InstrumentAny},
    types::{Currency, Price, Quantity},
};

use super::models::{KBarsResponse, SnapshotData, StockContract, TicksResponse};
use crate::common::instrument::{SIZE_PRECISION, STOCK_LOT_SIZE};
use crate::common::parse::parse_instrument_id;
use crate::common::tick_size::twse_stock_tick_size;

/// Parse a `SnapshotData` into a `QuoteTick` (top-of-book bid/ask).
pub fn parse_snapshot_to_quote_tick(
    snapshot: &SnapshotData,
    instrument_id: InstrumentId,
    price_precision: u8,
    size_precision: u8,
    ts_init: UnixNanos,
) -> anyhow::Result<QuoteTick> {
    QuoteTick::new_checked(
        instrument_id,
        Price::new(snapshot.buy_price, price_precision),
        Price::new(snapshot.sell_price, price_precision),
        Quantity::new(snapshot.buy_volume, size_precision),
        Quantity::new(snapshot.sell_volume, size_precision),
        UnixNanos::from(snapshot.ts),
        ts_init,
    )
}

/// Parse a gateway `TicksResponse` into `Vec<TradeTick>`.
///
/// Iterates parallel arrays: ts, close, volume, tick_type.
/// `tick_type`: 1 = Buy (aggressor = buyer), 2 = Sell (aggressor = seller), 0 = unknown.
pub fn parse_ticks_response(
    ticks: &TicksResponse,
    instrument_id: InstrumentId,
    price_precision: u8,
    size_precision: u8,
    ts_init: UnixNanos,
) -> anyhow::Result<Vec<TradeTick>> {
    let len = ticks.ts.len();
    let mut result = Vec::with_capacity(len);

    for i in 0..len {
        let aggressor_side = match ticks.tick_type[i] {
            1 => AggressorSide::Buyer,
            2 => AggressorSide::Seller,
            _ => AggressorSide::NoAggressor,
        };

        let trade = TradeTick::new_checked(
            instrument_id,
            Price::new(ticks.close[i], price_precision),
            Quantity::new(ticks.volume[i] as f64, size_precision),
            aggressor_side,
            TradeId::new(format!("{}-{}", ticks.code, ticks.ts[i])),
            UnixNanos::from(ticks.ts[i]),
            ts_init,
        )?;
        result.push(trade);
    }

    Ok(result)
}

/// Parse a gateway `KBarsResponse` into `Vec<Bar>`.
///
/// Iterates parallel arrays: ts, open, high, low, close, volume.
pub fn parse_kbars_response(
    kbars: &KBarsResponse,
    bar_type: BarType,
    price_precision: u8,
    size_precision: u8,
    ts_init: UnixNanos,
) -> Vec<Bar> {
    let len = kbars.ts.len();
    let mut result = Vec::with_capacity(len);

    for i in 0..len {
        let bar = Bar::new(
            bar_type,
            Price::new(kbars.open[i], price_precision),
            Price::new(kbars.high[i], price_precision),
            Price::new(kbars.low[i], price_precision),
            Price::new(kbars.close[i], price_precision),
            Quantity::new(kbars.volume[i] as f64, size_precision),
            UnixNanos::from(kbars.ts[i]),
            ts_init,
        );
        result.push(bar);
    }

    result
}

/// Parse a gateway `StockContract` into a Nautilus `Equity` instrument.
///
/// - `InstrumentId` = `{code}.SINOPAC`
/// - Tick size and precision derived from reference price via TWSE schedule
/// - Lot size = 1000 shares (standard Taiwan market lot)
pub fn parse_stock_to_equity(
    contract: &StockContract,
    ts_event: UnixNanos,
    ts_init: UnixNanos,
) -> anyhow::Result<InstrumentAny> {
    let instrument_id = parse_instrument_id(&contract.code)?;
    let raw_symbol = Symbol::new(&contract.code);
    let currency = Currency::TWD();

    let (tick_size, price_precision) = twse_stock_tick_size(contract.reference);
    let price_increment = Price::new(tick_size, price_precision);
    let lot_size = Some(Quantity::new(STOCK_LOT_SIZE, SIZE_PRECISION));

    let max_price = Some(Price::new(contract.limit_up, price_precision));
    let min_price = Some(Price::new(contract.limit_down, price_precision));

    let equity = Equity::new(
        instrument_id,
        raw_symbol,
        None, // isin
        currency,
        price_precision,
        price_increment,
        lot_size,
        None, // max_quantity
        None, // min_quantity
        max_price,
        min_price,
        None, // margin_init
        None, // margin_maint
        None, // maker_fee
        None, // taker_fee
        None, // info
        ts_event,
        ts_init,
    );

    Ok(InstrumentAny::Equity(equity))
}

#[cfg(test)]
mod tests {
    use nautilus_model::data::BarSpecification;
    use nautilus_model::enums::{AggregationSource, BarAggregation, PriceType};
    use nautilus_model::identifiers::Symbol;
    use nautilus_model::identifiers::Venue;
    use nautilus_model::instruments::Instrument;

    use super::*;
    use crate::common::testing::load_test_json_as;

    fn test_instrument_id() -> InstrumentId {
        InstrumentId::new(Symbol::new("2330"), Venue::new("SINOPAC"))
    }

    #[test]
    fn test_parse_snapshot_to_quote_tick() {
        let snapshots: Vec<SnapshotData> = load_test_json_as("market_snapshots.json");
        let quote = parse_snapshot_to_quote_tick(
            &snapshots[0],
            test_instrument_id(),
            1,
            0,
            UnixNanos::default(),
        )
        .unwrap();

        assert_eq!(quote.instrument_id, test_instrument_id());
        assert_eq!(quote.bid_price, Price::new(580.0, 1));
        assert_eq!(quote.ask_price, Price::new(581.0, 1));
    }

    #[test]
    fn test_parse_ticks_response() {
        let ticks: TicksResponse = load_test_json_as("market_ticks.json");
        let trades = parse_ticks_response(
            &ticks,
            test_instrument_id(),
            1,
            0,
            UnixNanos::default(),
        )
        .unwrap();

        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].price, Price::new(580.0, 1));
        assert_eq!(trades[0].aggressor_side, AggressorSide::Buyer);
        assert_eq!(trades[1].price, Price::new(581.0, 1));
        assert_eq!(trades[1].aggressor_side, AggressorSide::Seller);
    }

    #[test]
    fn test_parse_kbars_response() {
        let kbars: KBarsResponse = load_test_json_as("market_kbars.json");
        let bar_type = BarType::new(
            test_instrument_id(),
            BarSpecification::new(1, BarAggregation::Minute, PriceType::Last),
            AggregationSource::External,
        );
        let bars =
            parse_kbars_response(&kbars, bar_type, 1, 0, UnixNanos::default());

        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].open, Price::new(578.0, 1));
        assert_eq!(bars[0].high, Price::new(582.0, 1));
        assert_eq!(bars[0].close, Price::new(580.0, 1));
        assert_eq!(bars[1].open, Price::new(580.0, 1));
    }

    #[test]
    fn test_parse_stock_to_equity_tsmc() {
        let contracts: Vec<StockContract> = load_test_json_as("contracts_stocks.json");
        let equity = parse_stock_to_equity(
            &contracts[0], // 2330 TSMC, reference=580.0
            UnixNanos::default(),
            UnixNanos::default(),
        )
        .unwrap();

        match equity {
            InstrumentAny::Equity(e) => {
                assert_eq!(e.id().to_string(), "2330.SINOPAC");
                assert_eq!(e.price_precision(), 1); // 580 TWD -> tick=1.0 -> precision 1
                assert_eq!(e.quote_currency().code.as_str(), "TWD");
                assert!(e.lot_size().is_some());
                assert_eq!(e.lot_size().unwrap().as_f64(), 1000.0);
                assert!(e.max_price().is_some());
            }
            _ => panic!("Expected Equity"),
        }
    }
}
