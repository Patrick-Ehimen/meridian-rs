# 0001 - Core price & quantity types

- **Status:** Accepted
- **Date:** 2026-09-02
- **Affects:** `crates/core`

## Context

Every crate downstream speaks these types: adapters parse into them, `Book`
is keyed by them, the aggregator sums notionals in them, the servers hand
them to consumers. Six questions had to answer before I could pick a
representation. All six now resolved and codified as tests; the numbers are
what force the choice.

**Precision floor.** Uniswap V3 stores prices as `sqrtPriceX96` in Q64.96
fixed point. That is 96 binary fractional bits, ~29 decimal digits of
resolution; the real price range after squaring reaches ~57.8 decimal
digits. Real values reach ~49 decimal digits, exceeding `u128::MAX` by
~10 digits. See [Q1](../notes/questions/q01-q64-96-precision.md).

That precision must be reachable at the **adapter boundary**. The
downstream `Price` type does not need to carry all of it, because
consolidated depth is explicitly an analytical view, not tradeable
liquidity (CLAUDE.md invariant).

**Price range Meridian sees.** BTC at ~$100k on the high end, PEPE at
~$1e-6 on the low end, with realistic per-level quantities running from
0.001 BTC to ~10^13 PEPE tokens. See
[Q2](../notes/questions/q02-low-price-pair-scale.md).

**Arithmetic headroom.** Every `Price * Qty` produces an intermediate at
double the scale. That intermediate is where the classic fixed-point
overflow lives. See [Q3](../notes/questions/q03-notional-overflow.md).

**Qty precision cost of a uniform scale.** WETH is 18 native decimals,
USDC 6, WBTC 8. Storing every Qty at one global `N` truncates high-decimal
tokens; the truncation cost in USD-equivalent is bounded and comparable
across the eight tokens Meridian actually sees. See
[Q4](../notes/questions/q04-shared-scale.md).

**Decimal library correctness.** `rust_decimal` was historically broken as
a `HashMap` key (Hash inconsistent with PartialEq). `rust_decimal >= 1.43`
fixes this. See [Q5](../notes/questions/q05-rust-decimal-hash.md).

**Symbol identifier space.** Five venues, three identifier schemes: two
concatenated-string CEX conventions (Binance, Bybit), one hyphenated CEX
(Coinbase), 20-byte Ethereum pool addresses (Uniswap V3), and 32-byte
Solana mint pairs (Jupiter). See
[Q6](../notes/questions/q06-symbol-shape.md).

## Options considered

### A1 - Fixed scale N=12, `i64` storage, `i128` intermediate

`Price(i64)` and `Qty(i64)` store `real_value * 10^12`. Every `Price * Qty`
widens to `i128` before the shift back. `Symbol { base, quote }` with
`Ticker = [u8; 12]`.

Cost:

- `i64` storage per Price and Qty (8 bytes each).
- `i128` for intermediates; native x86-64 does 128-bit multiply in a single
  instruction pair.
- On 18-decimal DEX tokens (WETH, PEPE, SHIB), sub-10^-12 real units are
  truncated at adapter ingest. USD-equivalent loss per conversion is
  bounded at ~3e-9 (WETH) or lower. Below every exchange minimum trade
  size by orders of magnitude. Q4 tests prove it.
- Works across pairs down to ~$10^-9. Below that, either per-symbol scale
  or a switch to A2 / D.

### A2 - Fixed scale N=18, `i128` storage, `i256` intermediate

`Price(i128)` and `Qty(i128)` store `real_value * 10^18`. Every product
widens to `i256` (via `alloy::primitives::U256` or equivalent) before the
shift.

Cost:

- 16 bytes per Price and Qty; doubled storage cost across every `Book`
  entry, `DashMap` value, and gRPC/WebSocket response.
- Wide-integer arithmetic dependency for every product (not just the
  `uniswap-v3` adapter).
- Preserves full atomic fidelity for every token in the current set. Buys
  nothing that consolidated depth, VWAP, OFI, or arbitrage detection
  actually consume.

### D - `rust_decimal::Decimal`

`Price(Decimal)` and `Qty(Decimal)` carry their own scale internally. No
fixed `N` to defend.

Cost:

- 16 bytes per value.
- Every arithmetic op branches on scale normalisation. Benchmarks
  elsewhere in the Rust ecosystem put decimal ops at ~5-10x the cost of
  equivalent integer ops.
- Adds `rust_decimal` as a first-class runtime dependency in `crates/core`;
  currently only a `dev-dependency` from Q5's verification.
- Sidesteps the fixed-scale question entirely, which is real value if a
  pair below `$1e-9` ever appears.

## Decision

I store prices and quantities as fixed-point integers at scale `N=12` in
`i64`, with an `i128` intermediate for products. Concretely, in
`crates/core`:

```rust
pub const SCALE: u32 = 12;
pub const SCALE_FACTOR: i64 = 10i64.pow(SCALE);

pub struct Price(pub i64);
pub struct Qty(pub i64);

pub struct Symbol {
    pub base:  Ticker,
    pub quote: Ticker,
}
pub type Ticker = [u8; 12];
```

Every `Price * Qty` goes through an `i128` intermediate. A helper on
`Price` returns `Result<Notional, PrecisionOverflow>`, using `checked_mul`
so overflow is a typed error rather than a wraparound.

## Why I rejected the others

- **A2** pays for `i128` storage and `i256` intermediate arithmetic across
  every price and quantity in Meridian, in exchange for atomic-unit
  fidelity that nothing downstream consumes. The invariant "consolidated
  depth is analytical, not tradeable" (CLAUDE.md) rules out the one use
  case that would justify the cost. Rejected on cost/benefit, not on
  correctness.
- **D** loses A1's speed advantage without buying anything the analysis
  needs. `rust_decimal` also promotes to a first-class runtime dep and
  requires a pinned floor (>= 1.43) to avoid the Q5 hash bug. Kept viable
  as a **fallback**: if A1 hits a real precision or range problem in
  practice, D is the drop-in replacement.
- **Per-token Qty scale** (from Q4) forced either overflow-on-`i64`,
  wasted-headroom-on-`i128`, or a scale-carrying `Qty` type. All three
  costs, no benefit downstream. Rejected on the same grounds as A2.
- **Interned `Symbol(u32)`** (from Q6) saves 20 bytes per Symbol at the
  cost of a global registry with cross-thread synchronisation, plus
  opaque `Debug` output. At Meridian's scale (a few hundred symbols)
  the savings are noise.
- **Enum-over-venue-identifiers `Symbol`** (from Q6) kills cross-venue
  key equality, which is the entire point of consolidated depth.
  Structural disqualification.

## Consequences

**What this makes easy.**

- Integer arithmetic everywhere. No dependencies on `rust_decimal`,
  `alloy::primitives::U256`, or any wide-integer type in the hot path.
  `alloy` stays confined to the Uniswap adapter.
- `Price`, `Qty`, `Symbol` are all `Copy`. They flow freely across
  adapter tasks, `DashMap` entries, gRPC/REST/WebSocket response types.
  No borrows, no lifetimes.
- Derived `Hash`/`Ord` for `Symbol` are trivially correct (byte-array
  comparison); no Q5-style contract violation.
- Cross-venue consolidated depth works: BTCUSDT on Binance and Bybit
  produce identical `Symbol` values, and `HashMap<Symbol, _>` behaves.

**What this makes hard.**

- The adapter boundary owns two conversion responsibilities. DEX adapters
  must divide raw atomic units by `10^(native_decimals - 12)` for tokens
  with more than 12 decimals; the truncation is documented, not silent.
  CEX adapters must parse decimal strings directly into scaled `i64`
  (never through `f64`).
- Every `Price * Qty` must go through the checked-multiply helper.
  Freehand multiplication of two `i64`s can wraparound. A clippy lint
  or a wrapper method (no public `Mul<Qty> for Price`) enforces this.
- Tickers longer than 12 bytes have to be rejected at ingest with a
  clear error, or `MAX_TICKER_LEN` bumped and the codebase rebuilt.
- DEX-native identifiers do not fit `Symbol`. Each adapter owns its own
  `HashMap<VenueSymbol, Symbol>` mapping, populated at startup from
  venue metadata (token registries, `slot0` reads, mint decimals).

**Tests that pin the invariant.** All under `crates/core/src/` behind
`#[cfg(test)]`:

- `fixed_scale_analysis.rs` - N=12 is the narrowest working scale in
  `i64` given current pair coverage; extreme low-priced pairs (below
  `1e-9`) break it.
- `notional_overflow_analysis.rs` - `i128` intermediate handles every
  realistic level with 6-8 orders of headroom; aggregating 500 levels
  still fits.
- `qty_scale_analysis.rs` - Qty truncation at N=12 sits below the $0.01
  dust threshold for every token in the set.
- `rust_decimal_hash_check.rs` - documents the D-option safety floor
  in case we ever fall back.
- `symbol_shape_analysis.rs` - representative tickers fit `[u8; 12]`;
  DEX identifiers do not, forcing the two-layer design.

Plus `crates/venues/uniswap-v3/src/lib.rs` for the venue-boundary
precision floor: `SQRT_PRICE_SHIFT`, `Q96`, `MIN_SQRT_RATIO`,
`MAX_SQRT_RATIO`, `PRICE_SHIFT`, `Q192`, with 14 tests.

## I should revisit this if

- **Smart order routing lands.** SOR needs wei-exact quantities.
  Uniform N=12 truncates below the atomic unit for 18-decimal tokens.
  Move to A2, or thread per-token scale through `Qty`.
- **A pair below `$1e-9` needs to be supported.** N=12 in `i64` gives
  less than 1 bp of tick resolution at that price. Per-symbol scale
  or option D.
- **A benchmark shows A1's integer speed is not actually the win.**
  Unlikely on x86-64 (native 128-bit multiply), but worth a real
  measurement once ingestion is producing traffic.
- **A new ticker exceeds 12 bytes.** Bump `MAX_TICKER_LEN` and
  rebuild; the `symbol_shape_analysis` regression test fails loudly
  when this happens.
- **A precision bug shows up.** The `PrecisionOverflow` error path
  is the canary; if it fires in production traffic on realistic
  input, either the pair set has grown beyond N=12's envelope or an
  adapter is producing pathological values.
