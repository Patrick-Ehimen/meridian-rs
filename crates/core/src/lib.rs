//! Meridian shared domain types.
//!
//! Every other crate speaks these types. Adapters normalize venue-native
//! payloads *into* them; the aggregator, servers and TUI only ever see them.
//!
//! Numeric representation is fixed by [ADR 0001]: prices and quantities are
//! fixed-point integers at [`SCALE`] decimal places, stored in `i64`, with an
//! `i128` intermediate for every product. See [`amount`] for the arithmetic
//! rules and [`symbol`] for cross-venue instrument identity.
//!
//! Book storage is fixed by [ADR 0002]: each side is a `BTreeMap<Price, Qty>`
//! behind a [`BookSide`] that iterates best first, so no caller handles the
//! bid/ask direction by hand. See `book.rs` for the sequencing contract that
//! [`Book::apply`] enforces.
//!
//! The adapter interface is fixed by [ADR 0003]: see `feed.rs` for
//! [`VenueFeed`], the stream contract, and the transient/fatal error split.
//!
//! [ADR 0001]: ../../../docs/decisions/0001-core-types.md
//! [ADR 0002]: ../../../docs/decisions/0002-book-storage.md
//! [ADR 0003]: ../../../docs/decisions/0003-venue-feed-shape.md

mod amount;
mod book;
mod error;
mod feed;
mod side;
mod symbol;
mod time;
mod trade;
mod venue;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use amount::{
    NOTIONAL_SCALE, NOTIONAL_SCALE_FACTOR, Notional, Price, Qty, SCALE, SCALE_FACTOR,
};
pub use book::{ApplyOutcome, Book, BookDelta, BookEvent, BookSide, LevelUpdate, Levels};
pub use error::CoreError;
pub use feed::{
    ConnStatus, DEFAULT_FEED_CAPACITY, FeedError, FeedReceiver, FeedSink, VenueEvent, VenueFeed,
    feed_channel, run_session,
};
pub use side::{Level, Side};
pub use symbol::{MAX_TICKER_LEN, Symbol, Ticker};
pub use time::Timestamp;
pub use trade::Trade;
pub use venue::Venue;

#[cfg(test)]
mod fixed_scale_analysis;

#[cfg(test)]
mod notional_overflow_analysis;

#[cfg(test)]
mod qty_scale_analysis;

#[cfg(test)]
mod rust_decimal_hash_check;

#[cfg(test)]
mod symbol_shape_analysis;
