//! Executed trades, normalized across venues.

use serde::{Deserialize, Serialize};

use crate::amount::{Notional, Price, Qty};
use crate::error::CoreError;
use crate::side::Side;
use crate::symbol::Symbol;
use crate::time::Timestamp;
use crate::venue::Venue;

/// One print on one venue.
///
/// `aggressor` is an `Option` on purpose. Binance, Coinbase and Bybit all say
/// which side crossed the spread, but an on-chain swap only has a direction
/// relative to the pool, and some feeds omit it. Inferring it afterwards from
/// book state is lossy, so the type makes an adapter either supply it or say
/// plainly that it does not know. OFI must skip or separately count `None`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Trade {
    pub venue: Venue,
    pub symbol: Symbol,
    pub price: Price,
    pub qty: Qty,
    /// The side that took liquidity: `Bid` for a buy that lifted the ask,
    /// `Ask` for a sell that hit the bid. `None` if the venue does not say.
    pub aggressor: Option<Side>,
    /// The venue's own trade identifier, opaque and unique only within that
    /// venue and symbol. Binance and Coinbase send integers, Bybit a UUID,
    /// and a DEX swap is a transaction hash plus log index, so the common
    /// form is text. Used for de-duplication after a reconnect, never for
    /// ordering.
    pub trade_id: String,
    pub ts_exchange: Timestamp,
    pub ts_local: Timestamp,
}

impl Trade {
    /// `price * qty`, the trade's value at [`NOTIONAL_SCALE`](crate::NOTIONAL_SCALE).
    ///
    /// # Errors
    ///
    /// [`CoreError::PrecisionOverflow`] per [`Price::notional`].
    pub fn notional(&self) -> Result<Notional, CoreError> {
        self.price.notional(self.qty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amount::{NOTIONAL_SCALE_FACTOR, SCALE_FACTOR};

    fn trade(aggressor: Option<Side>) -> Trade {
        Trade {
            venue: Venue::Binance,
            symbol: Symbol::new("BTC", "USDT").expect("valid"),
            price: Price::from_scaled(100 * SCALE_FACTOR).expect("non-negative"),
            qty: Qty::from_scaled(3 * SCALE_FACTOR).expect("non-negative"),
            aggressor,
            trade_id: "12345".to_owned(),
            ts_exchange: Timestamp::from_unix_millis(1),
            ts_local: Timestamp::from_unix_millis(2),
        }
    }

    #[test]
    fn notional_is_price_times_qty() {
        assert_eq!(
            trade(Some(Side::Bid)).notional().map(Notional::scaled),
            Ok(300 * NOTIONAL_SCALE_FACTOR)
        );
    }

    #[test]
    fn unknown_aggressor_survives_a_serde_round_trip() {
        let t = trade(None);
        let json = serde_json::to_string(&t).expect("serializes");
        assert_eq!(serde_json::from_str::<Trade>(&json).expect("parses"), t);
    }
}
