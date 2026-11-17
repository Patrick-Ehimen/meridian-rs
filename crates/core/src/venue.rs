//! The venues Meridian ingests from.
//!
//! This lives in core, not in the adapter crates, so the aggregator can tell
//! which book came from where without importing every adapter.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// A trading venue.
///
/// `Display` and `FromStr` round-trip through the same lowercase names, which
/// is what logs, CLI config and metric labels all use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Venue {
    Binance,
    Coinbase,
    Bybit,
    UniswapV3,
    Jupiter,
}

impl Venue {
    /// Every venue, in declaration order. Useful for config validation and
    /// for iterating per-venue state in a stable order.
    pub const ALL: [Self; 5] = [
        Self::Binance,
        Self::Coinbase,
        Self::Bybit,
        Self::UniswapV3,
        Self::Jupiter,
    ];

    /// The canonical lowercase name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Binance => "binance",
            Self::Coinbase => "coinbase",
            Self::Bybit => "bybit",
            Self::UniswapV3 => "uniswap_v3",
            Self::Jupiter => "jupiter",
        }
    }

    /// True for on-chain venues, whose depth is derived rather than read.
    pub const fn is_dex(self) -> bool {
        matches!(self, Self::UniswapV3 | Self::Jupiter)
    }
}

impl fmt::Display for Venue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Venue {
    type Err = CoreError;

    /// Parses the canonical name, case-insensitively.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownVenue`] for anything else.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str().eq_ignore_ascii_case(s))
            .ok_or_else(|| CoreError::UnknownVenue(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_from_str_round_trip() {
        for venue in Venue::ALL {
            assert_eq!(venue.to_string().parse::<Venue>(), Ok(venue));
        }
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert_eq!("Binance".parse::<Venue>(), Ok(Venue::Binance));
        assert_eq!("UNISWAP_V3".parse::<Venue>(), Ok(Venue::UniswapV3));
    }

    #[test]
    fn unknown_venue_is_a_typed_error() {
        assert_eq!(
            "kraken".parse::<Venue>(),
            Err(CoreError::UnknownVenue("kraken".to_owned()))
        );
    }

    #[test]
    fn serde_uses_the_same_names_as_display() {
        let json = serde_json::to_string(&Venue::UniswapV3).expect("serializes");
        assert_eq!(json, "\"uniswap_v3\"");
    }

    #[test]
    fn only_on_chain_venues_are_dex() {
        let dex: Vec<_> = Venue::ALL.into_iter().filter(|v| v.is_dex()).collect();
        assert_eq!(dex, [Venue::UniswapV3, Venue::Jupiter]);
    }
}
