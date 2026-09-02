//! Q4 verification: should `Price` and `Qty` carry the same scale, or
//! should `Qty` inherit the token's on-chain decimals?
//!
//! See `docs/notes/questions/q04-shared-scale.md`.
//!
//! Uses `f64` for structural claims. This is analysis, not production
//! code: we are proving overflows and bounds, not doing exact
//! arithmetic.

struct Token {
    symbol: &'static str,
    /// On-chain (DEX) or matched-token (CEX) decimal count.
    decimals: u32,
    /// Representative USD price for expressing per-unit truncation
    /// as a dollar amount. Order-of-magnitude only.
    usd_price: f64,
}

const TOKENS: &[Token] = &[
    Token { symbol: "USDC", decimals: 6,  usd_price: 1.0 },
    Token { symbol: "USDT", decimals: 6,  usd_price: 1.0 },
    Token { symbol: "WBTC", decimals: 8,  usd_price: 100_000.0 },
    Token { symbol: "BONK", decimals: 5,  usd_price: 2e-8 },
    Token { symbol: "SOL",  decimals: 9,  usd_price: 100.0 },
    Token { symbol: "WETH", decimals: 18, usd_price: 3_000.0 },
    Token { symbol: "SHIB", decimals: 18, usd_price: 1e-5 },
    Token { symbol: "PEPE", decimals: 18, usd_price: 1e-6 },
];

const CANDIDATE_SCALES: &[u32] = &[12, 18];

/// Dust threshold: any truncation whose USD-equivalent is below this
/// value is structurally invisible. $0.01 is a common floor.
const DUST_USD: f64 = 0.01;

/// Upper bound on per-conversion truncation in the token's real units
/// when storing at scale `storage_n`. If `storage_n >= token_decimals`
/// the conversion pads with zeros and loses nothing; otherwise the
/// smallest storable real value is `10^-storage_n`, and everything
/// below is discarded.
fn truncation_real(token_decimals: u32, storage_n: u32) -> f64 {
    if storage_n >= token_decimals {
        0.0
    } else {
        10f64.powi(-(storage_n as i32))
    }
}

fn truncation_usd(token: &Token, storage_n: u32) -> f64 {
    truncation_real(token.decimals, storage_n) * token.usd_price
}

/// Worst-case scaled-integer magnitude for a Qty stored at the token's
/// own native decimals (Option B, per-token scale). Used to show that
/// uniform storage width cannot accommodate per-token scale.
fn native_scaled_max(token_decimals: u32, whole_units_worst_case: f64) -> f64 {
    whole_units_worst_case * 10f64.powi(token_decimals as i32)
}

fn fits_i64(v: f64) -> bool {
    v.is_finite() && v >= 0.0 && v <= i64::MAX as f64
}

fn fits_i128(v: f64) -> bool {
    v.is_finite() && v >= 0.0 && v <= i128::MAX as f64
}

// Headline finding for Option A: at N=12, every token in the current
// set has a per-conversion truncation bounded well below the $0.01
// dust threshold. Uniform scale is arithmetically safe.
#[test]
fn n12_truncation_is_below_dust_for_every_token() {
    for t in TOKENS {
        let lost_real = truncation_real(t.decimals, 12);
        let lost_usd = truncation_usd(t, 12);
        println!(
            "{:>4}: decimals={:>2}  trunc = {:>10.4e} tok  = {:>10.4e} USD  {}",
            t.symbol,
            t.decimals,
            lost_real,
            lost_usd,
            if lost_usd < DUST_USD { "OK" } else { "OVER" }
        );
        assert!(
            lost_usd < DUST_USD,
            "{} truncation at N=12 exceeds dust threshold",
            t.symbol
        );
    }
}

// N=18 pads every token in the set. Confirms that N=18 preserves
// full atomic fidelity as an upper-bound option (relevant if we ever
// need exact SOR quantities).
#[test]
fn n18_pads_every_token_without_loss() {
    for t in TOKENS {
        let lost = truncation_real(t.decimals, 18);
        println!("{:>4}: decimals={:>2}  trunc@N=18 = {}", t.symbol, t.decimals, lost);
        assert_eq!(lost, 0.0);
    }
}

// The specific WETH claim: at N=12, per-conversion truncation is at
// most 10^-12 ETH, or roughly 3 * 10^-9 USD at $3k/ETH. Well below
// Binance's 0.0001 ETH minimum trade and the $0.01 dust threshold.
#[test]
fn weth_truncation_at_n12_stays_far_below_exchange_minimums() {
    let weth = TOKENS.iter().find(|t| t.symbol == "WETH").unwrap();
    let lost_real = truncation_real(weth.decimals, 12);
    let lost_usd = truncation_usd(weth, 12);
    println!("WETH per-conversion loss = {:e} ETH = {:e} USD", lost_real, lost_usd);
    // Binance ETH minimum: 0.0001 ETH = 1e-4. Truncation is 1e-12,
    // eight orders of magnitude below the minimum.
    assert!(lost_real < 1e-4 / 1e6);
    assert!(lost_usd < DUST_USD);
}

// Option B in i64 storage: WETH at native 18 decimals overflows i64
// for realistic pool sizes. This makes "one Qty type at uniform
// storage width" impossible under per-token scale.
#[test]
fn per_token_scale_in_i64_overflows_for_weth() {
    let weth = TOKENS.iter().find(|t| t.symbol == "WETH").unwrap();
    let scaled = native_scaled_max(weth.decimals, 1000.0);
    println!("1000 WETH at native decimals = {:e}", scaled);
    println!("i64::MAX                     = {:e}", i64::MAX as f64);
    assert!(!fits_i64(scaled));
}

// Same case in i128: fits. Option B is viable only if every Qty pays
// for i128 storage, even pairs (USDC/USDT, WBTC) that would fit i64.
#[test]
fn per_token_scale_in_i128_fits_weth() {
    let weth = TOKENS.iter().find(|t| t.symbol == "WETH").unwrap();
    let scaled = native_scaled_max(weth.decimals, 1000.0);
    println!("1000 WETH at native decimals = {:e}", scaled);
    println!("i128::MAX                    = {:e}", i128::MAX as f64);
    assert!(fits_i128(scaled));
}

// The flip side: USDC at its native 6 decimals fits i64 with room to
// spare, even for $1B notionals. If we pay for i128 across the board
// (Option B), USDC pays for headroom it does not use.
#[test]
fn per_token_scale_usdc_fits_i64_at_billion_dollar_notional() {
    let usdc = TOKENS.iter().find(|t| t.symbol == "USDC").unwrap();
    let scaled = native_scaled_max(usdc.decimals, 1e9);
    println!("$1B in USDC at native decimals = {:e}", scaled);
    println!("i64::MAX                       = {:e}", i64::MAX as f64);
    assert!(fits_i64(scaled));
}

// Full trade-off table. Passes trivially; the point is the output.
// Run with `cargo test -p meridian-core qty_scale_analysis -- --show-output`.
#[test]
fn print_qty_scale_table() {
    assert!(!TOKENS.is_empty());
    println!();
    println!(
        "{:>4} {:>9} {:>12} {:>4} {:>14} {:>14}",
        "Tok", "Decimals", "USD price", "N", "Trunc(real)", "Trunc(USD)"
    );
    println!("{}", "-".repeat(66));
    for &n in CANDIDATE_SCALES {
        for t in TOKENS {
            let lost_real = truncation_real(t.decimals, n);
            let lost_usd = truncation_usd(t, n);
            println!(
                "{:>4} {:>9} {:>12.4e} {:>4} {:>14.4e} {:>14.4e}",
                t.symbol, t.decimals, t.usd_price, n, lost_real, lost_usd
            );
        }
        println!();
    }
}
