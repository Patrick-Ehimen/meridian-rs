# 0002 - Order book side storage

- **Status:** Accepted
- **Date:** 2026-11-17
- **Affects:** `crates/core`

## Context

Each side of a book is a price-ordered collection of levels that I mutate on
every inbound delta. Whatever I pick, the aggregator and the TUI inherit its
performance characteristics, and [ADR 0001](0001-core-types.md) constrains it:
the key type must be `Ord`, which some numeric representations cannot give me.
Under ADR 0001 `Price` is a fixed-point `i64` with a derived numeric `Ord`, so
it is a valid key as is.

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

[Q7](../notes/questions/q07-btreemap-range.md) sharpened that row. A walk
bounded by accumulated size only needs ordered iteration from the touch. The
read that needs a *seek by price* is band depth: "everything within X bps of
mid". Its bound is a price, and how many levels fall inside it varies by
venue and by second.

I also have to keep bids descending and asks ascending without confusing myself.
[Q8](../notes/questions/q08-bid-ordering.md) compared the two idioms: `.iter().rev()`
on bids fails silently when a call site forgets it, and a `std::cmp::Reverse`
bid key flips `range` bounds so the natural spelling panics.

## Options considered

### S1 - `BTreeMap<Price, Qty>` per side, plain keys, wrapped in `BookSide`

Both sides key on the plain `Price`. A `BookSide { side, levels }` owns the
direction: `best()`, `levels()` and `within(limit)` all return levels best
first, via one iterator type that pulls from the back of a `btree_map::Range`
for bids.

- Top of book: `O(log n)` via `first_key_value` / `last_key_value`.
- Insert / delete anywhere: `O(log n)`.
- Band depth: `O(log n)` to seek plus the levels in the band.
- The map is private, so `.rev()` is written once.

### S2 - `BTreeMap<Reverse<Price>, Qty>` for bids, `BTreeMap<Price, Qty>` for asks

Same complexity as S1. `first_key_value()` is the best level on both sides.

- Bids and asks are different types: shared code must be generic or
  duplicated.
- Band bounds flip on the bid side, and the natural spelling panics inside
  `BTreeMap::range` (pinned by `q8_reverse_keyed_range_with_natural_bounds_panics`).

### S3 - Sorted `Vec<Level>` per side, touch at the end

What many production books use on the hot path. The touch sits at the end of
the vector, so the common update near the top shifts only a few elements.

- Top of book: `O(1)`.
- Insert / delete: `O(log n)` to find, `O(k)` to shift, where `k` is the
  distance from the touch. Cheap near the top, `O(n)` deep in the book.
- Band depth: `partition_point` gives the same `O(log n)` seek as `range`.
- Contiguous memory, so iteration is much more cache-friendly.

### S4 - Skip list or custom structure

No measured problem for it to solve. Not considered further.

## Decision

Each side of a `Book` is a `BTreeMap<Price, Qty>` keyed by the plain `Price`,
wrapped in a `BookSide` that owns the bid/ask direction. Concretely, in
`crates/core/src/book.rs`:

```rust
pub struct BookSide {
    side: Side,
    levels: BTreeMap<Price, Qty>,
}

impl BookSide {
    pub fn best(&self) -> Option<Level>;
    pub fn set(&mut self, price: Price, qty: Qty) -> Option<Qty>; // qty 0 deletes
    pub fn levels(&self) -> Levels<'_>;                            // best first
    pub fn within(&self, limit: Price) -> Levels<'_>;              // at or better than limit
}
```

The map is private. No crate outside `core` ever sees a raw `BTreeMap` of
levels, so no caller writes `.rev()` or reasons about key direction.

## Why I rejected the others

- **S2** buys a symmetric `first_key_value()` and pays for it with two side
  types and a `range` that panics when written the obvious way. Q8's whole
  point was picking the idiom I will not misread later; S2 is the one whose
  misread crashes.
- **S3** is the likely end state for the hot path, but I have no fixture data
  yet to show it is needed, and it is the one option where a deep update
  costs `O(n)`. Binance diff streams touch levels hundreds deep. Choosing it
  now would be optimizing before measuring. Because `BookSide` hides the map,
  switching to S3 later changes one file.
- **S4** solves nothing I have measured.

## Consequences

**What this makes easy.**

- Every Q7 read is a method call: `best()`, `levels().take(n)`, a `levels()`
  walk with an early break, and `within(limit)` for band depth.
- Bids and asks share one type, so aggregator code over "a side" is written
  once.
- The CEX "qty 0 means delete" convention is enforced in exactly one place,
  `BookSide::set`. No zero-quantity level can be stored.
- The storage is swappable behind `BookSide` without touching callers.

**What this makes hard.**

- Iteration chases pointers through B-tree nodes. If profiling shows the
  aggregation pass is memory-bound, S3 is the fix.
- `Levels` cannot be `ExactSizeIterator`, because `btree_map::Range` is not.
  Callers wanting a count call `len()` on the side.

**The `Book::apply` contract that sits on top.** Recorded here because it is
the other half of what `Book` promises its callers:

- A delta covers update ids `first..=last`. If `last <= book.last_update_id`
  it is **stale**: `Ok(Stale)`, book untouched. This is a no-op rather than
  an error because Binance's documented sync procedure produces stale diffs
  on every startup.
- If `first <= book.last_update_id + 1` and `last > book.last_update_id` it
  is **contiguous or overlapping**: applied. Overlap is safe because level
  updates are absolute quantities, not increments.
- If `first > book.last_update_id + 1` it is a **gap**:
  `Err(SequenceGap)`, book untouched, adapter must resnap. Never continue.
- If the applied delta leaves `best_bid >= best_ask`, `Err(CrossedBook)`.
  The delta has already been applied (checking first would mean copying the
  book on every message), so the book is poisoned and must be resnapped.
  Locked counts as crossed.

**Tests that pin the invariants.** In `crates/core/src/book.rs`:

- `legal_delta_sequences_preserve_every_invariant` (proptest, model-based):
  after any mix of contiguous, overlapping and stale deltas the book equals a
  reference map, both sides are strictly ordered best first, no zero level is
  stored, and `last_update_id` never decreases.
- `apply_never_reports_success_on_a_crossed_book` (proptest).
- `a_gap_never_mutates_the_book` (proptest).
- `q7_*` and `q8_*` unit tests for band depth and the `Reverse` panic.

## I should revisit this if

- **A benchmark on recorded fixtures** (once step 04 gives me replayable
  data) shows the aggregation pass or delta apply is dominated by B-tree
  traversal. Then S3 behind the same `BookSide` API.
- **A venue's book is far deeper than assumed**, so that full-book walks in
  the TUI or servers start to show up in profiles.
- **A venue legitimately sends locked books.** Then `check_not_crossed`
  relaxes to strict `>`, and that venue's adapter documents why.
- **A venue sends incremental (delta-quantity) level updates** instead of
  absolute ones. The overlap rule above would then double-apply, and
  `apply` needs a strict-contiguity mode.
