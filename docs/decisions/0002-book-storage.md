# 0002 - Order book side storage

- **Status:** Proposed. I have not decided yet, see [Open questions](../notes/questions.md)
- **Date:** 2026-08-24
- **Affects:** `crates/core`

## Context

Each side of a book is a price-ordered collection of levels that I mutate on
every inbound delta. Whatever I pick, the aggregator and the TUI inherit its
performance characteristics, and [ADR 0001](0001-core-types.md) constrains it:
the key type must be `Ord`, which some numeric representations cannot give me.

Three access patterns have to be cheap, and they pull in different directions:

| Operation | Who needs it | How often |
| --- | --- | --- |
| Read top-of-book | spread, arbitrage signal, TUI | every tick |
| Insert / update / delete one level by price | every inbound delta | every message |
| Iterate in price order, from the top, up to depth N | consolidated depth, VWAP | every aggregation pass |

The third one is the constraint people forget. Consolidated depth is not "give me
every level" - it is "walk outward from the touch until I have accumulated N of
notional, then stop". A structure that cannot do a bounded ordered walk from the
best price forces a full scan on every aggregation pass.

I also have to keep bids descending and asks ascending without confusing myself,
and the two idioms for that (`.iter().rev()` on bids, versus wrapping the bid key
in `std::cmp::Reverse` so `.next()` is always the best price) have different
failure modes when read six months from now.

## Options considered

_Not yet written._

## Decision

_Not yet made._

## Why I rejected the others

_Pending._

## Consequences

_Pending._

## I should revisit this if

_Pending. Likely candidates: a benchmark on recorded fixtures once
[step 04](../notes/questions.md) gives me replayable data, or a venue whose book
depth is far larger than I assumed._
