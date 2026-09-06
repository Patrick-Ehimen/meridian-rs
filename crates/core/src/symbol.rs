//! Venue-agnostic instrument identity.
//!
//! Per [ADR 0001] Q6, a `Symbol` is structured (`{ base, quote }`) rather than
//! interned. Adapters translate their venue-native string into this shape, so
//! `BTCUSDT` on Binance and `BTC-USD` on Coinbase produce the same value and
//! consolidated depth can key on it.
//!
//! Venue-native identifiers that are not ticker pairs (a Uniswap pool address,
//! a Solana mint pair) deliberately do not fit. Each adapter owns its own
//! `HashMap<VenueSymbol, Symbol>` mapping, populated at startup.
//!
//! [ADR 0001]: ../../../docs/decisions/0001-core-types.md

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Maximum ticker length in bytes, fixed by ADR 0001.
///
/// `symbol_shape_analysis` shows every representative CEX and DEX ticker fits
/// in 12 bytes, and that a 20-byte Ethereum address and a 32-byte Solana
/// pubkey do not, which is what forces the two-layer design.
pub const MAX_TICKER_LEN: usize = 12;

/// One side of a trading pair, as fixed-width uppercase ASCII.
///
/// Fixed width keeps `Ticker` `Copy` and makes the derived `Ord` and `Hash` a
/// plain byte comparison, with none of the equal-but-hashing-differently risk
/// that Q5 found in an arbitrary-precision decimal.
///
/// Shorter tickers are right-padded with zero bytes. Padding is part of the
/// representation, so `BTC` has exactly one encoding and equality is stable.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Ticker([u8; MAX_TICKER_LEN]);

impl Ticker {
    /// Parses a ticker, uppercasing ASCII letters.
    ///
    /// Uppercasing is what makes cross-venue equality work: venues disagree on
    /// case, and a `Symbol` that compares unequal because one feed said `usdt`
    /// would silently split consolidated depth in two.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::TickerEmpty`], [`CoreError::TickerTooLong`] or
    /// [`CoreError::TickerNotAscii`] rather than truncating or substituting.
    pub fn new(raw: &str) -> Result<Self, CoreError> {
        if raw.is_empty() {
            return Err(CoreError::TickerEmpty);
        }
        if raw.len() > MAX_TICKER_LEN {
            return Err(CoreError::TickerTooLong {
                ticker: raw.to_owned(),
                len: raw.len(),
            });
        }

        let mut bytes = [0u8; MAX_TICKER_LEN];
        for (slot, byte) in bytes.iter_mut().zip(raw.bytes()) {
            if !byte.is_ascii_alphanumeric() {
                return Err(CoreError::TickerNotAscii {
                    ticker: raw.to_owned(),
                });
            }
            *slot = byte.to_ascii_uppercase();
        }
        Ok(Self(bytes))
    }

    /// The ticker as text, without the zero padding.
    pub fn as_str(&self) -> &str {
        let end = self
            .0
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(MAX_TICKER_LEN);
        // Construction rejects every non-ASCII byte, so this slice is always
        // valid UTF-8. `unwrap_or` keeps the no-panic rule for library code.
        std::str::from_utf8(&self.0[..end]).unwrap_or("")
    }
}

impl fmt::Display for Ticker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// Derived Debug would print twelve raw bytes. Logs and gap post-mortems are
// the main consumer of Debug here, so it renders the text instead.
impl fmt::Debug for Ticker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ticker({:?})", self.as_str())
    }
}

impl TryFrom<String> for Ticker {
    type Error = CoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<Ticker> for String {
    fn from(value: Ticker) -> Self {
        value.as_str().to_owned()
    }
}

/// A trading pair, normalized across venues.
///
/// The derived `Ord` is lexicographic by field declaration order, so symbols
/// sort by `base` first and then `quote`. That is deliberate: it groups every
/// quote currency for one asset together when a book map is iterated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Symbol {
    pub base: Ticker,
    pub quote: Ticker,
}

impl Symbol {
    /// Builds a symbol from two venue-native ticker strings.
    ///
    /// # Errors
    ///
    /// Propagates any [`Ticker::new`] failure, naming which side failed by
    /// way of the offending string in the error.
    pub fn new(base: &str, quote: &str) -> Result<Self, CoreError> {
        Ok(Self {
            base: Ticker::new(base)?,
            quote: Ticker::new(quote)?,
        })
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.base, self.quote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ticker_is_rejected() {
        assert_eq!(Ticker::new(""), Err(CoreError::TickerEmpty));
    }

    #[test]
    fn oversized_ticker_is_rejected_rather_than_truncated() {
        let raw = "A".repeat(MAX_TICKER_LEN + 1);
        assert_eq!(
            Ticker::new(&raw),
            Err(CoreError::TickerTooLong {
                ticker: raw,
                len: MAX_TICKER_LEN + 1,
            })
        );
    }

    #[test]
    fn a_ticker_of_exactly_max_len_is_accepted() {
        let raw = "A".repeat(MAX_TICKER_LEN);
        let ticker = Ticker::new(&raw).expect("boundary length fits");
        assert_eq!(ticker.as_str(), raw);
    }

    // Coinbase sends "BTC-USD", so the separator has to be stripped by the
    // adapter before it reaches core. Accepting it here would produce a
    // Ticker that never compares equal to Binance's.
    #[test]
    fn separators_and_other_punctuation_are_rejected() {
        assert_eq!(
            Ticker::new("BTC-USD"),
            Err(CoreError::TickerNotAscii {
                ticker: "BTC-USD".to_owned()
            })
        );
    }

    #[test]
    fn non_ascii_is_rejected() {
        let err = Ticker::new("BTĆ").expect_err("multi-byte char");
        assert!(matches!(err, CoreError::TickerNotAscii { .. }));
    }

    // The whole point of uppercasing: venues disagree on case, and a Symbol
    // that compared unequal on case would silently split consolidated depth
    // into two half-books.
    #[test]
    fn case_is_normalized_so_venues_agree() {
        assert_eq!(Ticker::new("usdt"), Ticker::new("USDT"));
        assert_eq!(
            Symbol::new("btc", "usdt").expect("valid"),
            Symbol::new("BTC", "USDT").expect("valid")
        );
    }

    #[test]
    fn as_str_does_not_leak_the_zero_padding() {
        let ticker = Ticker::new("BTC").expect("valid");
        assert_eq!(ticker.as_str(), "BTC");
        assert_eq!(ticker.as_str().len(), 3);
    }

    #[test]
    fn a_shorter_ticker_has_exactly_one_encoding() {
        assert_eq!(Ticker::new("BTC"), Ticker::new("btc"));
        let a = Ticker::new("BTC").expect("valid");
        let b = Ticker::new("BTC").expect("valid");
        assert_eq!(a, b);

        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(a, 1);
        assert_eq!(map.get(&b), Some(&1));
    }

    #[test]
    fn display_renders_a_readable_pair() {
        let symbol = Symbol::new("BTC", "USDT").expect("valid");
        assert_eq!(symbol.to_string(), "BTC/USDT");
        assert_eq!(format!("{:?}", symbol.base), r#"Ticker("BTC")"#);
    }

    // Ord is lexicographic by field order, which groups every quote for one
    // base together when a book map is iterated.
    #[test]
    fn symbols_sort_by_base_then_quote() {
        let mut symbols = [
            Symbol::new("ETH", "USDT").expect("valid"),
            Symbol::new("BTC", "USDT").expect("valid"),
            Symbol::new("BTC", "EUR").expect("valid"),
        ];
        symbols.sort();
        assert_eq!(
            symbols.map(|s| s.to_string()),
            ["BTC/EUR", "BTC/USDT", "ETH/USDT"]
        );
    }

    #[test]
    fn serde_round_trips_as_text() {
        let symbol = Symbol::new("BTC", "USDT").expect("valid");
        let json = serde_json::to_string(&symbol).expect("serializes");
        assert_eq!(json, r#"{"base":"BTC","quote":"USDT"}"#);
        assert_eq!(
            serde_json::from_str::<Symbol>(&json).expect("parses"),
            symbol
        );
    }

    // Deserialization has to run the same validation as the constructor,
    // otherwise a payload can mint a Ticker that Ticker::new would reject.
    #[test]
    fn deserializing_an_invalid_ticker_is_rejected() {
        assert!(serde_json::from_str::<Ticker>(r#""BTC-USD""#).is_err());
        assert!(serde_json::from_str::<Ticker>(r#""AAAAAAAAAAAAA""#).is_err());
        assert!(serde_json::from_str::<Ticker>(r#""""#).is_err());
    }
}
