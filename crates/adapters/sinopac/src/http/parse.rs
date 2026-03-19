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
//! Parsers that convert Sinopac REST responses to Nautilus domain types.

use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Bar, BarType, QuoteTick, TradeTick},
    enums::{AggressorSide, AssetClass, OptionKind},
    identifiers::{InstrumentId, Symbol, TradeId},
    instruments::{
        Equity, FuturesContract as NautilusFuturesContract, InstrumentAny,
        OptionContract as NautilusOptionContract,
    },
    types::{Currency, Price, Quantity},
};
use ustr::Ustr;

use super::models::{
    FuturesContract, KBarsResponse, OptionsContract, SnapshotData, StockContract, TicksResponse,
};
use crate::common::instrument::{
    futures_multiplier, options_multiplier, CONTRACT_LOT_SIZE, SIZE_PRECISION, STOCK_LOT_SIZE,
};
use crate::common::parse::parse_instrument_id;
use crate::common::tick_size::{futures_tick_size, options_tick_size, twse_stock_tick_size};

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

/// Parse a gateway `FuturesContract` into a Nautilus `FuturesContract` instrument.
///
/// - Root symbol and multiplier derived from `category` field
/// - Tick size from `futures_tick_size()` lookup
/// - Expiration parsed from `delivery_date`
pub fn parse_futures_to_contract(
    contract: &FuturesContract,
    ts_event: UnixNanos,
    ts_init: UnixNanos,
) -> anyhow::Result<InstrumentAny> {
    let instrument_id = parse_instrument_id(&contract.code)?;
    let raw_symbol = Symbol::new(&contract.code);
    let currency = Currency::TWD();

    let root_symbol = &contract.category;
    let (tick_size, price_precision) = futures_tick_size(root_symbol);
    let price_increment = Price::new(tick_size, price_precision);
    let multiplier = Quantity::new(futures_multiplier(root_symbol), 0);
    let lot_size = Quantity::new(CONTRACT_LOT_SIZE, SIZE_PRECISION);

    let expiration_ns = parse_date_to_nanos(&contract.delivery_date)?;
    let activation_ns = parse_date_to_nanos(&contract.update_date).unwrap_or(ts_event);

    let asset_class = match contract.underlying_kind.as_str() {
        "I" => AssetClass::Index,
        _ => AssetClass::Equity,
    };

    let max_price = Some(Price::new(contract.limit_up, price_precision));
    let min_price = Some(Price::new(contract.limit_down, price_precision));

    let futures = NautilusFuturesContract::new(
        instrument_id,
        raw_symbol,
        asset_class,
        Some(Ustr::from("TAIFEX")),
        Ustr::from(root_symbol),
        activation_ns,
        expiration_ns,
        currency,
        price_precision,
        price_increment,
        multiplier,
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

    Ok(InstrumentAny::FuturesContract(futures))
}

/// Parse a gateway `OptionsContract` into a Nautilus `OptionContract` instrument.
///
/// - Root symbol and multiplier derived from `category` field
/// - Tick size from `options_tick_size()` based on reference premium
/// - Option kind parsed from `option_right` ("Call" / "Put")
pub fn parse_options_to_contract(
    contract: &OptionsContract,
    ts_event: UnixNanos,
    ts_init: UnixNanos,
) -> anyhow::Result<InstrumentAny> {
    let instrument_id = parse_instrument_id(&contract.code)?;
    let raw_symbol = Symbol::new(&contract.code);
    let currency = Currency::TWD();

    let root_symbol = &contract.category;
    let (tick_size, price_precision) = options_tick_size(contract.reference);
    let price_increment = Price::new(tick_size, price_precision);
    let multiplier = Quantity::new(options_multiplier(root_symbol), 0);
    let lot_size = Quantity::new(CONTRACT_LOT_SIZE, SIZE_PRECISION);

    let option_kind = match contract.option_right.as_str() {
        "Call" => OptionKind::Call,
        "Put" => OptionKind::Put,
        other => anyhow::bail!("Unknown option_right: {other}"),
    };

    let strike_price = Price::new(contract.strike_price, 0);

    let expiration_ns = parse_date_to_nanos(&contract.delivery_date)?;
    let activation_ns = parse_date_to_nanos(&contract.update_date).unwrap_or(ts_event);

    let asset_class = match contract.underlying_kind.as_str() {
        "I" => AssetClass::Index,
        _ => AssetClass::Equity,
    };

    let max_price = Some(Price::new(contract.limit_up, price_precision));
    let min_price = Some(Price::new(contract.limit_down, price_precision));

    let option = NautilusOptionContract::new(
        instrument_id,
        raw_symbol,
        asset_class,
        Some(Ustr::from("TAIFEX")),
        Ustr::from(root_symbol),
        option_kind,
        strike_price,
        currency,
        activation_ns,
        expiration_ns,
        price_precision,
        price_increment,
        multiplier,
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

    Ok(InstrumentAny::OptionContract(option))
}

/// Parse a date string like "2026/06/17" or "2026-06-17" to `UnixNanos`.
///
/// Treats the date as midnight in Taiwan time (UTC+8).
fn parse_date_to_nanos(date_str: &str) -> anyhow::Result<UnixNanos> {
    let normalized = date_str.replace('/', "-");
    let date = chrono::NaiveDate::parse_from_str(&normalized, "%Y-%m-%d")?;
    let datetime = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid date: {date_str}"))?;
    // Taiwan is UTC+8, so midnight local = 16:00 previous day UTC
    let utc = datetime - chrono::TimeDelta::hours(8);
    let timestamp_ns = utc
        .and_utc()
        .timestamp_nanos_opt()
        .ok_or_else(|| anyhow::anyhow!("Timestamp overflow for {date_str}"))?;
    Ok(UnixNanos::from(timestamp_ns as u64))
}

#[cfg(test)]
mod tests {
    use nautilus_model::data::BarSpecification;
    use nautilus_model::enums::{AggregationSource, BarAggregation, PriceType};
    use nautilus_model::identifiers::Symbol;
    use nautilus_model::identifiers::Venue;
    use nautilus_model::instruments::Instrument;
    use rstest::rstest;

    use super::*;
    use crate::common::testing::load_test_json_as;
    use crate::http::models::{FuturesContract, OptionsContract};

    fn test_instrument_id() -> InstrumentId {
        InstrumentId::new(Symbol::new("2330"), Venue::new("SINOPAC"))
    }

    #[rstest]
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

    #[rstest]
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

    #[rstest]
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

    #[rstest]
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

    #[rstest]
    fn test_parse_futures_to_contract_txf() {
        let contracts: Vec<FuturesContract> = load_test_json_as("contracts_futures.json");
        let instrument = parse_futures_to_contract(
            &contracts[0], // TXFC6
            UnixNanos::default(),
            UnixNanos::default(),
        )
        .unwrap();

        match instrument {
            InstrumentAny::FuturesContract(f) => {
                assert_eq!(f.id().to_string(), "TXFC6.SINOPAC");
                assert_eq!(f.underlying().unwrap().as_str(), "TXF");
                assert_eq!(f.multiplier().as_f64(), 200.0);
                assert_eq!(f.price_precision(), 0); // TXF tick=1.0
                assert_eq!(f.lot_size().unwrap().as_f64(), 1.0);
                assert_eq!(f.quote_currency().code.as_str(), "TWD");
            }
            _ => panic!("Expected FuturesContract"),
        }
    }

    #[rstest]
    fn test_parse_options_to_contract_call() {
        let contracts: Vec<OptionsContract> = load_test_json_as("contracts_options.json");
        let instrument = parse_options_to_contract(
            &contracts[0], // TXO20000C6, strike=20000, Call
            UnixNanos::default(),
            UnixNanos::default(),
        )
        .unwrap();

        match instrument {
            InstrumentAny::OptionContract(o) => {
                assert_eq!(o.id().to_string(), "TXO20000C6.SINOPAC");
                assert_eq!(o.option_kind(), Some(OptionKind::Call));
                assert_eq!(o.strike_price().unwrap().as_f64(), 20000.0);
                assert_eq!(o.underlying().unwrap().as_str(), "TXO");
                assert_eq!(o.multiplier().as_f64(), 50.0);
                assert_eq!(o.quote_currency().code.as_str(), "TWD");
            }
            _ => panic!("Expected OptionContract"),
        }
    }

    #[rstest]
    fn test_parse_date_to_nanos_slash_format() {
        // 2026/06/17 00:00:00 +08:00 = 2026-06-16T16:00:00Z
        let nanos = parse_date_to_nanos("2026/06/17").unwrap();
        assert_eq!(nanos.as_u64(), 1_781_625_600_000_000_000);
    }

    #[rstest]
    fn test_parse_date_to_nanos_dash_format() {
        // 2026-03-02 00:00:00 +08:00 = 2026-03-01T16:00:00Z
        let nanos = parse_date_to_nanos("2026-03-02").unwrap();
        assert_eq!(nanos.as_u64(), 1_772_380_800_000_000_000);
    }

    #[rstest]
    fn test_parse_all_instrument_types() {
        // Stocks -> Equity
        let stocks: Vec<StockContract> = load_test_json_as("contracts_stocks.json");
        for stock in &stocks {
            let instrument =
                parse_stock_to_equity(stock, UnixNanos::default(), UnixNanos::default());
            assert!(instrument.is_ok(), "Failed to parse stock: {}", stock.code);
            match instrument.unwrap() {
                InstrumentAny::Equity(_) => {}
                other => panic!("Expected Equity for {}, got {other:?}", stock.code),
            }
        }

        // Futures -> FuturesContract
        let futures: Vec<FuturesContract> = load_test_json_as("contracts_futures.json");
        for f in &futures {
            let instrument =
                parse_futures_to_contract(f, UnixNanos::default(), UnixNanos::default());
            assert!(instrument.is_ok(), "Failed to parse futures: {}", f.code);
            match instrument.unwrap() {
                InstrumentAny::FuturesContract(_) => {}
                other => panic!("Expected FuturesContract for {}, got {other:?}", f.code),
            }
        }

        // Options -> OptionContract
        let options: Vec<OptionsContract> = load_test_json_as("contracts_options.json");
        for opt in &options {
            let instrument =
                parse_options_to_contract(opt, UnixNanos::default(), UnixNanos::default());
            assert!(instrument.is_ok(), "Failed to parse option: {}", opt.code);
            match instrument.unwrap() {
                InstrumentAny::OptionContract(_) => {}
                other => panic!("Expected OptionContract for {}, got {other:?}", opt.code),
            }
        }
    }

    #[rstest]
    fn test_parse_stock_low_price_different_tick_size() {
        let contract = StockContract {
            code: "9999".to_string(),
            symbol: "TSE9999".to_string(),
            name: "Test".to_string(),
            exchange: "TSE".to_string(),
            category: "Test".to_string(),
            limit_up: 8.8,
            limit_down: 7.2,
            reference: 8.0, // < 10 TWD -> tick=0.01, precision=2
            update_date: "2026-03-04".to_string(),
            day_trade: "No".to_string(),
        };

        let instrument =
            parse_stock_to_equity(&contract, UnixNanos::default(), UnixNanos::default()).unwrap();
        match instrument {
            InstrumentAny::Equity(e) => {
                assert_eq!(e.price_precision(), 2); // 0.01 tick -> 2 decimals
            }
            _ => panic!("Expected Equity"),
        }
    }
}
