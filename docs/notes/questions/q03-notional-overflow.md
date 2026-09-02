# Q3 - Notional overflow in the intermediate

- **Blocks:** [ADR 0001 - Core price & quantity types](../../decisions/0001-core-types.md)
- **Status:** Resolved.
- **Codified in:** `crates/core/src/notional_overflow_analysis.rs`.

## Question

What is the largest `Price * Qty` notional I could see in a single level, in
scaled units? Does the **intermediate** fit in `i64`? In `i128`? Overflow in
the intermediate before the divide is the classic fixed-point bug, and it is
why `PrecisionOverflow` is on the core error list.

## Approach

Mirror image of Q2. Q2 fixed the storage scale for `Price` and `Qty` on their
own. Q3 asks whether their **product**, before we shift back down, fits in
any storage integer we would actually use.

Method:

1. Attach a realistic per-level quantity to each pair from Q2's set. Meme
   coins carry trillions of tokens per level; stables carry hundreds of
   millions of dollars notional; BTC book levels rarely exceed low
   hundreds.
2. For each candidate scale (`N` = 12, 18) and each level, compute
   `price_scaled * qty_scaled` and check whether it fits in `i64`, `i128`,
   or `i256`.
3. Also stress the aggregation surface: consolidated depth sums notionals
   across levels and venues, so single-level fit is necessary but not
   sufficient.

Expected outcome: `i64` intermediate is dead on arrival at any scale we
care about; `i128` is the natural width for N=12; N=18 forces `i256`.

## Working notes

### Numbers at N=12

Six realistic top-of-book levels, worst-case quantities, `Price * Qty`
intermediate at scale `2N = 24`:

| Pair | Price | Qty | Intermediate | Fits |
| --- | --- | --- | --- | --- |
| BTC/USD | 100,000 | 100 | 1.0e31 | i128 |
| ETH/USD | 3,000 | 10,000 | 3.0e30 | i128 |
| SOL/USD | 100 | 100,000 | 1.0e31 | i128 |
| USDC/USDT | 1 | 100,000,000 | 1.0e32 | i128 |
| SHIB/USD | 1e-5 | 1e12 | 1.0e31 | i128 |
| PEPE/USD | 1e-6 | 1e13 | 1.0e31 | i128 |

`i64::MAX ~ 9.2e18`, `i128::MAX ~ 1.7e38`. Every level lands in the
`1e30 - 1e32` band. `i64` misses by 12-14 orders of magnitude; `i128`
sits with ~6-8 orders of headroom per level. USDC/USDT is the worst case
at N=12 because the tight scale meets the huge stablecoin quantities.

### Aggregation stress at N=12

Consolidated depth sums notionals across many levels. With five venues
and ~50-100 levels per side each, 500 aggregated levels is a plausible
upper bound. Even summing 500 copies of the worst single-level notional
(`1e32`) gives `5e34`, still ~3 orders of magnitude below `i128::MAX`.

That is enough. The aggregation surface does not force a wider
intermediate at N=12.

### Numbers at N=18

Same levels, scaled at `N = 18`:

- BTC single level: price_scaled = `1e23`, qty_scaled = `1e20`,
  intermediate = `1e43`. `i128::MAX ~ 1.7e38`. **Overflows by ~5 orders
  of magnitude.**
- Even ETH single level: `1e21 * 1e22 = 1e43`. Same result.

At N=18 the intermediate demands `i256` or an equivalent wide type. This
is the arithmetic-side symmetry to Q1 sub-part 3, which found the
squared-price step needs `U512` on the Uniswap side. Wider scale, wider
intermediate.

### Codified in tests

`crates/core/src/notional_overflow_analysis.rs` has six tests plus a
table printer. The five load-bearing tests:

- `i64_intermediate_overflows_at_n12` - rules out `i64` for the
  intermediate at Q2's sweet spot scale.
- `i128_intermediate_fits_every_level_at_n12` - the good news:
  every realistic level fits `i128` at N=12. Prints per-pair headroom
  in orders of magnitude.
- `i128_intermediate_overflows_at_n18_for_btc` - the bad news:
  N=18 with `i128` overflows even a single BTC level.
- `i256_intermediate_fits_every_level_at_n18` - the escape: widening
  to `i256` rescues N=18.
- `i128_survives_realistic_aggregation_at_n12` - 500 worst-case levels
  summed at N=12 still fit `i128`. Aggregation does not force a wider
  intermediate.

Plus `print_notional_intermediate_table` for eyeballing the full
trade-off with
`cargo test -p meridian-core notional_overflow_analysis -- --show-output`.

## Resolution

Two viable arithmetic layouts:

1. **N=12 storage in `i64`, intermediate in `i128`.** Single-level
   notionals sit at `1e30 - 1e32`, well inside `i128::MAX ~ 1.7e38`.
   Aggregation across ~500 realistic levels still leaves ~3 orders
   of headroom. This is Q2's "sweet spot" made arithmetically sound.
2. **N=18 storage in `i128`, intermediate in `i256`.** Buys the full
   precision Q1 sub-part 3 wanted, at the cost of a wide-integer
   dependency (`ruint`, `alloy_primitives::U256`, or hand-rolled).
   BTC single level at N=18 overflows `i128` by ~5 orders, so `i256`
   or wider is mandatory on the intermediate.

`i64` is ruled out as an intermediate at any scale we would actually use.

**What this pins down for ADR 0001.**

- Option A (fixed `N=12` in `i64`) survives Q3 with headroom, provided
  every product goes through an `i128` intermediate before the shift.
  `PrecisionOverflow` guards multiplication at the boundary.
- Option B (fixed `N=18` in `i128`) survives only with `i256` on the
  intermediate. Cost: wide-integer arithmetic on every product.
- Option C (per-symbol scale) inherits whichever intermediate width
  its per-symbol `N` implies, plus the cross-symbol comparison work
  Q2 flagged.
- Option D (`rust_decimal`) sidesteps this analysis by carrying its
  own scale, at the cost of Q5's hash question.

Q3 does not pick a winner. It removes the "obviously broken" branches
(`i64` intermediate, `i128` intermediate at N=18) and makes Option A's
arithmetic soundness concrete. ADR 0001 gets to weigh A through D once
Q4, Q5, Q6 land.
