use nautilus_model::{enums::OrderSide, identifiers::InstrumentId};

use super::consts::SINOPAC;

/// Constructs an `InstrumentId` from a Sinopac contract code.
///
/// Example: `"2330"` → `InstrumentId("2330.SINOPAC")`
pub fn parse_instrument_id(code: &str) -> anyhow::Result<InstrumentId> {
    InstrumentId::from_as_ref(format!("{code}.{SINOPAC}"))
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
    use super::*;

    #[test]
    fn test_parse_instrument_id() {
        let id = parse_instrument_id("2330").unwrap();
        assert_eq!(id.to_string(), "2330.SINOPAC");
    }

    #[test]
    fn test_parse_instrument_id_futures() {
        let id = parse_instrument_id("TXFC6").unwrap();
        assert_eq!(id.to_string(), "TXFC6.SINOPAC");
    }

    #[test]
    fn test_parse_order_side_buy() {
        assert_eq!(parse_order_side("Buy").unwrap(), OrderSide::Buy);
    }

    #[test]
    fn test_parse_order_side_sell() {
        assert_eq!(parse_order_side("Sell").unwrap(), OrderSide::Sell);
    }

    #[test]
    fn test_parse_order_side_unknown() {
        assert!(parse_order_side("Unknown").is_err());
    }
}
