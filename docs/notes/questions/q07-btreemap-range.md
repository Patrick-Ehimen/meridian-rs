# Q7 - Which aggregator operation needs BTreeMap::range?

- **Blocks:** [ADR 0002 - Order book side storage](../../decisions/0002-book-storage.md)
- **Status:** Resolved.
- **Codified in:** `crates/core/src/book.rs` (`BookSide::within`, tests prefixed `q7_`).

## Question

Which aggregator operation specifically needs `BTreeMap::range`? If I cannot
name it, I do not yet know why I am choosing the data structure.

## Approach

List every read the aggregator, arbitrage signal and TUI will make against a
book side, then ask of each one: is it bounded by a *count* of levels, or by a
*price*? Only a price bound needs `range`. A count bound is satisfied by any
structure that iterates in order from the touch.

## Working notes

### The reads, classified

| Read | Consumer | Bounded by | Needs |
| --- | --- | --- | --- |
| Top of book | spread, arbitrage, TUI | nothing (one level) | `first_key_value` / `last_key_value` |
| Top N levels | TUI ladder | count | ordered iteration + `take(n)` |
| Walk until N notional filled (slippage / impact price) | arbitrage sizing, VWAP-to-size | accumulated size | ordered iteration + early `break` |
| Depth within X bps of mid ("2% depth") | consolidated depth | **price** | `range` |
| Point lookup / update at one price | every delta | exact key | `get` / `insert` / `remove` |

The notional walk looks like it needs `range`, but it does not: it starts at
the touch and stops on a running total, so the stopping point is not known
until I get there. `iter()` with a `break` does the job, and a sorted
`Vec` would do it equally well.

### Band depth is the one

"How much rests within 10 bps of mid" is the standard cross-venue liquidity
metric, and it is exactly the consolidated-depth view CLAUDE.md describes. The
bound is a price computed from mid, not a level count, and venues differ
wildly in tick size: at BTC around $100k, 10 bps is $100, which spans up to
10,000 price ticks at a $0.01 tick, while on a coarse-tick venue it can be a
handful. How many of those ticks are populated changes every second. A
count-bounded walk cannot express it.

With `range` it is `O(log n)` to find the band edge plus the levels inside the
band, and nothing outside the band is visited:

- asks: `levels.range(..=mid * (1 + x))`
- bids: `levels.range(mid * (1 - x)..)`, read from the back

A sorted `Vec` can get the same bound with `partition_point` (binary search),
so `range` is not unique to `BTreeMap`. What `BTreeMap` adds is that the same
structure also gives `O(log n)` insert and delete anywhere in the book, which
the `Vec` only gives near the end it keeps the touch at.

### A second, later use

Once the Uniswap V3 adapter (step 07) projects liquidity onto a price grid,
merging it into a consolidated band will be a `range` over the CEX books
between adjacent initialized-tick prices. Same operation, different bound.

## Resolution

The operation is **band depth: all liquidity within a price limit of mid**.
It is bounded by price, not by level count, so it needs an ordered structure
that can seek to a key. Top-of-book, top-N and the notional walk do not need
`range` at all; they need ordered iteration from the touch.

In code this is `BookSide::within(limit)`, which yields levels at or better
than `limit`, best first, on either side. The `q7_` tests pin the band on both
sides and the two edge cases (limit beyond the whole side, limit inside the
spread). The aggregator in step 06 computes the limit from mid and folds the
quantities; core only provides the slice.
