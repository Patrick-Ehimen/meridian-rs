//! The error type for operations on core domain types.
//!
//! Per [ADR 0001] and step 05 of the build path, this crate is a library, so
//! it exposes a concrete `thiserror` enum rather than `anyhow`. Network,
//! JSON and WebSocket failures do not belong here; each adapter owns its own
//! error type and converts into this one at the trait boundary.
//!
//! [ADR 0001]: ../../../docs/decisions/0001-core-types.md

use thiserror::Error;

use crate::symbol::MAX_TICKER_LEN;

/// Every way an operation on a core type can fail.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// A price was constructed from a negative scaled value. Book prices are
    /// non-negative; a negative one means the adapter mis-parsed a payload.
    #[error("price cannot be negative (got scaled value {0})")]
    NegativePrice(i64),

    /// A quantity was constructed from a negative scaled value. Resting size
    /// is non-negative. Signed order flow is a different concept and will get
    /// its own type when OFI lands.
    #[error("quantity cannot be negative (got scaled value {0})")]
    NegativeQty(i64),

    /// A `Price * Qty` product did not fit in the `i128` intermediate, or a
    /// notional sum overflowed while aggregating levels.
    ///
    /// This is the typed error the ADR requires in place of a silent
    /// wraparound.
    #[error("notional overflowed the i128 intermediate")]
    PrecisionOverflow,

    /// A notional was divided by a zero quantity, which happens if a VWAP is
    /// computed over an empty or fully-deleted set of levels.
    #[error("cannot divide a notional by a zero quantity")]
    DivideByZeroQty,

    /// A ticker was longer than [`MAX_TICKER_LEN`] bytes. The ADR fixes the
    /// width, so this is a real ingest failure, not something to truncate.
    #[error("ticker {ticker:?} is {len} bytes, exceeds the {MAX_TICKER_LEN}-byte limit")]
    TickerTooLong { ticker: String, len: usize },

    /// A ticker contained a byte outside `[A-Z0-9]` after uppercasing.
    /// Venue-native identifiers that do not fit this shape (pool addresses,
    /// Solana mints) are the adapter's problem to map, not core's.
    #[error("ticker {ticker:?} must be ASCII alphanumeric")]
    TickerNotAscii { ticker: String },

    /// A ticker was empty.
    #[error("ticker cannot be empty")]
    TickerEmpty,

    /// A venue name did not match any [`Venue`](crate::Venue).
    #[error("unknown venue {0:?}")]
    UnknownVenue(String),

    /// A delta started after the update the book expected next, so at least
    /// one message was dropped. The book is untouched; the adapter must
    /// resnap. Never continue past this.
    #[error("sequence gap: expected update {expected}, delta starts at {got}")]
    SequenceGap { expected: u64, got: u64 },

    /// A delta claimed a first update id greater than its last. That is a
    /// malformed payload, not a gap.
    #[error("delta update range is inverted: first {first} > last {last}")]
    InvalidUpdateRange { first: u64, last: u64 },

    /// After an update the best bid was at or above the best ask. Either the
    /// venue sent a bad diff or an earlier one was misapplied; either way the
    /// book can no longer be trusted and must be resnapped.
    #[error("crossed book: best bid {best_bid} >= best ask {best_ask}")]
    CrossedBook {
        best_bid: crate::Price,
        best_ask: crate::Price,
    },

    /// A delta was routed to a book for a different venue or symbol. That is
    /// an adapter routing bug, and applying it would corrupt the book.
    #[error("delta for {got_venue} {got_symbol} applied to book for {venue} {symbol}")]
    BookMismatch {
        venue: crate::Venue,
        symbol: crate::Symbol,
        got_venue: crate::Venue,
        got_symbol: crate::Symbol,
    },
}
