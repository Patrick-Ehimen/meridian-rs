//! Q6 verification: what shape does `Symbol` take?
//!
//! See `docs/notes/questions/q06-symbol-shape.md`.
//!
//! Design analysis, not production code. The types defined here are
//! throw-away placeholders that prove the structural claims: fixed-size
//! canonical `Symbol` fits every CEX pair, `Copy` and small; DEX-native
//! identifiers do not fit and must live in per-adapter mappings.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem::size_of;

/// Fixed size for a ticker string. Sized to comfortably hold the
/// longest CEX-observed token symbols including prefix-multiplied
/// listings like `1000PEPE` / `1000BONK`.
const MAX_TICKER_LEN: usize = 12;

/// Fixed-size ticker. Padded with `0x00` for short strings.
type Ticker = [u8; MAX_TICKER_LEN];

/// Canonical Symbol: base + quote as fixed-size tickers. This is the
/// candidate shape for `crates/core::Symbol`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
struct CanonicalSymbol {
    base: Ticker,
    quote: Ticker,
}

fn ticker(bytes: &[u8]) -> Ticker {
    assert!(bytes.len() <= MAX_TICKER_LEN, "ticker too long");
    let mut t = [0u8; MAX_TICKER_LEN];
    t[..bytes.len()].copy_from_slice(bytes);
    t
}

// Every representative ticker Meridian expects to see fits in
// MAX_TICKER_LEN bytes, including the 1000-prefix meme listings.
// Regression guard if a new listing pushes past 12 chars.
#[test]
fn representative_tickers_fit_max_ticker_len() {
    let representative = [
        "BTC", "ETH", "SOL", "USDT", "USDC", "WBTC", "WETH", "SHIB", "PEPE", "BONK", "1000PEPE",
        "1000BONK", "MATIC", "AVAX", "DOGE", "TRUMP", "WEETH",
    ];
    for t in representative {
        println!("{:>10} : {} bytes", t, t.len());
        assert!(
            t.len() <= MAX_TICKER_LEN,
            "ticker {t:?} ({} bytes) does not fit MAX_TICKER_LEN={}",
            t.len(),
            MAX_TICKER_LEN,
        );
    }
}

// CanonicalSymbol is Copy: cheap to pass across threads and store
// in DashMap without borrowing. Size is a small multiple of a word,
// well under a cache line.
#[test]
fn canonical_symbol_is_copy_and_small() {
    let btcusdt = CanonicalSymbol {
        base: ticker(b"BTC"),
        quote: ticker(b"USDT"),
    };
    let clone = btcusdt;
    assert_eq!(btcusdt, clone);

    let s = size_of::<CanonicalSymbol>();
    println!("size_of::<CanonicalSymbol> = {} bytes", s);
    assert_eq!(s, 2 * MAX_TICKER_LEN);
    assert!(s < 64, "should fit inside one cache line");
}

// Derived PartialEq and Hash are consistent (byte-array comparison).
// No scale-normalisation trap like Q5's rust_decimal question.
#[test]
fn identically_spelled_symbols_are_equal_and_hash_equal() {
    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    let a = CanonicalSymbol {
        base: ticker(b"BTC"),
        quote: ticker(b"USDT"),
    };
    let b = CanonicalSymbol {
        base: ticker(b"BTC"),
        quote: ticker(b"USDT"),
    };
    assert_eq!(a, b);
    assert_eq!(hash_of(&a), hash_of(&b));
}

// Different pairs sort deterministically. Useful when the aggregator
// wants stable output ordering (BTreeMap keyed on Symbol).
#[test]
fn symbols_have_total_order() {
    let s1 = CanonicalSymbol {
        base: ticker(b"BTC"),
        quote: ticker(b"USDT"),
    };
    let s2 = CanonicalSymbol {
        base: ticker(b"ETH"),
        quote: ticker(b"USDT"),
    };
    let s3 = CanonicalSymbol {
        base: ticker(b"BTC"),
        quote: ticker(b"USDC"),
    };

    let mut sorted = vec![s2, s1, s3];
    sorted.sort();
    println!("sorted = {:?}", sorted);
    // Byte-lex order: BTC/USDC (43 54 43 / 55 53 44) < BTC/USDT < ETH/USDT
    assert_eq!(sorted, vec![s3, s1, s2]);
}

// Ethereum pool address (20 bytes) does NOT fit a Ticker. Forces
// the two-layer design: pool addresses live in adapter-internal
// mappings, not in the canonical Symbol.
#[test]
fn ethereum_pool_address_does_not_fit_ticker() {
    const ETH_ADDRESS_LEN: usize = 20;
    // Compile-time assertion so the invariant fails the build if the
    // ticker size is ever bumped past 20.
    const _: () = assert!(ETH_ADDRESS_LEN > MAX_TICKER_LEN);
    println!(
        "Ethereum address = {} bytes, Ticker = {} bytes",
        ETH_ADDRESS_LEN, MAX_TICKER_LEN
    );
}

// Same story for Solana: 32-byte pubkeys, 64 bytes for a pair.
// Larger than the entire canonical Symbol. Must live outside.
#[test]
fn solana_mint_pair_does_not_fit_canonical_symbol() {
    const SOLANA_PUBKEY_LEN: usize = 32;
    let sym_size = size_of::<CanonicalSymbol>();
    let pair_size = 2 * SOLANA_PUBKEY_LEN;
    println!(
        "Solana mint pair = {} bytes, CanonicalSymbol = {} bytes",
        pair_size, sym_size
    );
    assert!(pair_size > sym_size);
}

// Prints the size trade-off across the three options considered.
// Passes trivially; the point is the output.
#[test]
fn print_symbol_option_sizes() {
    println!();
    println!("Storage cost per Symbol, three options:");
    println!(
        "  A. CanonicalSymbol (base+quote Ticker)   = {} bytes  Copy",
        size_of::<CanonicalSymbol>()
    );
    println!(
        "  B. Interned u32                          = {} bytes  needs registry",
        size_of::<u32>()
    );
    println!("  C. Enum over venue-native identifiers    = variable  no cross-venue equality");
    println!();
    println!("Meridian scale: a few hundred symbols across all venues.");
    println!("At this scale, 24 bytes Copy beats 4 bytes with global synchronisation.");
}
