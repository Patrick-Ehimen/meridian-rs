//! Book side and price level.
//!
//! `Level` carries only what every venue supports. Binance sends an order
//! count per level and Coinbase does not, so `num_orders` stays adapter-local
//! rather than becoming an `Option` that half the pipeline has to ignore.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::amount::{Price, Qty};

/// Which side of the book a level, update or aggressor sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Side {
    Bid,
    Ask,
}

impl Side {
    /// The other side. A buy aggressor lifts the ask; a sell hits the bid.
    pub const fn opposite(self) -> Self {
        match self {
            Self::Bid => Self::Ask,
            Self::Ask => Self::Bid,
        }
    }

    /// True if `a` is a better price than `b` from this side's point of view:
    /// higher for bids, lower for asks.
    ///
    /// This is the one place the bid/ask direction is spelled out for price
    /// comparisons; see ADR 0002 for why it is not encoded in the map key.
    pub fn is_better(self, a: Price, b: Price) -> bool {
        match self {
            Self::Bid => a > b,
            Self::Ask => a < b,
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bid => "bid",
            Self::Ask => "ask",
        })
    }
}

/// One resting price level: a price and the total quantity at it.
///
/// No `Ord` derive. A derived one would be lexicographic over `(price, qty)`,
/// which reads as "order by price" until two sides are compared and the
/// direction is silently wrong for bids. Order levels with
/// [`Side::is_better`] or by iterating a [`BookSide`](crate::BookSide).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Level {
    pub price: Price,
    pub qty: Qty,
}

impl Level {
    pub const fn new(price: Price, qty: Qty) -> Self {
        Self { price, qty }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(scaled: i64) -> Price {
        Price::from_scaled(scaled).expect("non-negative")
    }

    #[test]
    fn opposite_is_an_involution() {
        assert_eq!(Side::Bid.opposite(), Side::Ask);
        assert_eq!(Side::Ask.opposite(), Side::Bid);
        assert_eq!(Side::Bid.opposite().opposite(), Side::Bid);
    }

    #[test]
    fn better_means_higher_for_bids_and_lower_for_asks() {
        assert!(Side::Bid.is_better(price(101), price(100)));
        assert!(!Side::Bid.is_better(price(100), price(101)));
        assert!(Side::Ask.is_better(price(100), price(101)));
        assert!(!Side::Ask.is_better(price(101), price(100)));
    }

    #[test]
    fn equal_prices_are_not_better() {
        assert!(!Side::Bid.is_better(price(100), price(100)));
        assert!(!Side::Ask.is_better(price(100), price(100)));
    }
}
