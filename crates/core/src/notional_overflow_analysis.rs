//! Q3 verification: for a chosen (scale, storage) combination, does the
//! `Price * Qty` intermediate fit *before* we shift back down?
//!
//! See `docs/notes/questions/q03-notional-overflow.md`.
//!
//! Uses `f64` arithmetic like Q2. This is analysis, not production code:
//! the claims are structural (this width overflows, that one fits), not
//! numerically exact.
struct Level {
    pair: &'static str,
    price: f64,
    qty: f64,
}

/// Worst-case single-level notionals across Meridian's expected pair range.
/// Quantities picked to be plausible for a top-of-book level on either a
/// CEX or a DEX pool: not the absolute maximum, but the realistic upper
/// end for that pair type.
const LEVELS: &[Level] = &[
    // High price, moderate qty (BTC book levels rarely hundreds).
    Level {pair: "BTC/USD", price: 100_000.0, qty: 100.0 },
    Level {pair: "ETH/USD", price: 3_000.0, qty: 10_000.0 },
    Level {pair: "SOL/USD", price: 100.0, qty: 100_000.0 },
    // Stable pairs: modest price, potentially huge qty.
    Level { pair: "SHIB/USD", price: 1e-5, qty: 1e12 },
    Level { pair: "PEPE/USD", price: 1e-6, qty: 1e13 },
];

/// Candidate fixed scales for `Price` and `Qty` (same scale on both
/// sides for this analysis; Q4 revisits whether they must match).
const SCALES: &[u32] = &[12, 18];
fn scaled(v:f64, n: u32) -> f64 {
    v * 10f64.powi(n as i32)
}

fn intermediate(price:f64, qty: f64, n: u32) -> f64 {
    scaled(price, n) * scaled(qty, n)
}

fn fits_i64(v: f64) -> bool {
    v.is_finite() && v >= 0.0 && v <= i64::MAX as f64
}

fn fits_i128(v: f64) -> bool {
    v.is_finite() && v >= 0.0 && v <= i128::MAX as f64
}

/// i256::MAX ~ 5.79e76 (2^255 - 1). Comparing an f64 to this bound is
/// exact enough for structural claims; the mantissa loses precision
/// past ~15 digits but the ordering with anything smaller than 10^76
/// still holds.
fn fits_i256(v: f64) -> bool {
    v.is_finite() && v >= 0.0 && v <= 5.789e76
}
// The clean disqualification. Any realistic price * qty at N=12 blows
// past i64::MAX. Rules out i64 as the intermediate width even for a
// scale we already know works for storage.
#[test]
fn i64_intermediate_overflows_at_n12() {
    let btc = LEVELS.iter().find(|l| l.pair == "BTC/USD").unwrap();
    let mid = intermediate(btc.price, btc.qty, 12);
    println!("BTC/USD @ N=12 intermediate = {:e}", mid);
    println!("i64::MAX                    = {:e}", i64::MAX as f64);
    assert!(!fits_i64(mid));
}

// The headline good-news finding: at N=12, an i128 intermediate holds
// every single-level notional across the full pair range with plenty of
// margin. This is what makes Q2's N=12 sweet spot arithmetically viable.
#[test]
fn i128_intermediate_fits_every_level_at_n12() {
    for l in LEVELS {
        let mid = intermediate(l.price, l.qty, 12);
        let headroom = (i128::MAX as f64).log10() - mid.log10();
        println!(
            "{:>10} @ N=12: intermediate = {:>10.4e}   headroom ~ {:.1} orders",
            l.pair, mid, headroom
        );
        assert!(fits_i128(mid), "{} overflows i128 at N=12", l.pair);
    }
}
// The bad-news finding for N=18: even a single BTC level overflows
// an i128 intermediate. Mirrors Q1 sub-part 3's Uniswap-side result
// that squared-price arithmetic needs U512 headroom.
#[test]
fn i128_intermediate_overflows_at_n18_for_btc() {
    let btc = LEVELS.iter().find(|l| l.pair == "BTC/USD").unwrap();
    let mid = intermediate(btc.price, btc.qty, 18);
    println!("BTC/USD @ N=18 intermediate = {:e}", mid);
    println!("i128::MAX                   = {:e}", i128::MAX as f64);
    assert!(!fits_i128(mid));
}

// The escape at N=18: widen to i256 for the intermediate and every
// realistic level fits comfortably.
#[test]
fn i256_intermediate_fits_every_level_at_n18() {
    for l in LEVELS {
        let mid = intermediate(l.price, l.qty, 18);
        println!("{:>10} @ N=18: intermediate = {:e}", l.pair, mid);
        assert!(fits_i256(mid), "{} would overflow i256 at N=18", l.pair);
    }
}
// Aggregation stress test at N=12/i128. Consolidated depth sums notionals
// across many levels and many venues. Even with 500 worst-case levels
// summed, the total sits well below i128::MAX.
#[test]
fn i128_survives_realistic_aggregation_at_n12() {
    const AGGREGATED_LEVELS: usize = 500;
    let worst = LEVELS
        .iter()
        .map(|l| intermediate(l.price, l.qty, 12))
        .fold(0.0f64, f64::max);
    let summed = worst * AGGREGATED_LEVELS as f64;
    let headroom = (i128::MAX as f64).log10() - summed.log10();
    println!("worst single-level    = {:e}", worst);
    println!("aggregated x {:>3}      = {:e}", AGGREGATED_LEVELS, summed);
    println!("i128::MAX             = {:e}", i128::MAX as f64);
    println!("aggregation headroom  ~ {:.1} orders of magnitude", headroom);
    assert!(fits_i128(summed));
}
// Prints the full trade-off table so a reader can see the shape of the
// problem, not just pass/fail. Passes trivially.
#[test]
fn print_notional_intermediate_table() {
    assert!(!LEVELS.is_empty());
    println!();
    println!(
        "{:>10} {:>10} {:>10} {:>4} {:>16} {:>8}",
        "Pair", "Price", "Qty", "N", "Intermediate", "Fits"
    );
    println!("{}", "-".repeat(72));
    for &n in SCALES {
        for l in LEVELS {
            let mid = intermediate(l.price, l.qty, n);
            let fits = if fits_i64(mid) {
                "i64"
            } else if fits_i128(mid) {
                "i128"
            } else if fits_i256(mid) {
                "i256"
            } else {
                "overflow"
            };
            println!(
                "{:>10} {:>10.4e} {:>10.4e} {:>4} {:>16.4e} {:>8}",
                l.pair, l.price, l.qty, n, mid, fits
            );
        }
        println!();
    }
}