//! Nanosecond wall-clock timestamps.
//!
//! Wrapped in a newtype so the backing can change later without touching every
//! crate. Every `Book` carries two of these: `ts_exchange` from the venue
//! payload and `ts_local` from when the adapter received it. Without both,
//! there is no way to tell afterwards whether we lagged or the venue did.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Nanoseconds since the Unix epoch, UTC.
///
/// `u64` nanoseconds runs out in the year 2554.
#[repr(transparent)]
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Timestamp(u64);

impl Timestamp {
    /// The Unix epoch. Doubles as "not yet known" for an empty book.
    pub const EPOCH: Self = Self(0);

    pub const fn from_unix_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    /// Converts a venue's millisecond timestamp (Binance, Bybit).
    ///
    /// Saturates rather than wrapping on absurd input, so a corrupt payload
    /// yields an obviously wrong time instead of a plausible one.
    pub const fn from_unix_millis(millis: u64) -> Self {
        Self(millis.saturating_mul(1_000_000))
    }

    /// Converts a venue's microsecond timestamp (Coinbase).
    pub const fn from_unix_micros(micros: u64) -> Self {
        Self(micros.saturating_mul(1_000))
    }

    pub const fn as_unix_nanos(self) -> u64 {
        self.0
    }

    /// The current wall-clock time.
    ///
    /// A clock set before 1970 yields [`Timestamp::EPOCH`] rather than a
    /// panic, keeping the no-panic rule for library code.
    pub fn now() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        Self(nanos)
    }

    /// `self - earlier` in nanoseconds, or `None` if `earlier` is later.
    ///
    /// `ts_local.nanos_since(ts_exchange)` is the ingest latency. A `None`
    /// there means the venue clock is ahead of ours: clock skew, not a bug.
    pub const fn nanos_since(self, earlier: Self) -> Option<u64> {
        self.0.checked_sub(earlier.0)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secs = self.0 / 1_000_000_000;
        let nanos = self.0 % 1_000_000_000;
        write!(f, "{secs}.{nanos:09}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_conversions_land_in_nanoseconds() {
        assert_eq!(Timestamp::from_unix_millis(1).as_unix_nanos(), 1_000_000);
        assert_eq!(Timestamp::from_unix_micros(1).as_unix_nanos(), 1_000);
    }

    #[test]
    fn absurd_millis_saturate_instead_of_wrapping() {
        assert_eq!(
            Timestamp::from_unix_millis(u64::MAX).as_unix_nanos(),
            u64::MAX
        );
    }

    #[test]
    fn now_is_after_this_test_was_written() {
        // 2026-01-01T00:00:00Z
        assert!(Timestamp::now() > Timestamp::from_unix_millis(1_767_225_600_000));
    }

    #[test]
    fn latency_is_none_under_clock_skew() {
        let venue = Timestamp::from_unix_nanos(1_000);
        let local = Timestamp::from_unix_nanos(1_500);
        assert_eq!(local.nanos_since(venue), Some(500));
        assert_eq!(venue.nanos_since(local), None);
    }

    #[test]
    fn display_is_seconds_with_nine_fractional_digits() {
        assert_eq!(
            Timestamp::from_unix_nanos(1_700_000_000_000_000_042).to_string(),
            "1700000000.000000042"
        );
    }
}
