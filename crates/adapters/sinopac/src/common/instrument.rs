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
        "TXF" => 200.0,  // 台指期: 200 TWD per point
        "MXF" => 50.0,   // 小台指: 50 TWD per point
        "T5F" => 100.0,  // 台灣50期貨
        "XIF" => 200.0,  // 非金電期貨
        "ZEF" => 4000.0, // 電子期: 4000 TWD per point
        "ZFF" => 1000.0, // 金融期: 1000 TWD per point
        _ => 2000.0,     // Stock/commodity futures default
    }
}

/// Returns the contract multiplier for a TAIFEX options product.
///
/// `symbol` is the root symbol (e.g., "TXO").
pub fn options_multiplier(symbol: &str) -> f64 {
    match symbol {
        "TXO" => 50.0, // 台指選: 50 TWD per point
        _ => 2000.0,   // Stock options default
    }
}

/// Extract the root symbol from a Sinopac futures/options contract code.
///
/// Examples:
///   "TXFC6"     -> "TXF"  (strip delivery month code)
///   "TXFR1"     -> "TXF"  (strip continuous contract suffix)
///   "TXF202406" -> "TXF"  (strip YYYYMM)
///   "MXF"       -> "MXF"  (already root)
///   "TXO20000C6" -> "TXO" (strip strike + delivery)
///
/// Heuristic: find the first digit — everything before it is the root symbol,
/// capped at 3 characters (all TAIFEX product codes are 2-3 chars).
/// Delivery codes like "TXFC6" have a single-letter month code (A-L) before
/// the year digit, so we look for the first digit to find the boundary.
pub fn extract_root_symbol(code: &str) -> &str {
    let first_digit = code
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(code.len());
    // Cap at 3 since TAIFEX roots are at most 3 chars; month code comes after
    let end = first_digit.min(3);
    &code[..end]
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

    #[rstest]
    fn test_extract_root_symbol_futures_delivery() {
        assert_eq!(extract_root_symbol("TXFC6"), "TXF");
    }

    #[rstest]
    fn test_extract_root_symbol_futures_continuous() {
        assert_eq!(extract_root_symbol("TXFR1"), "TXF");
    }

    #[rstest]
    fn test_extract_root_symbol_futures_yyyymm() {
        assert_eq!(extract_root_symbol("TXF202406"), "TXF");
    }

    #[rstest]
    fn test_extract_root_symbol_bare() {
        assert_eq!(extract_root_symbol("MXF"), "MXF");
    }

    #[rstest]
    fn test_extract_root_symbol_options() {
        assert_eq!(extract_root_symbol("TXO20000C6"), "TXO");
    }
}
