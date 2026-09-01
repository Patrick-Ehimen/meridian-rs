# Q2 - Low-price pair under a fixed 8-decimal scale

- **Blocks:** [ADR 0001 - Core price & quantity types](../../decisions/0001-core-types.md)
- **Status:** Resolved.
- **Codified in:** `crates/core/src/fixed_scale_analysis.rs`.

## Question

Take the lowest-priced pair I intend to support, something quoted around
`0.00001`, and work out what a fixed 8-decimal scale does to it. How many
significant figures survive? Is one tick 0.01% of price, or 8%?

## Approach

Q1 answered "how much precision does Uniswap V3 carry into the adapter".
This question is the mirror image: **how much precision does the internal
`Price` type actually need to hold?**

A single fixed decimal scale (a global `N` such that every `Price` is stored
as an integer of `real_price * 10^N`) is the simplest possible
representation. It is fast, `Ord` is trivially correct, arithmetic uses
integer ops, no allocation. The question is whether one `N` can span the
full range of pairs Meridian will see without either overflowing at the top
or losing meaningful resolution at the bottom.

Method:

1. Pick a set of representative pairs across the price range Meridian will
   actually see. High: BTC/USD (~10^5). Middle: ETH/USD (~10^3), SOL/USD
   (~10^2), USDC/USDT (~1). Low: SHIB/USD (~10^-5), PEPE/USD (~10^-6).
2. For a set of candidate scales (`N` = 6, 8, 12, 18), compute for each pair:
    - The **scaled integer** used to store price 1.0 of that magnitude.
    - The **tick size** (`10^-N`) as a percentage of price.
    - **Effective significant figures** (roughly `log10(scaled_integer)`).
3. Read across the resulting table: does any single `N` keep tick size below
   some reasonable threshold (say 0.01% = 1 bp) for **every** row, without
   overflowing `i64` or `i128` at the top?

Expected outcome: no single fixed scale works cleanly across the range. That
result points toward one of:

- Per-symbol scale (each pair gets its own `N`, stored alongside the pair).
- A decimal type that carries its own scale internally
  (`rust_decimal::Decimal`).
- Rational storage (numerator/denominator), rejected upfront on perf grounds.

The choice among those is ADR 0001's job; the point of Q2 is to prove the
"one fixed scale" option does not survive contact with real pairs.

## Working notes

### The trade-off in numbers

Six representative pairs across the range Meridian will see, with the tick
size a fixed decimal scale `N` implies for each. Tick threshold set at
1 bp (0.01% of price). Storage row shows the smallest signed integer that
holds the scaled value.

Direct answer to the original question first: at `N = 8`, for a pair around
`0.00001` (SHIB-class), **one tick is 10 bps (0.1%) of price**, not 0.01%
and not 8%. For PEPE-class at `0.000001`, one tick is 100 bps (1%). Both
carry only 3-4 significant figures of the scaled integer.

Full table (from `print_scale_precision_table`):

| Pair | Price | N=6 tick | N=8 tick | N=12 tick | N=18 tick |
| --- | --- | --- | --- | --- | --- |
| BTC/USD | 100,000 | ~0 | ~0 | ~0 | ~0 (needs i128) |
| ETH/USD | 3,000 | ~0 | ~0 | ~0 | ~0 (needs i128) |
| SOL/USD | 100 | 0.0001 bp | ~0 | ~0 | ~0 (needs i128) |
| USDC/USDT | 1 | 0.01 bp | ~0 | ~0 | ~0 |
| SHIB/USD | 1e-5 | **1000 bp** | **10 bp** | 0.001 bp | ~0 |
| PEPE/USD | 1e-6 | **10000 bp** | **100 bp** | 0.01 bp | ~0 |

Reading the columns:

- **N=6**: fine at the top (all fit `i64`), catastrophic below `$0.01`.
- **N=8**: same shape, just shifted one order of magnitude lower. Fails
  from SHIB-class down.
- **N=12**: threads the needle for every pair above. All fit `i64`, all
  tick under 1 bp.
- **N=18**: comfortable at the low end but forces `i128` from `SOL/USD`
  upward. BTC/USD scales to `10^23`, well past `i64::MAX ~= 9.2 * 10^18`.

### Where N=12 breaks

A hypothetical DEX pair at price `1e-10` (plausible after some token-decimals
adjustments on obscure tokens) at N=12 has:

- Scaled integer: 100
- Tick: 100 bps (1%)
- Effectively 2 significant figures

That is the ceiling of what N=12 covers. Any move to support such tail
pairs re-opens the decision.

### Codified in tests

`crates/core/src/fixed_scale_analysis.rs` holds seven tests plus a table
printer. The five load-bearing tests:

- `n8_loses_meaningful_precision_on_shib_and_pepe` - locks the low-end
  failure at N=8.
- `n8_is_fine_for_btc_and_eth` - negative control, prevents the wrong
  lesson ("N=8 is bad everywhere").
- `n18_overflows_i64_for_btc` - locks the high-end failure at N=18.
- `n12_works_for_all_representative_pairs` - documents that N=12 is the
  narrowest working scale for `i64` given current pair coverage.
- `n12_fails_for_hypothetical_extreme_low_pair` - documents the thin
  margin: 1e-10 already breaks it.

Plus `print_scale_precision_table` for eyeballing the full trade-off with
`cargo test -p meridian-core -- --nocapture`.

## Resolution

**Original question answered directly.** At N=8, one tick on a
`0.00001`-priced pair is **0.1% (10 bps)** of price, and the scaled integer
carries ~3-4 significant figures. Neither 0.01% nor 8% is right.

**Broader finding.** For the pairs Meridian currently plans to support,
**N=12 in `i64`** is the only scale that keeps tick size below 1 bp for
every pair without overflowing storage. It works, but with no margin: one
memecoin at `1e-10` or one DEX pool with awkward decimals reopens the
decision, and this is before Q3's notional-overflow arithmetic bites.

**What this pins down for ADR 0001.**

- Fixed `N` in `i64` is viable **only** if we commit to a documented lower
  bound on supported price (~`1e-9`) and prove Q3 fits.
- Alternatives worth carrying into ADR 0001:
  1. Fixed `N=18` in `i128`. Buys full range at the cost of doubled storage
     and slower arithmetic.
  2. Per-symbol scale. Each `Price` value knows its own `N`, stored in the
     `Symbol` (or a sidecar). Retains `i64` fast path per pair, opens up
     the full range across pairs, but complicates comparisons across
     venues quoting the same pair at different scales.
  3. A decimal type that carries scale internally (`rust_decimal`).
     Simplest to reason about, slowest per-op, and puts Q5 (hash equality)
     directly on the critical path.

Q2 does not pick a winner; it establishes that "just use N=8" is
demonstrably wrong and that the real choice lives between the three
alternatives above. ADR 0001 gets to weigh them once Q3, Q4, Q5, Q6 land.
