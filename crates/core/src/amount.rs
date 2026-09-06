//! Fixed-point prices, quantities and notionals.
//!
//! Per [ADR 0001], every price and quantity is an integer count of
//! `10^-12` units held in an `i64`, and every `Price * Qty` widens to `i128`
//! before anything is shifted back. The scales compose:
//!
//! ```text
//! Price  (scale 12)  *  Qty (scale 12)  ->  Notional (scale 24)
//! Notional (scale 24) / Qty (scale 12)  ->  Price    (scale 12)
//! ```
//!
//! That second line is VWAP, which is why the notional keeps the doubled
//! scale instead of being shifted straight back down: shifting per level
//! would round every level before the sum, and the rounding would
//! accumulate across the hundreds of levels a consolidated book carries.
//!
//! # Why there are no operator impls
//!
//! `Price + Price` is meaningless and deliberately does not compile. The
//! operations that *are* meaningful still do not get `std::ops` impls,
//! because every one of them can overflow, and the `std::ops` signatures
//! have nowhere to report that: they would panic in debug and silently
//! wrap in release. ADR 0001 requires a typed error instead, so the
//! arithmetic lives in inherent methods returning [`Result`].
//!
//! [ADR 0001]: ../../../docs/decisions/0001-core-types.md

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Decimal places carried by every [`Price`] and [`Qty`], fixed by ADR 0001.
///
/// `fixed_scale_analysis` shows 12 is the narrowest scale that covers the
/// current pair range in `i64`, and `qty_scale_analysis` shows the
/// truncation it imposes on 18-decimal tokens stays below a $0.01 dust
/// threshold.
pub const SCALE: u32 = 12;

/// `10^SCALE`. One whole unit of price or quantity, in scaled form.
pub const SCALE_FACTOR: i64 = 10i64.pow(SCALE);

/// Decimal places carried by a [`Notional`], which is the product of two
/// scale-12 values and therefore twice the scale.
pub const NOTIONAL_SCALE: u32 = 2 * SCALE;

/// `10^NOTIONAL_SCALE`. One whole unit of notional, in scaled form.
pub const NOTIONAL_SCALE_FACTOR: i128 = 10i128.pow(NOTIONAL_SCALE);

/// Renders a scaled integer as decimal text, trimming trailing zeros.
///
/// Values reaching here are non-negative by construction, so this does not
/// deal with a negative fractional part.
fn fmt_scaled(value: i128, scale: u32, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let factor = 10i128.pow(scale);
    let whole = value / factor;
    let frac = value % factor;
    if frac == 0 {
        return write!(f, "{whole}");
    }
    let mut digits = format!("{frac:0width$}", width = scale as usize);
    while digits.ends_with('0') {
        digits.pop();
    }
    write!(f, "{whole}.{digits}")
}

/// A price, as an integer count of `10^-12` quote units.
///
/// Non-negative by construction: a negative price means an adapter
/// mis-parsed a payload, so [`Price::from_scaled`] rejects it rather than
/// letting it reach a book.
///
/// `Price + Price` is not a meaningful operation and does not compile:
///
/// ```compile_fail
/// use meridian_core::Price;
/// let a = Price::from_scaled(1).unwrap();
/// let b = Price::from_scaled(2).unwrap();
/// let _ = a + b;
/// ```
///
/// Neither does multiplying a price by a quantity with `*`. ADR 0001 routes
/// every product through [`Price::notional`] so overflow is a typed error:
///
/// ```compile_fail
/// use meridian_core::{Price, Qty};
/// let p = Price::from_scaled(1).unwrap();
/// let q = Qty::from_scaled(2).unwrap();
/// let _ = p * q;
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct Price(i64);

impl Price {
    /// The zero price. Useful as a fold identity, not as a real book level.
    pub const ZERO: Self = Self(0);

    /// Builds a price from an already-scaled integer.
    ///
    /// # Errors
    ///
    /// [`CoreError::NegativePrice`] if `scaled` is negative.
    pub fn from_scaled(scaled: i64) -> Result<Self, CoreError> {
        if scaled < 0 {
            return Err(CoreError::NegativePrice(scaled));
        }
        Ok(Self(scaled))
    }

    /// The underlying scaled integer, at [`SCALE`] decimal places.
    pub const fn scaled(self) -> i64 {
        self.0
    }

    /// True if this is the zero price.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// The gap between two prices, as a non-negative price-shaped value.
    ///
    /// Used for spreads. This is symmetric on purpose: the caller knows
    /// which side is which, and a signed difference would need a fourth
    /// type that nothing downstream currently consumes.
    pub const fn abs_diff(self, other: Self) -> Self {
        if self.0 >= other.0 {
            Self(self.0 - other.0)
        } else {
            Self(other.0 - self.0)
        }
    }

    /// `self * qty`, widened to an `i128` intermediate at
    /// [`NOTIONAL_SCALE`].
    ///
    /// # Errors
    ///
    /// [`CoreError::PrecisionOverflow`] if the product does not fit. Two
    /// non-negative `i64`s cannot actually overflow an `i128` (the largest
    /// possible product is about `8.5e37` against a bound of `1.7e38`), so
    /// this path is unreachable at the current widths. It stays checked
    /// because ADR 0001 lists a width change as a revisit trigger, and a
    /// silent wraparound is exactly what the ADR forbids.
    pub fn notional(self, qty: Qty) -> Result<Notional, CoreError> {
        i128::from(self.0)
            .checked_mul(i128::from(qty.scaled()))
            .map(Notional)
            .ok_or(CoreError::PrecisionOverflow)
    }
}

/// A resting quantity, as an integer count of `10^-12` base units.
///
/// Non-negative by construction. Signed order flow is a different concept
/// and gets its own type when OFI lands.
///
/// `qty == 0` at a level is the universal CEX convention for "delete this
/// level", which is why [`Qty::is_zero`] exists rather than callers
/// comparing against a literal.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct Qty(i64);

impl Qty {
    /// The zero quantity, which by CEX convention means a deleted level.
    pub const ZERO: Self = Self(0);

    /// Builds a quantity from an already-scaled integer.
    ///
    /// # Errors
    ///
    /// [`CoreError::NegativeQty`] if `scaled` is negative.
    pub fn from_scaled(scaled: i64) -> Result<Self, CoreError> {
        if scaled < 0 {
            return Err(CoreError::NegativeQty(scaled));
        }
        Ok(Self(scaled))
    }

    /// The underlying scaled integer, at [`SCALE`] decimal places.
    pub const fn scaled(self) -> i64 {
        self.0
    }

    /// True if this level is empty, which venues use to mean "delete".
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// `self + rhs`.
    ///
    /// # Errors
    ///
    /// [`CoreError::PrecisionOverflow`] if the sum leaves `i64`.
    pub fn checked_add(self, rhs: Self) -> Result<Self, CoreError> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(CoreError::PrecisionOverflow)
    }

    /// `self - rhs`.
    ///
    /// # Errors
    ///
    /// [`CoreError::NegativeQty`] if the result would go below zero.
    pub fn checked_sub(self, rhs: Self) -> Result<Self, CoreError> {
        let raw = self
            .0
            .checked_sub(rhs.0)
            .ok_or(CoreError::PrecisionOverflow)?;
        Self::from_scaled(raw)
    }
}

/// The value of a level or a fill: `Price * Qty`, at [`NOTIONAL_SCALE`].
///
/// Held in `i128` because the doubled scale puts realistic level values well
/// past `i64::MAX`. `notional_overflow_analysis` shows the width absorbs
/// every pair in the current set with 6-8 orders of headroom, and that
/// summing 500 levels still fits.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "i128", into = "i128")]
pub struct Notional(i128);

impl Notional {
    /// The zero notional, and the identity when folding levels.
    pub const ZERO: Self = Self(0);

    /// Builds a notional from an already-scaled `i128`.
    ///
    /// # Errors
    ///
    /// [`CoreError::PrecisionOverflow`] if `scaled` is negative. A negative
    /// notional can only come from a negative price or quantity, both of
    /// which are rejected upstream.
    pub fn from_scaled(scaled: i128) -> Result<Self, CoreError> {
        if scaled < 0 {
            return Err(CoreError::PrecisionOverflow);
        }
        Ok(Self(scaled))
    }

    /// The underlying scaled integer, at [`NOTIONAL_SCALE`] decimal places.
    pub const fn scaled(self) -> i128 {
        self.0
    }

    /// `self + rhs`, for aggregating levels into a book total.
    ///
    /// # Errors
    ///
    /// [`CoreError::PrecisionOverflow`] if the sum leaves `i128`.
    pub fn checked_add(self, rhs: Self) -> Result<Self, CoreError> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(CoreError::PrecisionOverflow)
    }

    /// Volume-weighted average price: `self / qty`, landing back at
    /// [`SCALE`].
    ///
    /// # Errors
    ///
    /// [`CoreError::DivideByZeroQty`] if `qty` is zero, which happens when a
    /// VWAP is taken over an empty or fully-deleted set of levels.
    /// [`CoreError::PrecisionOverflow`] if the quotient does not fit `i64`.
    pub fn vwap(self, qty: Qty) -> Result<Price, CoreError> {
        if qty.is_zero() {
            return Err(CoreError::DivideByZeroQty);
        }
        let quotient = self.0 / i128::from(qty.scaled());
        let narrowed = i64::try_from(quotient).map_err(|_| CoreError::PrecisionOverflow)?;
        Price::from_scaled(narrowed)
    }
}

// Serde and Display plumbing. The `try_from` conversions are what keep the
// non-negative invariant intact across a deserialize: a plain
// `#[serde(transparent)]` would let `-1` construct a Price directly from
// JSON and bypass `from_scaled` entirely.

impl TryFrom<i64> for Price {
    type Error = CoreError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::from_scaled(value)
    }
}

impl From<Price> for i64 {
    fn from(value: Price) -> Self {
        value.0
    }
}

impl TryFrom<i64> for Qty {
    type Error = CoreError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::from_scaled(value)
    }
}

impl From<Qty> for i64 {
    fn from(value: Qty) -> Self {
        value.0
    }
}

impl TryFrom<i128> for Notional {
    type Error = CoreError;
    fn try_from(value: i128) -> Result<Self, Self::Error> {
        Self::from_scaled(value)
    }
}

impl From<Notional> for i128 {
    fn from(value: Notional) -> Self {
        value.0
    }
}

// Derived Debug would print the raw scaled integer, which is unreadable in a
// gap post-mortem: `Price(100000000000000)` rather than `Price(100000)`.

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_scaled(i128::from(self.0), SCALE, f)
    }
}

impl fmt::Debug for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Price({self})")
    }
}

impl fmt::Display for Qty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_scaled(i128::from(self.0), SCALE, f)
    }
}

impl fmt::Debug for Qty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Qty({self})")
    }
}

impl fmt::Display for Notional {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_scaled(self.0, NOTIONAL_SCALE, f)
    }
}

impl fmt::Debug for Notional {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Notional({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a price from whole quote units, for readable test setup.
    fn price(whole: i64) -> Price {
        Price::from_scaled(whole * SCALE_FACTOR).expect("non-negative")
    }

    /// Builds a quantity from whole base units, for readable test setup.
    fn qty(whole: i64) -> Qty {
        Qty::from_scaled(whole * SCALE_FACTOR).expect("non-negative")
    }

    #[test]
    fn negative_price_is_rejected() {
        assert_eq!(Price::from_scaled(-1), Err(CoreError::NegativePrice(-1)));
    }

    #[test]
    fn negative_qty_is_rejected() {
        assert_eq!(Qty::from_scaled(-1), Err(CoreError::NegativeQty(-1)));
    }

    #[test]
    fn zero_qty_marks_a_deleted_level() {
        assert!(Qty::ZERO.is_zero());
        assert!(!qty(1).is_zero());
    }

    // Price (scale 12) * Qty (scale 12) has to land at scale 24, or every
    // downstream notional is off by twelve orders of magnitude.
    #[test]
    fn notional_composes_the_two_scales() {
        let n = price(100_000).notional(qty(2)).expect("fits i128");
        assert_eq!(n.scaled(), 200_000 * NOTIONAL_SCALE_FACTOR);
        assert_eq!(n.to_string(), "200000");
    }

    #[test]
    fn vwap_inverts_notional() {
        let n = price(100_000).notional(qty(2)).expect("fits i128");
        assert_eq!(n.vwap(qty(2)), Ok(price(100_000)));
    }

    // The operation VWAP actually performs: sum notionals across levels,
    // divide by summed quantity. (100*2 + 200*3) / 5 == 160.
    #[test]
    fn vwap_over_multiple_levels_is_weighted() {
        let levels = [(price(100), qty(2)), (price(200), qty(3))];

        let mut total_notional = Notional::ZERO;
        let mut total_qty = Qty::ZERO;
        for (p, q) in levels {
            total_notional = total_notional
                .checked_add(p.notional(q).expect("fits i128"))
                .expect("fits i128");
            total_qty = total_qty.checked_add(q).expect("fits i64");
        }

        assert_eq!(total_notional.vwap(total_qty), Ok(price(160)));
    }

    #[test]
    fn vwap_of_a_fully_deleted_book_is_a_typed_error() {
        assert_eq!(
            Notional::ZERO.vwap(Qty::ZERO),
            Err(CoreError::DivideByZeroQty)
        );
    }

    #[test]
    fn notional_sum_overflow_is_typed_not_a_wraparound() {
        let max = Notional::from_scaled(i128::MAX).expect("non-negative");
        let one = Notional::from_scaled(1).expect("non-negative");
        assert_eq!(max.checked_add(one), Err(CoreError::PrecisionOverflow));
    }

    #[test]
    fn qty_adds_and_subtracts() {
        assert_eq!(qty(3).checked_add(qty(4)), Ok(qty(7)));
        assert_eq!(qty(7).checked_sub(qty(4)), Ok(qty(3)));
    }

    #[test]
    fn qty_cannot_be_subtracted_below_zero() {
        let err = qty(1).checked_sub(qty(2)).expect_err("goes negative");
        assert!(matches!(err, CoreError::NegativeQty(_)));
    }

    #[test]
    fn display_trims_trailing_zeros() {
        assert_eq!(price(100_000).to_string(), "100000");
        assert_eq!(
            Price::from_scaled(SCALE_FACTOR / 2)
                .expect("non-negative")
                .to_string(),
            "0.5"
        );
        assert_eq!(
            Price::from_scaled(1).expect("non-negative").to_string(),
            "0.000000000001"
        );
    }

    #[test]
    fn debug_renders_decimal_text_not_the_scaled_integer() {
        assert_eq!(format!("{:?}", price(100_000)), "Price(100000)");
    }

    // A plain `#[serde(transparent)]` would let a hostile or buggy payload
    // construct a negative Price straight from JSON, bypassing from_scaled.
    #[test]
    fn deserializing_a_negative_price_is_rejected() {
        assert!(serde_json::from_str::<Price>("-1").is_err());
        assert!(serde_json::from_str::<Qty>("-1").is_err());
    }

    #[test]
    fn serde_round_trips_through_the_scaled_integer() {
        let json = serde_json::to_string(&price(3)).expect("serializes");
        assert_eq!(json, "3000000000000");
        assert_eq!(
            serde_json::from_str::<Price>(&json).expect("parses"),
            price(3)
        );
    }

    // Book sides are keyed by Price, so the Ord derive has to be the plain
    // numeric one.
    #[test]
    fn prices_order_numerically() {
        let mut sorted = [price(3), price(1), price(2)];
        sorted.sort();
        assert_eq!(sorted, [price(1), price(2), price(3)]);
    }

    #[test]
    fn abs_diff_is_symmetric() {
        assert_eq!(price(10).abs_diff(price(4)), price(6));
        assert_eq!(price(4).abs_diff(price(10)), price(6));
    }

    // KNOWN LIMITATION, not a design intent.
    //
    // ADR 0001's own context names "~10^13 PEPE tokens" as a realistic
    // per-level quantity, but a uniform scale of 12 in i64 caps Qty at
    // i64::MAX / 10^12, which is about 9.22e6 whole units. A PEPE or SHIB
    // level overflows that ceiling by five to six orders of magnitude.
    //
    // The Q3 analysis never caught this because every one of its tests
    // checks the Price * Qty *product* against i64/i128/i256 and none
    // checks that the scaled *operand* fits its i64 storage.
    //
    // This test pins the real ceiling so the limit is visible in the test
    // output rather than discovered by an adapter in production.
    #[test]
    fn qty_ceiling_at_scale_12_excludes_high_supply_tokens() {
        let max_whole_units = i64::MAX / SCALE_FACTOR;
        println!("max representable Qty at scale {SCALE} = {max_whole_units} whole units");
        assert_eq!(max_whole_units, 9_223_372);

        // A 10^13 PEPE level cannot even be scaled without leaving i64.
        let pepe_level: i64 = 10_000_000_000_000;
        assert_eq!(pepe_level.checked_mul(SCALE_FACTOR), None);

        // Nor can a 10^12 SHIB level.
        let shib_level: i64 = 1_000_000_000_000;
        assert_eq!(shib_level.checked_mul(SCALE_FACTOR), None);

        // What does fit is anything up to roughly 9.2 million units, which
        // covers every BTC, ETH and SOL level but no high-supply memecoin.
        assert!(Qty::from_scaled(9_000_000 * SCALE_FACTOR).is_ok());
    }
}
