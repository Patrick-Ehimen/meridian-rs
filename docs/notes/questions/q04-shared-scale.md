# Q4 - Should Price and Qty carry the same scale?

- **Blocks:** [ADR 0001 - Core price & quantity types](../../decisions/0001-core-types.md)
- **Status:** Resolved.
- **Codified in:** `crates/core/src/qty_scale_analysis.rs`.

## Question

Should `Price` and `Qty` carry the **same** scale? WETH has 18 decimals, USDC
6, WBTC 8. One global `N` forces a single compromise across two different
axes.

## Approach

Q2 and Q3 quietly assumed a uniform scale (same `N` on both sides of
`Price * Qty`). Q4 tests whether that assumption survives contact with real
quantities. Two options:

- **A. Uniform scale.** One global `N` for both. Simple arithmetic, uniform
  storage width, no metadata to carry alongside each `Qty`.
- **B. Per-token scale.** `Qty` inherits the token's on-chain decimals.
  WETH quantities at scale 18, USDC at 6, WBTC at 8. Preserves exact atomic
  amounts from DEX but complicates every arithmetic op.

Method:

1. For each token, compute the **per-conversion truncation** when storing
   at a candidate `N` below the token's native decimals, and translate that
   loss into USD equivalent.
2. For each token, compute the **native-scaled storage size** if we used
   per-token scale, and check whether the whole set fits `i64` or forces
   `i128`.
3. Weigh precision-loss cost of Option A against storage/complexity cost
   of Option B.

## Working notes

### Precision loss under Option A (uniform N)

At storage scale `N`, the smallest storable real value is `10^-N`. Anything
below that in real terms is discarded when converting from a token's native
atomic units. So per-conversion truncation is bounded by `10^-N` (real), or
`0` if `N >= native_decimals`.

At **N = 12**, expressed as USD-equivalent loss per conversion:

| Token | Decimals | Truncation (real) | Truncation (USD) |
| --- | --- | --- | --- |
| USDC | 6 | 0 (pads by 10^6) | $0 |
| USDT | 6 | 0 | $0 |
| WBTC | 8 | 0 (pads by 10^4) | $0 |
| BONK | 5 | 0 (pads by 10^7) | $0 |
| SOL | 9 | 0 (pads by 10^3) | $0 |
| WETH | 18 | 1e-12 ETH | ~3e-9 USD |
| SHIB | 18 | 1e-12 SHIB | ~1e-17 USD |
| PEPE | 18 | 1e-12 PEPE | ~1e-18 USD |

Every truncation is far below the $0.01 dust threshold. For WETH
specifically: `1e-12` ETH is eight orders of magnitude below Binance's
`0.0001` ETH minimum trade size.

At **N = 18** every token in the set pads. No truncation anywhere. This is
the upper-bound option if we ever need exact atomic fidelity.

### Complexity cost under Option B (per-token N)

Per-token scale forces one of:

- **Uniform `i64` storage.** Fails: WETH at native 18 decimals with 1000-ETH
  pool liquidity requires storing `10^21`, well past `i64::MAX ~ 9.2e18`.
- **Uniform `i128` storage.** Works, but every `Qty` pays for `i128` even
  when the token only needs `i64` (USDC/USDT/WBTC). $1B in USDC at native
  6 decimals is `10^15`, comfortably within `i64`.
- **Varying storage width per token.** Kills the idea of a single `Qty`
  type; every `Qty` operation branches on the token.

On top of storage, every arithmetic op involving `Qty` needs scale metadata,
and cross-token comparisons need normalization.

### Codified in tests

`crates/core/src/qty_scale_analysis.rs` has seven tests plus a table
printer. The six load-bearing tests:

- `n12_truncation_is_below_dust_for_every_token` - Option A verdict: every
  per-conversion loss at N=12 is under $0.01.
- `n18_pads_every_token_without_loss` - N=18 is the no-loss upper bound.
- `weth_truncation_at_n12_stays_far_below_exchange_minimums` - the
  high-water-mark specific case: 1e-12 ETH ~ 3e-9 USD, eight orders below
  Binance's minimum trade.
- `per_token_scale_in_i64_overflows_for_weth` - rules out Option B with
  uniform i64.
- `per_token_scale_in_i128_fits_weth` - confirms Option B needs i128
  across the board.
- `per_token_scale_usdc_fits_i64_at_billion_dollar_notional` - shows the
  wasted headroom: low-decimal tokens pay for i128 they do not need.

Plus `print_qty_scale_table` for eyeballing the full trade-off with
`cargo test -p meridian-core qty_scale_analysis -- --show-output`.

## Resolution

**Uniform scale wins for Meridian.**

Precision loss under Option A is bounded and structurally invisible for
every token in the current set:

- Six of eight tokens pad without loss at N=12.
- The three 18-decimal tokens (WETH, SHIB, PEPE) truncate by at most
  `1e-12` in real units, translating to USD losses between `1e-18` and
  `3e-9` per conversion. All below dust by orders of magnitude.

Complexity cost under Option B is real: either overflow on i64, wasted
headroom on i128, or a `Qty` type that carries per-instance scale
metadata. None of these is worth paying for benefit Meridian does not
consume.

The one scenario that would flip this: **smart order routing** or any
execution path that submits DEX transactions with wei-exact amounts.
`CLAUDE.md` explicitly places that out of scope ("consolidated depth is
an analytical view, not tradeable liquidity"). If SOR ever lands, Q4
re-opens.

**What this pins down for ADR 0001.**

- `Price` and `Qty` share a global scale `N`. Options carried forward:
  - **A1**: `N = 12`, both in `i64` storage, `i128` intermediate. Fastest.
    Documented truncation of ~1e-12 on 18-decimal tokens.
  - **A2**: `N = 18`, both in `i128` storage, `i256` intermediate. No
    truncation. Wide-integer dependency and slower arithmetic.
- Q4 removes the "per-token scale" branch. The remaining choice between
  A1 and A2 is a speed-vs-fidelity trade that ADR 0001 gets to make once
  Q5 and Q6 land.
