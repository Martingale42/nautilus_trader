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

//! Shared parsing helpers for Sinopac adapter.

use nautilus_core::UnixNanos;
use nautilus_model::{enums::OrderSide, identifiers::InstrumentId};

use super::consts::SINOPAC;

/// Constructs an `InstrumentId` from a Sinopac contract code.
///
/// Example: `"2330"` → `InstrumentId("2330.SINOPAC")`
pub fn parse_instrument_id(code: &str) -> anyhow::Result<InstrumentId> {
    InstrumentId::from_as_ref(format!("{code}.{SINOPAC}"))
}

/// Converts a Taiwan local time (UTC+8) `NaiveDateTime` to `UnixNanos`.
pub fn taiwan_naive_to_unix_nanos(dt: chrono::NaiveDateTime) -> anyhow::Result<UnixNanos> {
    let utc = dt - chrono::TimeDelta::hours(8);
    let nanos = utc
        .and_utc()
        .timestamp_nanos_opt()
        .ok_or_else(|| anyhow::anyhow!("Timestamp overflow for {dt}"))?;
    Ok(UnixNanos::from(nanos as u64))
}

/// Maps a Sinopac action string to a Nautilus `OrderSide`.
pub fn parse_order_side(action: &str) -> anyhow::Result<OrderSide> {
    match action {
        "Buy" => Ok(OrderSide::Buy),
        "Sell" => Ok(OrderSide::Sell),
        other => anyhow::bail!("Unknown Sinopac action: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_parse_instrument_id() {
        let id = parse_instrument_id("2330").unwrap();
        assert_eq!(id.to_string(), "2330.SINOPAC");
    }

    #[rstest]
    fn test_parse_instrument_id_futures() {
        let id = parse_instrument_id("TXFC6").unwrap();
        assert_eq!(id.to_string(), "TXFC6.SINOPAC");
    }

    #[rstest]
    fn test_parse_order_side_buy() {
        assert_eq!(parse_order_side("Buy").unwrap(), OrderSide::Buy);
    }

    #[rstest]
    fn test_parse_order_side_sell() {
        assert_eq!(parse_order_side("Sell").unwrap(), OrderSide::Sell);
    }

    #[rstest]
    fn test_parse_order_side_unknown() {
        assert!(parse_order_side("Unknown").is_err());
    }
}
