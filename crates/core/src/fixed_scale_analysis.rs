//! Q2 verification: does one fixed decimal scale `N` work across Meridian's
//! full price range?
//!
//! See `docs/notes/questions/q02-low-price-pair-scale.md`.
//!
//! Uses `f64` arithmetic because this is analysis, not production code: we
//! are exploring what a fixed-scale `Price` would *look like* for each pair,
//! not building the actual type. The claims proved here are structural
//! (this scale overflows at the top, that scale under-resolves at the
//! bottom), not numerically exact.

struct Pair {
    name: &'static str,
    price: f64,
}

/// Representative pairs spanning the price range Meridian will see.
/// High: BTC. Mid: ETH, SOL, stablecoin. Low: SHIB, PEPE.
const PAIRS: &[Pair] = &[
    Pair { name: "BTC/USD", price: 100_000.0 },
    Pair { name: "ETH/USD", price: 3_000.0 },
    Pair { name: "SOL/USD", price: 100.0 },
    Pair { name: "USDC/USDT", price: 1.0 },
    Pair { name: "SHIB/USD", price: 0.00001 },
    Pair { name: "PEPE/USD", price: 0.000001 },
];

/// Candidate fixed scales to evaluate.
const SCALES: &[u32] = &[6, 8, 12, 18];

/// If one tick is more than this many basis points of price, the scale is
/// too coarse for the pair. 1 bp = 0.01% is a common granularity for
/// financial data.
const MAX_TICK_BPS: f64 = 1.0;

fn scaled_value(price: f64, n: u32) -> f64 {
    price * 10f64.powi(n as i32)
}

fn tick_bps(price: f64, n: u32) -> f64 {
    // One tick = 10^-N in real terms; expressed in basis points.
    (10f64.powi(-(n as i32)) / price) * 10_000.0
}

fn fits_i64(scaled: f64) -> bool {
    scaled.is_finite() && scaled >= 0.0 && scaled <= i64::MAX as f64
}

fn fits_i128(scaled: f64) -> bool {
    scaled.is_finite() && scaled >= 0.0 && scaled <= i128::MAX as f64
}

// At N=8, SHIB (~$10^-5) has a tick of 10 bps and PEPE (~$10^-6) has a tick
// of 100 bps. Both are far above the 1-bp threshold. This is the concrete
// low-end failure the question anticipated.
#[test]
fn n8_loses_meaningful_precision_on_shib_and_pepe() {
    let shib = PAIRS.iter().find(|p| p.name == "SHIB/USD").unwrap();
    let pepe = PAIRS.iter().find(|p| p.name == "PEPE/USD").unwrap();

    let shib_tick = tick_bps(shib.price, 8);
    let pepe_tick = tick_bps(pepe.price, 8);

    println!("N=8, SHIB/USD tick = {:.4} bps", shib_tick);
    println!("N=8, PEPE/USD tick = {:.4} bps", pepe_tick);

    // SHIB should be ~10 bps.
    assert!(shib_tick > 5.0 * MAX_TICK_BPS);
    // PEPE should be ~100 bps. Unusable for real market data.
    assert!(pepe_tick > 50.0 * MAX_TICK_BPS);
}

// Negative control: N=8 is comfortable at the top of the range. The failure
// above is not "N=8 is bad everywhere", it is "N=8 is bad specifically at
// the low end". Prevents the wrong lesson.
#[test]
fn n8_is_fine_for_btc_and_eth() {
    let btc = PAIRS.iter().find(|p| p.name == "BTC/USD").unwrap();
    let eth = PAIRS.iter().find(|p| p.name == "ETH/USD").unwrap();

    let btc_tick = tick_bps(btc.price, 8);
    let eth_tick = tick_bps(eth.price, 8);
    println!("N=8, BTC/USD tick = {:e} bps", btc_tick);
    println!("N=8, ETH/USD tick = {:e} bps", eth_tick);

    assert!(btc_tick < MAX_TICK_BPS);
    assert!(eth_tick < MAX_TICK_BPS);
    assert!(fits_i64(scaled_value(btc.price, 8)));
}

// N=18 (matching WETH decimals) buys precision at the bottom but pushes BTC
// out of `i64`. This is the concrete high-end failure symmetric to the
// low-end failure at N=8.
#[test]
fn n18_overflows_i64_for_btc() {
    let btc = PAIRS.iter().find(|p| p.name == "BTC/USD").unwrap();
    let scaled = scaled_value(btc.price, 18);
    println!("BTC/USD at N=18 scaled = {:e}", scaled);
    println!("i64::MAX               = {:e}", i64::MAX as f64);
    assert!(!fits_i64(scaled));
    assert!(fits_i128(scaled));
}

// For N in {6, 8}, at least one pair exceeds the 1-bp tick threshold on
// the low end. Locks the low-end failure in.
#[test]
fn small_n_fails_at_the_low_end() {
    for &n in &[6u32, 8] {
        let failures: Vec<&str> = PAIRS
            .iter()
            .filter(|p| tick_bps(p.price, n) > MAX_TICK_BPS)
            .map(|p| p.name)
            .collect();
        println!("N={:>2}: tick > 1 bp for {:?}", n, failures);
        assert!(!failures.is_empty(), "expected N={n} to under-resolve some pair");
    }
}

// For N >= 18, at least one pair overflows `i64`. Locks the high-end
// failure in.
#[test]
fn large_n_overflows_i64_at_the_high_end() {
    let overflows: Vec<&str> = PAIRS
        .iter()
        .filter(|p| !fits_i64(scaled_value(p.price, 18)))
        .map(|p| p.name)
        .collect();
    println!("N=18: i64 overflow for {:?}", overflows);
    assert!(!overflows.is_empty(), "expected N=18 to overflow i64 somewhere");
}

// N=12 threads the needle for the pairs above: every one fits `i64` and
// every one keeps tick below 1 bp. This is the honest finding, not
// "nothing works". The decision in ADR 0001 has to weigh this against the
// thin headroom below.
#[test]
fn n12_works_for_all_representative_pairs() {
    for p in PAIRS {
        let s = scaled_value(p.price, 12);
        let t = tick_bps(p.price, 12);
        println!("N=12, {:>10}: scaled = {:>18.4e}  tick = {:>10.6} bps", p.name, s, t);
        assert!(fits_i64(s), "{} overflows i64 at N=12", p.name);
        assert!(t < MAX_TICK_BPS, "{} tick exceeds 1 bp at N=12", p.name);
    }
}

// The margin below N=12 is thin. A hypothetical DEX pair at 1e-10 (real
// after some decimals adjustments on obscure Solana or Ethereum tokens)
// already breaks N=12. Any move to support tail-of-tail pairs re-opens
// this decision.
#[test]
fn n12_fails_for_hypothetical_extreme_low_pair() {
    let extreme_price = 1e-10_f64;
    let s = scaled_value(extreme_price, 12);
    let t = tick_bps(extreme_price, 12);
    println!("N=12, hypothetical 1e-10 pair: scaled = {:.4e}  tick = {:.4} bps", s, t);
    assert!(t > MAX_TICK_BPS);
}

// Prints the full trade-off table so a reader running `cargo test -- --nocapture`
// can see the shape of the problem, not just the pass/fail outcome.
#[test]
fn print_scale_precision_table() {
    assert!(!PAIRS.is_empty());
    println!();
    println!(
        "{:>10} {:>12} {:>4} {:>18} {:>18} {:>10}",
        "Pair", "Price", "N", "Scaled", "Tick (bps)", "Fits"
    );
    println!("{}", "-".repeat(78));
    for &n in SCALES {
        for p in PAIRS {
            let s = scaled_value(p.price, n);
            let t = tick_bps(p.price, n);
            let fits = if fits_i64(s) {
                "i64"
            } else if fits_i128(s) {
                "i128"
            } else {
                "overflow"
            };
            println!(
                "{:>10} {:>12.6} {:>4} {:>18.4e} {:>18.6} {:>10}",
                p.name, p.price, n, s, t, fits
            );
        }
        println!();
    }
}
