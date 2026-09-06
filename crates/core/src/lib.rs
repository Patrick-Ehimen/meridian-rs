//! Meridian shared domain types.
//!
//! Every other crate speaks these types. Adapters normalize venue-native
//! payloads *into* them; the aggregator, servers and TUI only ever see them.
//!
//! Numeric representation is fixed by [ADR 0001]. The fixed-point `Price`
//! and `Qty` types land in a follow-up commit; this one establishes the
//! instrument identity and error vocabulary they build on.
//!
//! [ADR 0001]: ../../../docs/decisions/0001-core-types.md

mod error;
mod symbol;

pub use error::CoreError;
pub use symbol::{MAX_TICKER_LEN, Symbol, Ticker};

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
