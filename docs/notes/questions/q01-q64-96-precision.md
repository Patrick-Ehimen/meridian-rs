# Q1 - Q64.96 precision demand

- **Blocks:** [ADR 0001 - Core price & quantity types](../../decisions/0001-core-types.md)
- **Status:** Resolved. All three sub-parts landed; ready to graduate to
  [ADR 0001](../../decisions/0001-core-types.md).
- **Codified in:** `crates/venues/uniswap-v3/src/lib.rs`

## Question

What precision does Q64.96 actually demand, and can one representation hold
both it and Binance's 2-decimal USDT quotes without loss?

## Approach

Breaking this into three sub-parts, all arithmetic:

1. **Fractional precision.** 96 binary fractional bits corresponds to how many
   decimal digits? Compute `log10(2^96)`.
2. **Integer range.** 64 integer binary bits on the *square root* of price
   means the represented price range is `(2^64)^2 = 2^128`. What does that
   mean in human decimal terms at the top and bottom?
3. **The squaring step.** I do not store price, I store `sqrt(price)`. Squaring
   `sqrtPriceX96` doubles the fractional precision from 96 to 192 bits, but
   the value is then scaled by `2^192`, not `2^96`. What precision survives
   *after* the token-decimals adjustment (`* 10^(decimals0 - decimals1)`)?

The trap to avoid: quoting "Q64.96 gives ~29 decimal digits" and stopping
there. Price itself is Q128.192 once squared, and the human-readable price
after decimals adjustment can have far fewer significant digits than that if
the two token decimals differ wildly. WETH 18 vs USDC 6 is a `10^12` gap on
its own.

## Working notes

### Sub-part 1: fractional precision is ~28.9 decimal digits

`log10(2^96) = 96 * log10(2) = 96 * 0.30103 = 28.898...`

So 96 binary fractional bits carries **~28.9 decimal digits** of precision. The
smallest representable fractional increment is `1 / 2^96 = 1.26 * 10^-29`.

Concretely:

- Any decimal representation with fewer than 29 fractional digits loses
  information when parsing a raw `sqrtPriceX96` value.
- For comparison: IEEE 754 `f64` has 52 mantissa bits, or ~15.95 decimal
  digits. Q64.96 has almost 2x the fractional resolution of an `f64`.
- This is the precision of `sqrt(price)`, not `price`. Sub-part 3 is where
  the post-squaring, post-decimals-adjustment picture arrives.

Codified in `crates/venues/uniswap-v3/src/lib.rs`:

- `SQRT_PRICE_SHIFT: u32 = 96` and `Q96: u128 = 1 << 96` as the two primitive
  constants.
- Five tests lock them down: `Q96` matches `2^96` computed two ways, matches
  Uniswap's whitepaper reference value
  (`79_228_162_514_264_337_593_543_950_336`), sits strictly between `10^28`
  and `10^29`, prints as 29 characters, and shift round-trips a set of small
  integers.

What this pins down for the eventual `Price` type: the floor is 29 decimal
digits, or we accept a documented, understood loss.

### Sub-part 2: integer range and the u128 overflow trap

64 integer binary bits on `sqrt(price)` gives:

- `sqrt(price)` range: `1 / 2^96` up to `2^64`
- Squared, `price` range: `1 / 2^192` up to `2^128`
- `log10(2^128) = 128 * 0.30103 = 38.5`
- So the unscaled price range is roughly `10^-38.5` to `10^38.5`. Token
  decimals adjustment (`* 10^(dec0 - dec1)`) shifts that window along the
  number line without changing its width. WETH 18 vs USDC 6 shifts by
  `10^12`.

But that is only the **theoretical** range. The on-chain value is a `uint160`
and Uniswap V3 constrains actual `sqrtPriceX96` values to a tighter band via
`MIN_SQRT_RATIO` and `MAX_SQRT_RATIO`, corresponding to tick range
`[-887272, 887272]`:

- `MIN_SQRT_RATIO = 4_295_128_739` (~10 decimal digits, fits in `u32`)
- `MAX_SQRT_RATIO = 1_461_446_703_485_210_103_287_273_052_203_988_822_378_723_970_342`
  (49 decimal digits, ~160 bits)

The trap: `u128::MAX` is ~`3.4 * 10^38` (39 digits). `MAX_SQRT_RATIO` exceeds
`u128::MAX` by roughly **10 decimal digits**. Any Rust code that holds a
real-world `sqrtPriceX96` in a `u128` is a latent overflow bug on high-price
pools.

What this pins down for the adapter's internal working type:

- Raw `sqrtPriceX96` storage must be at least `U192` or `U256`.
  `alloy::primitives::U256` is the natural choice since `alloy` returns it
  from `slot0()` anyway.
- The squared price (before shifting back down) needs even more room, up to
  `2^320`. That is sub-part 3's territory.
- This does **not** affect the eventual `Price` type directly. By the time
  we reach `Price`, we have already converted out of Q-space. It dictates
  only the adapter's internal working type.

Codified in `crates/venues/uniswap-v3/src/lib.rs`:

- `MIN_SQRT_RATIO: U256` and `MAX_SQRT_RATIO: U256`, using
  `alloy::primitives::uint!` for compile-time construction from the
  documented literals.
- Four tests: `MAX_SQRT_RATIO > u128::MAX` (the headline finding),
  `MIN_SQRT_RATIO < u128::MAX` (negative control, prevents the wrong
  lesson), `MIN < MAX` (swap-guard), and `MAX_SQRT_RATIO` prints as 49
  decimal digits (typo-guard on the literal).

### Sub-part 3: the squaring step

`price = (sqrtPriceX96)^2 / 2^192`. Two things follow.

**Precision doubles.** 96 fractional binary bits become **192**, or:

```
log10(2^192) = 192 * 0.30103 = 57.796...
```

So the raw squared price carries **~57.8 decimal digits** of fractional
precision. The token-decimals adjustment (`* 10^(dec0 - dec1)`) is exact
multiplication by a power of ten, which slides the value along the number
line without losing bits, so it does not degrade this figure.

**The naive squaring order overflows `U256`.** `MAX_SQRT_RATIO` is ~`2^160`,
so `MAX_SQRT_RATIO^2` reaches ~`2^320`. `U256` caps at `2^256`, roughly `10^77`
short of what is needed. Two options:

- Widen to `U512` for the intermediate. `alloy::primitives::U512` is the
  standard choice.
- Reorder to shift-then-square (drops precision but stays in `U256`). Only
  viable if the analytical view can tolerate the loss.

Codified in `crates/venues/uniswap-v3/src/lib.rs`:

- `PRICE_SHIFT: u32 = 2 * SQRT_PRICE_SHIFT` (192).
- `Q192: U256 = 2^192` via `uint!` literal.
- Five tests: `Q192 == 1 << 192`; `Q96 * Q96 == Q192`;
  `MAX_SQRT_RATIO.checked_mul(MAX_SQRT_RATIO)` returns `None` (the overflow
  trap); `U512::from(MAX_SQRT_RATIO).pow(2)` succeeds (the escape); `Q192`
  sits between `10^57` and `10^58` (encodes the ~57.8-digit precision claim).

## Resolution

Q64.96, as it applies to Meridian:

1. **Fractional precision (sub-part 1):** ~29 decimal digits pre-squaring.
   Any decimal representation with fewer than 29 fractional digits loses
   information when parsing a raw `sqrtPriceX96`. `f64` is not enough
   (~16 digits).
2. **Integer range (sub-part 2):** real `sqrtPriceX96` values reach ~49
   decimal digits, exceeding `u128::MAX` by ~10 digits. **Adapter storage
   type for raw `sqrtPriceX96` must be at least `U256`.**
3. **Squaring (sub-part 3):** post-squaring precision is ~58 decimal digits
   in Q128.192; the naive squaring order overflows `U256`, so **squared-price
   arithmetic must use `U512`** (or shift-then-square, accepting loss).
4. **Token-decimals adjustment is lossless.** Multiplying by
   `10^(dec0 - dec1)` slides the magnitude without touching precision, so
   the ceiling stays at ~58 decimal digits regardless of pair.

**Implications for the eventual `Price` type in `crates/core`:**

- The adapter boundary is where Q-space ends. `Price` never sees a
  `sqrtPriceX96` directly.
- To preserve all sub-part 3 precision downstream, `Price` would need ~58
  fractional decimal digits. Most decimal libraries (including
  `rust_decimal`) cap at ~28. **A truncation at the adapter boundary is
  expected; it must be documented, not silent.**
- Consolidated depth is explicitly an analytical view, not tradeable
  liquidity, so a 15-to-18-digit downstream representation is defensible.
  This decision moves to ADR 0001.

Ready to graduate.
