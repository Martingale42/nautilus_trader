//! MT5-specific enumerations.

use serde::{Deserialize, Serialize};
use std::fmt;

/// MT5 order types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Mt5OrderType {
    OrderTypeBuy,
    OrderTypeSell,
    OrderTypeBuyLimit,
    OrderTypeSellLimit,
    OrderTypeBuyStop,
    OrderTypeSellStop,
    OrderTypeBuyStopLimit,
    OrderTypeSellStopLimit,
    OrderTypeCloseBy,
}

impl fmt::Display for Mt5OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OrderTypeBuy => write!(f, "ORDER_TYPE_BUY"),
            Self::OrderTypeSell => write!(f, "ORDER_TYPE_SELL"),
            Self::OrderTypeBuyLimit => write!(f, "ORDER_TYPE_BUY_LIMIT"),
            Self::OrderTypeSellLimit => write!(f, "ORDER_TYPE_SELL_LIMIT"),
            Self::OrderTypeBuyStop => write!(f, "ORDER_TYPE_BUY_STOP"),
            Self::OrderTypeSellStop => write!(f, "ORDER_TYPE_SELL_STOP"),
            Self::OrderTypeBuyStopLimit => write!(f, "ORDER_TYPE_BUY_STOP_LIMIT"),
            Self::OrderTypeSellStopLimit => write!(f, "ORDER_TYPE_SELL_STOP_LIMIT"),
            Self::OrderTypeCloseBy => write!(f, "ORDER_TYPE_CLOSE_BY"),
        }
    }
}

/// MT5 order states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mt5OrderState {
    Started = 0,
    Placed = 1,
    Canceled = 2,
    Partial = 3,
    Filled = 4,
    Rejected = 5,
    Expired = 6,
    RequestAdd = 7,
    RequestModify = 8,
    RequestCancel = 9,
}

/// MT5 position types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mt5PositionType {
    Buy = 0,
    Sell = 1,
}

/// MT5 timeframes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mt5TimeFrame {
    #[serde(rename = "TICK")]
    Tick,
    #[serde(rename = "M1")]
    M1,
    #[serde(rename = "M5")]
    M5,
    #[serde(rename = "M15")]
    M15,
    #[serde(rename = "M30")]
    M30,
    #[serde(rename = "H1")]
    H1,
    #[serde(rename = "H4")]
    H4,
    #[serde(rename = "D1")]
    D1,
    #[serde(rename = "W1")]
    W1,
    #[serde(rename = "MN1")]
    MN1,
}

impl Mt5TimeFrame {
    /// Returns the timeframe in minutes (0 for tick data)
    pub const fn as_minutes(&self) -> i64 {
        match self {
            Self::Tick => 0,  // Tick data has no fixed timeframe
            Self::M1 => 1,
            Self::M5 => 5,
            Self::M15 => 15,
            Self::M30 => 30,
            Self::H1 => 60,
            Self::H4 => 240,
            Self::D1 => 1440,
            Self::W1 => 10080,
            Self::MN1 => 43200,
        }
    }
}

/// MT5 tick flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mt5TickFlags(pub u32);

impl Mt5TickFlags {
    pub const BID: u32 = 0x02;
    pub const ASK: u32 = 0x04;
    pub const LAST: u32 = 0x08;
    pub const VOLUME: u32 = 0x10;
    pub const BUY: u32 = 0x20;
    pub const SELL: u32 = 0x40;

    pub fn is_buy(&self) -> bool {
        self.0 & Self::BUY != 0
    }

    pub fn is_sell(&self) -> bool {
        self.0 & Self::SELL != 0
    }

    pub fn has_bid(&self) -> bool {
        self.0 & Self::BID != 0
    }

    pub fn has_ask(&self) -> bool {
        self.0 & Self::ASK != 0
    }

    pub fn has_last(&self) -> bool {
        self.0 & Self::LAST != 0
    }

    pub fn has_volume(&self) -> bool {
        self.0 & Self::VOLUME != 0
    }
}
