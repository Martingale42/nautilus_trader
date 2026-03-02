//! Utility parsing functions for MT5 adapter.

use nautilus_core::datetime::{millis_to_nanos, secs_to_nanos};
use nautilus_model::identifiers::{InstrumentId, Symbol, Venue};

/// Parse MT5 symbol string to Nautilus InstrumentId
///
/// # Examples
/// ```
/// # use nautilus_mt5::common::parse::parse_instrument_id;
/// let instrument_id = parse_instrument_id("EURUSD").unwrap();
/// assert_eq!(instrument_id.symbol.as_str(), "EURUSD");
/// ```
pub fn parse_instrument_id(symbol: &str) -> anyhow::Result<InstrumentId> {
    let venue = Venue::new("MT5");
    let symbol = Symbol::new(symbol);
    Ok(InstrumentId::new(symbol, venue))
}

/// Parse timestamp from MT5 milliseconds to UnixNanos
///
/// Delegates to `nautilus_core::datetime::millis_to_nanos` for overflow-checked conversion.
pub fn parse_timestamp_ms(ms: i64) -> anyhow::Result<u64> {
    millis_to_nanos(ms as f64)
}

/// Parse timestamp from MT5 seconds to UnixNanos
///
/// Delegates to `nautilus_core::datetime::secs_to_nanos` for overflow-checked conversion.
pub fn parse_timestamp_s(s: i64) -> anyhow::Result<u64> {
    secs_to_nanos(s as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_instrument_id() {
        let id = parse_instrument_id("EURUSD").unwrap();
        assert_eq!(id.symbol.as_str(), "EURUSD");
        assert_eq!(id.venue.as_str(), "MT5");
    }

    #[test]
    fn test_parse_timestamp_ms() {
        let ms = 1709891679000_i64;
        let ns = parse_timestamp_ms(ms).unwrap();
        assert_eq!(ns, 1709891679000000000);
    }

    #[test]
    fn test_parse_timestamp_s() {
        let s = 1709891679_i64;
        let ns = parse_timestamp_s(s).unwrap();
        assert_eq!(ns, 1709891679000000000);
    }
}
