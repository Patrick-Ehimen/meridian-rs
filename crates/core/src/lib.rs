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
//! [ADR 0001]: ../../../docs/decisions/0001-core-types.md

mod amount;
mod error;
mod symbol;

pub use amount::{
    NOTIONAL_SCALE, NOTIONAL_SCALE_FACTOR, Notional, Price, Qty, SCALE, SCALE_FACTOR,
};
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
