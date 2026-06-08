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

//! Instrument helpers for Taiwan market contracts.

/// Default stock lot size for Common orders (1000 shares = 1 lot in Taiwan market).
pub const STOCK_LOT_SIZE: f64 = 1000.0;

/// Default lot size for futures/options contracts (1 contract).
pub const CONTRACT_LOT_SIZE: f64 = 1.0;

/// Size precision for all Taiwan instruments (whole shares/contracts).
pub const SIZE_PRECISION: u8 = 0;

/// TWD currency code.
pub const TWD: &str = "TWD";

/// Returns the contract multiplier for a TAIFEX futures product.
///
/// The multiplier defines how many TWD one index point is worth.
/// `symbol` is the root symbol (e.g., "TXF", "MXF"), not the delivery code.
pub fn futures_multiplier(symbol: &str) -> f64 {
    match symbol {
        "TXF" => 200.0,  // TAIEX futures: 200 TWD per point
        "MXF" => 50.0,   // Mini-TAIEX: 50 TWD per point
        "T5F" => 100.0,  // TAIEX 50 futures
        "XIF" => 200.0,  // Non-finance/electronics futures
        "ZEF" => 4000.0, // Electronics sector futures: 4000 TWD per point
        "ZFF" => 1000.0, // Finance sector futures: 1000 TWD per point
        _ => 2000.0,     // Stock/commodity futures default
    }
}

/// Returns the contract multiplier for a TAIFEX options product.
///
/// `symbol` is the root symbol (e.g., "TXO").
pub fn options_multiplier(symbol: &str) -> f64 {
    match symbol {
        "TXO" => 50.0, // TAIEX options: 50 TWD per point
        _ => 2000.0,   // Stock options default
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_futures_multiplier_txf() {
        assert_eq!(futures_multiplier("TXF"), 200.0);
    }

    #[rstest]
    fn test_futures_multiplier_mxf() {
        assert_eq!(futures_multiplier("MXF"), 50.0);
    }

    #[rstest]
    fn test_futures_multiplier_unknown_defaults_to_stock() {
        assert_eq!(futures_multiplier("ABC"), 2000.0);
    }

    #[rstest]
    fn test_options_multiplier_txo() {
        assert_eq!(options_multiplier("TXO"), 50.0);
    }
}
