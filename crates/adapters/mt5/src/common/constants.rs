//! MT5 constants including error codes and trade return codes.
//!
//! These constants are derived from the MT5 API documentation and
//! the MT5-ZeroMQ JsonAPI.mq5 implementation.

/// MT5 Trade Return Codes
///
/// These codes are returned by MT5 when executing trade operations.
/// Source: JsonAPI.mq5 lines 1227-1274
pub mod retcode {
    pub const REQUOTE: i32 = 10004;
    pub const REJECT: i32 = 10006;
    pub const CANCEL: i32 = 10007;
    pub const PLACED: i32 = 10008;
    pub const DONE: i32 = 10009;
    pub const DONE_PARTIAL: i32 = 10010;
    pub const ERROR: i32 = 10011;
    pub const TIMEOUT: i32 = 10012;
    pub const INVALID: i32 = 10013;
    pub const INVALID_VOLUME: i32 = 10014;
    pub const INVALID_PRICE: i32 = 10015;
    pub const INVALID_STOPS: i32 = 10016;
    pub const TRADE_DISABLED: i32 = 10017;
    pub const MARKET_CLOSED: i32 = 10018;
    pub const NO_MONEY: i32 = 10019;
    pub const PRICE_CHANGED: i32 = 10020;
    pub const PRICE_OFF: i32 = 10021;
    pub const INVALID_EXPIRATION: i32 = 10022;
    pub const ORDER_CHANGED: i32 = 10023;
    pub const TOO_MANY_REQUESTS: i32 = 10024;
    pub const NO_CHANGES: i32 = 10025;
    pub const SERVER_DISABLES_AT: i32 = 10026;
    pub const CLIENT_DISABLES_AT: i32 = 10027;
    pub const LOCKED: i32 = 10028;
    pub const FROZEN: i32 = 10029;
    pub const INVALID_FILL: i32 = 10030;
    pub const CONNECTION: i32 = 10031;
    pub const ONLY_REAL: i32 = 10032;
    pub const LIMIT_ORDERS: i32 = 10033;
    pub const LIMIT_VOLUME: i32 = 10034;
    pub const INVALID_ORDER: i32 = 10035;
    pub const POSITION_CLOSED: i32 = 10036;
    pub const INVALID_CLOSE_VOLUME: i32 = 10038;
    pub const CLOSE_ORDER_EXIST: i32 = 10039;
    pub const LIMIT_POSITIONS: i32 = 10040;
    pub const REJECT_CANCEL: i32 = 10041;
    pub const LONG_ONLY: i32 = 10042;
    pub const SHORT_ONLY: i32 = 10043;
    pub const CLOSE_ONLY: i32 = 10044;
}

/// MT5 Error Codes
///
/// General error codes returned by MT5 API functions.
pub mod error {
    pub const SUCCESS: i32 = 0;
    pub const MARKET_UNKNOWN_SYMBOL: i32 = 4301;
    pub const MARKET_WRONG_PROPERTY: i32 = 4303;
    pub const TRADE_DISABLED: i32 = 4752;
    pub const TRADE_POSITION_NOT_FOUND: i32 = 4753;
    pub const TRADE_ORDER_NOT_FOUND: i32 = 4754;

    // Custom error codes from MT5-ZeroMQ
    pub const DESERIALIZATION: i32 = 65537;
    pub const WRONG_ACTION: i32 = 65538;
    pub const WRONG_ACTION_TYPE: i32 = 65539;
    pub const WRONG_REQUEST_STRUCTURE: i32 = 65540;
    pub const UNKNOWN_SYMBOL: i32 = 65541;
    pub const ACCOUNT_HISTORY_ERROR: i32 = 65542;
}

/// Get human-readable description for MT5 trade return code
pub fn retcode_description(code: i32) -> &'static str {
    match code {
        retcode::REQUOTE => "Requote",
        retcode::REJECT => "Request rejected",
        retcode::CANCEL => "Request canceled",
        retcode::PLACED => "Order placed",
        retcode::DONE => "Request completed",
        retcode::DONE_PARTIAL => "Request completed partially",
        retcode::ERROR => "Request processing error",
        retcode::TIMEOUT => "Request canceled by timeout",
        retcode::INVALID => "Invalid request",
        retcode::INVALID_VOLUME => "Invalid volume",
        retcode::INVALID_PRICE => "Invalid price",
        retcode::INVALID_STOPS => "Invalid stops",
        retcode::TRADE_DISABLED => "Trade disabled",
        retcode::MARKET_CLOSED => "Market closed",
        retcode::NO_MONEY => "Not enough money",
        retcode::PRICE_CHANGED => "Price changed",
        retcode::PRICE_OFF => "No prices",
        retcode::INVALID_EXPIRATION => "Invalid expiration",
        retcode::ORDER_CHANGED => "Order state changed",
        retcode::TOO_MANY_REQUESTS => "Too many requests",
        retcode::NO_CHANGES => "No changes",
        retcode::SERVER_DISABLES_AT => "Autotrading disabled by server",
        retcode::CLIENT_DISABLES_AT => "Autotrading disabled by client",
        retcode::LOCKED => "Request locked",
        retcode::FROZEN => "Order or position frozen",
        retcode::INVALID_FILL => "Invalid fill type",
        retcode::CONNECTION => "No connection",
        retcode::ONLY_REAL => "Only real accounts allowed",
        retcode::LIMIT_ORDERS => "Orders limit reached",
        retcode::LIMIT_VOLUME => "Volume limit reached",
        retcode::INVALID_ORDER => "Invalid or prohibited order type",
        retcode::POSITION_CLOSED => "Position already closed",
        retcode::INVALID_CLOSE_VOLUME => "Invalid close volume",
        retcode::CLOSE_ORDER_EXIST => "Close order already exists",
        retcode::LIMIT_POSITIONS => "Positions limit reached",
        retcode::REJECT_CANCEL => "Reject cancel",
        retcode::LONG_ONLY => "Long only",
        retcode::SHORT_ONLY => "Short only",
        retcode::CLOSE_ONLY => "Close only",
        _ => "Unknown retcode",
    }
}

/// Check if retcode indicates success
pub fn is_success(code: i32) -> bool {
    matches!(code, retcode::DONE | retcode::DONE_PARTIAL | retcode::PLACED)
}

/// Check if retcode indicates rejection/failure
pub fn is_rejection(code: i32) -> bool {
    !is_success(code) && code != retcode::REQUOTE && code != retcode::PRICE_CHANGED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_success() {
        assert!(is_success(retcode::DONE));
        assert!(is_success(retcode::DONE_PARTIAL));
        assert!(is_success(retcode::PLACED));
        assert!(!is_success(retcode::REJECT));
        assert!(!is_success(retcode::INVALID));
    }

    #[test]
    fn test_is_rejection() {
        assert!(is_rejection(retcode::REJECT));
        assert!(is_rejection(retcode::INVALID));
        assert!(is_rejection(retcode::NO_MONEY));
        assert!(!is_rejection(retcode::DONE));
        assert!(!is_rejection(retcode::REQUOTE));
    }

    #[test]
    fn test_retcode_description() {
        assert_eq!(retcode_description(retcode::DONE), "Request completed");
        assert_eq!(retcode_description(retcode::NO_MONEY), "Not enough money");
        assert_eq!(retcode_description(99999), "Unknown retcode");
    }
}
