# Q6 - Symbol representation

- **Blocks:** [ADR 0001 - Core price & quantity types](../../decisions/0001-core-types.md)
- **Status:** Resolved.
- **Codified in:** `crates/core/src/symbol_shape_analysis.rs`.

## Question

`Symbol`: structured `{ base, quote }` or an interned canonical string? A
Uniswap pool address and a Solana mint pair have to fit whichever I pick.
See the identifier column in
[venue wire formats](../../learnings/venue-wire-formats.md).

## Approach

Three candidate shapes:

- **A. Structured `{ base: Ticker, quote: Ticker }`** with fixed-size
  tickers so `Symbol` is `Copy`.
- **B. Interned canonical string** with a global `Symbol(u32)` index.
- **C. Enum over venue-native identifiers** (`Cex{..}`, `UniswapV3(Address)`,
  `Jupiter{..}`).

Method:

1. Reject C outright: consolidated depth requires cross-venue key equality
   (BTCUSDT on Binance and Bybit must produce the same Symbol). An enum
   keyed on venue-native identifiers cannot deliver that.
2. Between A and B, pick by scale. Meridian aggregates a few hundred pairs,
   not tens of thousands. At that scale, fixed-size `Copy` beats a global
   interner.
3. Prove A covers every CEX ticker and prove it does **not** cover DEX
   pool addresses / mint pubkeys. That gap forces the two-layer design
   CLAUDE.md already implies: adapters own venue-native identifiers and
   map them to canonical `Symbol` at ingest.

## Working notes

### Ticker sizing

Longest tickers observed on the target venues:

| Ticker | Bytes |
| --- | --- |
| BTC, ETH, SOL | 3 |
| USDT, USDC, WBTC, WETH, SHIB, PEPE, BONK | 4 |
| MATIC, TRUMP, WEETH | 5 |
| 1000PEPE, 1000BONK (Binance micro-price convention) | 8 |

`Ticker = [u8; 12]` gives a comfortable four-byte margin over the longest
observed listing. `Symbol { base, quote }` lands at exactly 24 bytes.

### The DEX-identifier gap

- Ethereum pool address: 20 bytes. Larger than a single Ticker.
- Solana mint pubkey pair: 64 bytes. Larger than the entire canonical
  Symbol.

Neither fits inside `Symbol`. Both must live in per-adapter mapping
tables (`VenueSymbol -> Symbol`), populated at startup from venue
metadata:

- Uniswap adapter reads `token0()`/`token1()` on each configured pool,
  looks up token symbols and decimals, produces the canonical Symbol.
- Jupiter adapter looks up mint pubkeys in a token registry, produces
  the canonical Symbol.

This is the CLAUDE.md invariant made concrete: "venue-specific parsing
stays inside the adapter, never leaks into shared code".

### Cost comparison (from `print_symbol_option_sizes`)

| Option | Size per Symbol | Trade-off |
| --- | --- | --- |
| A. Canonical `{ Ticker, Ticker }` | 24 bytes | `Copy`, no global state |
| B. Interned `u32` | 4 bytes | needs registry + cross-thread sync |
| C. Enum over venue identifiers | variable | no cross-venue equality |

24 bytes is a rounding error at a few hundred symbols. The `Copy`
ergonomics matter far more: `Symbol` flows freely across adapter tasks,
`DashMap` entries, gRPC/REST/WebSocket response types.

### Codified in tests

`crates/core/src/symbol_shape_analysis.rs` has seven tests plus a size
printer. The six load-bearing tests:

- `representative_tickers_fit_max_ticker_len` - every observed CEX
  ticker fits in 12 bytes, including `1000PEPE`. Regression guard if a
  future listing pushes past.
- `canonical_symbol_is_copy_and_small` - Symbol is `Copy` and sits at
  24 bytes, well under a cache line.
- `identically_spelled_symbols_are_equal_and_hash_equal` - derived
  `PartialEq` and `Hash` agree. No Q5-style contract violation.
- `symbols_have_total_order` - `Ord` derives correctly, giving stable
  BTreeMap iteration order.
- `ethereum_pool_address_does_not_fit_ticker` - locks the two-layer
  design in code: pool addresses cannot live in `Symbol`.
- `solana_mint_pair_does_not_fit_canonical_symbol` - same story for
  Jupiter.

Plus `print_symbol_option_sizes` for the trade-off table.

## Resolution

`Symbol` is **structured `{ base: Ticker, quote: Ticker }`** with
`Ticker = [u8; 12]`.

- Copy, 24 bytes, derives `PartialEq + Eq + Hash + PartialOrd + Ord`
  correctly.
- Covers every CEX ticker observed with margin.
- Explicitly does **not** cover DEX-native identifiers. That is a
  feature: pool addresses and mint pubkeys live in per-adapter
  mapping tables where they belong.

**Two-layer design that follows:**

1. `Symbol` (canonical, in `crates/core`): what the aggregator and
   servers speak.
2. `VenueSymbol` (per-adapter internal): whatever the venue's native
   identifier is. Each adapter owns a `HashMap<VenueSymbol, Symbol>`
   populated at startup.

**What this pins down for ADR 0001.**

- `Symbol { base: Ticker, quote: Ticker }` with `Ticker = [u8; 12]` is
  the recommended shape.
- Adapter trait (`VenueFeed` or whatever step 02 lands on) must expose
  a way to configure the `VenueSymbol -> Symbol` mapping at startup.
- If a future ticker exceeds 12 bytes, either bump `MAX_TICKER_LEN` and
  rebuild, or reject the listing at ingest with a clear error. The
  regression test in `symbol_shape_analysis.rs` will fail loudly if the
  representative set outgrows the size.
