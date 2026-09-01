# Open questions

One file per question in [questions/](questions/), each following the same
template:

```
# Q<N> - <title>

- **Blocks:** <ADR or area>
- **Status:** <Open / In progress / Resolved>
- **Codified in:** <path or "not yet">

## Question
## Approach
## Working notes
## Resolution
```

A question stays here until it is fully resolved, then graduates to a
[learning](../learnings/index.md) if it is a fact or a
[decision](../decisions/index.md) if it is a choice, and its file is deleted.

## Core types (blocking [ADR 0001](../decisions/0001-core-types.md))

| # | Question | Status |
| --- | --- | --- |
| [Q1](questions/q01-q64-96-precision.md) | Q64.96 precision demand | Resolved |
| [Q2](questions/q02-low-price-pair-scale.md) | Low-price pair under a fixed 8-decimal scale | Open |
| [Q3](questions/q03-notional-overflow.md) | Notional overflow in the intermediate | Open |
| [Q4](questions/q04-shared-scale.md) | Should `Price` and `Qty` carry the same scale? | Open |
| [Q5](questions/q05-rust-decimal-hash.md) | Does `rust_decimal` hash equal for equal values? | Open |
| [Q6](questions/q06-symbol-shape.md) | `Symbol` representation | Open |

## Book storage (blocking [ADR 0002](../decisions/0002-book-storage.md))

| # | Question | Status |
| --- | --- | --- |
| [Q7](questions/q07-btreemap-range.md) | Which aggregator operation needs `BTreeMap::range`? | Open |
| [Q8](questions/q08-bid-ordering.md) | Bid ordering idiom | Open |

## Ingestion

| # | Question | Status |
| --- | --- | --- |
| [Q9](questions/q09-coinbase-no-sequence.md) | Coinbase `l2update` has no sequence number | Open |
| [Q10](questions/q10-venuefeed-gap-detection.md) | Where does gap detection live in the `VenueFeed` trait? | Open |
| [Q11](questions/q11-backpressure.md) | Backpressure contract across venues | Open |

## Aggregation

| # | Question | Status |
| --- | --- | --- |
| [Q12](questions/q12-dashmap-iteration.md) | `DashMap` iteration order and consolidated VWAP | Open |
| [Q13](questions/q13-consolidated-depth-jupiter.md) | What does consolidated depth mean when one leg is a Jupiter quote curve? | Open |

## Verification

| # | Question | Status |
| --- | --- | --- |
| [Q14](questions/q14-fixture-verification.md) | Reconfirm venue wire formats against captured fixtures | Open |
