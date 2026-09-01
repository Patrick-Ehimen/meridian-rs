# Venue wire formats

> **Provenance:** compiled from vendor documentation, **not** yet verified against
> recorded traffic. `fixtures/` is empty until step 04. Every claim on this page is
> provisional and should be re-confirmed against a captured payload, then rewritten
> in my own voice with the fixture cited. Until then, treat it as a survey, not a
> learning I have earned.

Why this page exists: I cannot choose the core types in
[ADR 0001](../decisions/0001-core-types.md) from taste. The choice is forced by
what the five venues actually put on the wire. This is that survey.

## The one-line summary

Four of the five venues send prices and quantities as **decimal strings**, on
purpose, so that I do not lose precision parsing them. The fifth does not have
prices at all - it has a square root of one, in binary fixed point. If the first
thing an adapter does is `parse::<f64>()`, the precision the venue carefully
preserved is gone on line one.

## Binance - spot diff-depth stream

Stream `<symbol>@depth@100ms`; snapshot from `GET /api/v3/depth`.

```json
{"e":"depthUpdate","E":1672515782136,"s":"BTCUSDT",
 "U":157,"u":160,
 "b":[["26314.50","0.00120000"]],
 "a":[["26315.10","1.35000000"]]}
```

- Price and quantity are **strings**. `[price, qty]` pairs, bids in `b`, asks in `a`.
- `U` = first update ID in this event, `u` = final update ID. A message covers a
  *range*, not a point - so gap detection compares ranges, not increments.
- Snapshot carries `lastUpdateId`. The documented sync procedure is: buffer the
  stream first, discard any event with `u <= lastUpdateId`, and require the first
  event applied to satisfy `U <= lastUpdateId + 1 <= u`. If it does not, resnap.
- `E` is an event timestamp in **epoch milliseconds**.
- Quantity `"0.00000000"` means delete the level.

## Coinbase - Exchange `level2` channel

A `snapshot` message, then `l2update` deltas on the same stream.

```json
{"type":"snapshot","product_id":"BTC-USD",
 "bids":[["26314.27","0.5"]],"asks":[["26315.02","1.1"]]}

{"type":"l2update","product_id":"BTC-USD",
 "changes":[["buy","26314.27","0.00000000"]],
 "time":"2022-08-04T15:25:05.010758Z"}
```

- Strings again, but the shape differs: changes are `[side, price, new_size]`
  triples with the side as the **word** `"buy"`/`"sell"`, not two separate arrays.
- The value is the **new absolute size** at that price, not a delta to add.
- **There is no per-message sequence number on `l2update`.** This is the single
  biggest normalization problem on this page - see the open question it raises.
- `time` is RFC 3339 with **microsecond** precision, not epoch millis.
- Symbol is `BTC-USD`, hyphenated, versus Binance's concatenated `BTCUSDT`.

## Bybit - v5 `orderbook.50.BTCUSDT`

```json
{"topic":"orderbook.50.BTCUSDT","type":"snapshot","ts":1672304484978,
 "data":{"s":"BTCUSDT",
   "b":[["16493.50","0.006"]],"a":[["16611.00","0.029"]],
   "u":18521,"seq":7961638724},
 "cts":1672304484976}
```

- Snapshot and delta arrive on the **same stream**, distinguished by `type`.
- **Two** sequence fields. `u` increments per message for this topic; `seq` orders
  across the service. `u == 1` signals a service restart and must be treated as a
  snapshot regardless of the `type` field.
- `ts` is when the system sent it; `cts` is when the matching engine produced it.
  Two different clocks, which is exactly why `Book` carries both `ts_exchange` and
  `ts_local`.
- Quantity `"0"` deletes.

## Uniswap V3 - no order book exists

Nothing here is a book. Depth is **derived** from pool state.

- `slot0()` returns `sqrtPriceX96` as a `uint160` - the square root of the price,
  in **Q64.96** binary fixed point (96 fractional bits).
- Human price of token1 in token0 is `(sqrtPriceX96 / 2^96)^2`, then scaled by
  `10^(decimals0 - decimals1)`.
- Equivalently, `price = 1.0001^tick`, where `tick` is an `int24`.
- `liquidity()` is a `uint128`. Initialized ticks are found through the tick bitmap.
- `Swap` / `Mint` / `Burn` events move this state.
- Token decimals are per-token and inconsistent: USDC 6, WETH 18, WBTC 8.

The precision question this forces: 96 fractional **binary** digits is roughly 29
decimal digits. Any fixed decimal scale I choose is a lossy projection of it. The
live question is whether that loss is acceptable given that our consolidated depth
is explicitly an analytical view, not tradeable liquidity.

## Jupiter - quote API, no book either

- `GET /quote?inputMint=..&outputMint=..&amount=..` returns `inAmount` and
  `outAmount` as **decimal strings of integer atomic units** - not human decimals.
  Converting requires the mint's decimals.
- There is no depth to read. A depth curve has to be **synthesized** by quoting
  several sizes and observing how `outAmount` degrades. That is a fundamentally
  different acquisition model from a subscribe-and-diff CEX feed.
- Solana lamports are 9 decimals; SPL token decimals come from the mint account.

## The cross-venue comparison that matters

| | Binance | Coinbase | Bybit | Uniswap V3 | Jupiter |
| --- | --- | --- | --- | --- | --- |
| Numbers on wire | decimal string | decimal string | decimal string | `uint160` Q64.96 | integer atomic units |
| Sequencing | `U`/`u` range | **none on l2update** | `u` + `seq` | block number | none (poll) |
| Snapshot source | separate REST call | same stream | same stream | contract read | n/a |
| Timestamp | epoch ms | RFC 3339 µs | epoch ms ×2 | block time | request time |
| Symbol form | `BTCUSDT` | `BTC-USD` | `BTCUSDT` | pool address | mint pubkey pair |
| Delete a level | qty `"0"` | size `"0"` | qty `"0"` | n/a | n/a |

Every column that differs is a thing the adapter must absorb so that nothing
downstream ever sees it. That is the whole argument for normalizing at the
adapter boundary, made concrete.

## What this gives me

1. **Do not parse to float.** The strings are the precision. Parse decimal text
   directly into whatever [ADR 0001](../decisions/0001-core-types.md) settles on.
2. **`Symbol` cannot be a venue string.** Five venues, five identifier schemes, one
   of which is a 20-byte address and another a pair of pubkeys.
3. **Gap detection cannot be one shared algorithm.** A range check, a two-field
   check, a restart sentinel, and one venue with no sequence at all. The
   `VenueFeed` trait has to let each adapter own its own gap rule and only report
   the *outcome* - "gap, resnap" - upward.
4. **Two timestamps is not paranoia.** Bybit ships two itself.
