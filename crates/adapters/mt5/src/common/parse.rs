//! Utility parsing functions for MT5 adapter.

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
/// MT5 sends timestamps in milliseconds, we need to convert to nanoseconds
pub fn parse_timestamp_ms(ms: i64) -> u64 {
    (ms * 1_000_000) as u64
}

/// Parse timestamp from MT5 seconds to UnixNanos
pub fn parse_timestamp_s(s: i64) -> u64 {
    (s * 1_000_000_000) as u64
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
        let ns = parse_timestamp_ms(ms);
        assert_eq!(ns, 1709891679000000000);
    }

    #[test]
    fn test_parse_timestamp_s() {
        let s = 1709891679_i64;
        let ns = parse_timestamp_s(s);
        assert_eq!(ns, 1709891679000000000);
    }
}
