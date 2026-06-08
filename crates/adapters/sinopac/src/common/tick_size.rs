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

//! Tick size rules for TWSE/TAIFEX instruments.

/// Returns the TWSE tick size and price precision for a given stock reference price.
///
/// Official TWSE tick size schedule (as of 2020 revision):
///   Price < 10:       tick = 0.01  (precision 2)
///   10 <= Price < 50:  tick = 0.05  (precision 2)
///   50 <= Price < 100: tick = 0.10  (precision 2)
///   100 <= Price < 500: tick = 0.50 (precision 2)
///   500 <= Price < 1000: tick = 1.0 (precision 1)
///   Price >= 1000:     tick = 5.0   (precision 1)
///
/// `reference` is the contract's reference price (yesterday's close / IPO price).
pub fn twse_stock_tick_size(reference: f64) -> (f64, u8) {
    if reference < 10.0 {
        (0.01, 2)
    } else if reference < 50.0 {
        (0.05, 2)
    } else if reference < 100.0 {
        (0.10, 2)
    } else if reference < 500.0 {
        (0.50, 2)
    } else if reference < 1000.0 {
        (1.0, 1)
    } else {
        (5.0, 1)
    }
}

/// Returns the TWSE tick size and price precision for an ETF reference price.
///
/// ETFs / beneficiary certificates (TWSE `category == "00"`) follow a different,
/// coarser-below-50 / finer-above schedule than common stocks:
///   Price < 50:   tick = 0.01 (precision 2)
///   Price >= 50:  tick = 0.05 (precision 2)
///
/// This differs from [`twse_stock_tick_size`]: e.g. an ETF at 104 TWD ticks at
/// 0.05 (not the stock-tier 0.50), and an ETF at 36 TWD ticks at 0.01 (not the
/// stock-tier 0.05). `reference` is the contract's reference price.
pub fn twse_etf_tick_size(reference: f64) -> (f64, u8) {
    if reference < 50.0 { (0.01, 2) } else { (0.05, 2) }
}

/// Returns the tick size and precision for TAIFEX futures contracts.
///
/// TXF (TAIEX futures) and most index futures: tick = 1.0, precision 0.
/// Some sector futures have finer increments.
pub fn futures_tick_size(symbol: &str) -> (f64, u8) {
    match symbol {
        "TXF" | "MXF" | "T5F" | "XIF" => (1.0, 0),
        "ZEF" | "ZFF" => (0.2, 1),
        _ => (1.0, 0),
    }
}

/// Returns the tick size and precision for TAIFEX options contracts.
///
/// TXO (TAIEX options): premium < 10 -> tick 0.1, premium >= 10 -> tick 1.0.
/// Uses reference price as proxy for current premium level.
pub fn options_tick_size(reference: f64) -> (f64, u8) {
    if reference < 10.0 { (0.1, 1) } else { (1.0, 0) }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    fn test_twse_tick_size_under_10() {
        assert_eq!(twse_stock_tick_size(5.0), (0.01, 2));
    }

    #[rstest]
    fn test_twse_tick_size_10_to_50() {
        assert_eq!(twse_stock_tick_size(25.0), (0.05, 2));
    }

    #[rstest]
    fn test_twse_tick_size_50_to_100() {
        assert_eq!(twse_stock_tick_size(75.0), (0.10, 2));
    }

    #[rstest]
    fn test_twse_tick_size_100_to_500() {
        assert_eq!(twse_stock_tick_size(300.0), (0.50, 2));
    }

    #[rstest]
    fn test_twse_tick_size_500_to_1000() {
        assert_eq!(twse_stock_tick_size(750.0), (1.0, 1));
    }

    #[rstest]
    fn test_twse_tick_size_above_1000() {
        assert_eq!(twse_stock_tick_size(1500.0), (5.0, 1));
    }

    #[rstest]
    fn test_twse_tick_size_boundary_at_10() {
        assert_eq!(twse_stock_tick_size(10.0), (0.05, 2));
    }

    #[rstest]
    fn test_twse_tick_size_tsmc_reference() {
        // TSMC at ~580 TWD
        assert_eq!(twse_stock_tick_size(580.0), (1.0, 1));
    }

    #[rstest]
    fn test_etf_tick_size_below_50() {
        // 00631L at ~36.67 TWD -> ETF tick 0.01 (NOT the stock-tier 0.05)
        assert_eq!(twse_etf_tick_size(36.67), (0.01, 2));
    }

    #[rstest]
    fn test_etf_tick_size_at_or_above_50() {
        // 0050 at ~104 TWD -> ETF tick 0.05 (NOT the stock-tier 0.50)
        assert_eq!(twse_etf_tick_size(104.15), (0.05, 2));
        assert_eq!(twse_etf_tick_size(50.0), (0.05, 2));
    }

    #[rstest]
    fn test_etf_vs_stock_tick_diverge() {
        // Same reference, different schedule: ETF coarser below 50, finer above.
        assert_ne!(twse_etf_tick_size(104.15), twse_stock_tick_size(104.15));
        assert_eq!(twse_etf_tick_size(104.15), (0.05, 2));
        assert_eq!(twse_stock_tick_size(104.15), (0.50, 2));
    }

    #[rstest]
    fn test_futures_tick_size_txf() {
        assert_eq!(futures_tick_size("TXF"), (1.0, 0));
    }

    #[rstest]
    fn test_options_tick_size_low_premium() {
        assert_eq!(options_tick_size(5.0), (0.1, 1));
    }

    #[rstest]
    fn test_options_tick_size_high_premium() {
        assert_eq!(options_tick_size(50.0), (1.0, 0));
    }
}
