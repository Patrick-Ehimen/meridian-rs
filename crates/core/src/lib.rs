//! Meridian shared domain types.
//!
//! Under construction. See `docs/notes/questions.md` for the open questions
//! that gate what lives here (primarily ADR 0001 for `Price`, `Qty`, `Symbol`).

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
