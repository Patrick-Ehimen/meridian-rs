# Q8 - Bid ordering idiom

- **Blocks:** [ADR 0002 - Order book side storage](../../decisions/0002-book-storage.md)
- **Status:** Resolved.
- **Codified in:** `crates/core/src/book.rs` (`BookSide`, `Levels`, tests prefixed `q8_`).

## Question

Bid ordering: `.iter().rev()` or `Reverse` in the key? Which one will I misread
in six months?

## Approach

Write the three queries the book actually serves (best level, best-first
iteration, band depth from Q7) under each idiom, and look for the place where
the bid version reads differently from the ask version. That difference is
where the six-months-later bug lives.

## Working notes

### Option 1: `.iter().rev()` at call sites

Both sides are `BTreeMap<Price, Qty>`. Bids are read from the back.

- Best bid: `bids.last_key_value()`, best ask: `asks.first_key_value()`.
- Band: `bids.range(limit..).rev()` vs `asks.range(..=limit)`.

Bounds read naturally on both sides. The failure mode is *omission*:
every call site that walks bids has to remember `.rev()`, and forgetting it
compiles, runs and returns the **worst** bid first. Nothing in the types
catches it. With a TUI, an aggregator, an arbitrage signal and three servers
all reading books, that is a lot of call sites to get right.

### Option 2: `Reverse<Price>` as the bid key

`bids: BTreeMap<Reverse<Price>, Qty>`, so `.iter().next()` is the best bid.

- Best on both sides is `first_key_value()`. That part is genuinely nicer.
- Band: `bids.range(..=Reverse(limit))`. The bound flips direction relative
  to the ask side, and every key has to be wrapped and unwrapped.
- The two sides are now different types, so any function over "a side" must
  be generic over the key or duplicated.

The failure mode here is worse than omission. Written the natural way,
"bids between 99 and 101" is `range(Reverse(99)..=Reverse(101))`, and in
`Reverse` order that start is *greater* than the end. `BTreeMap::range`
**panics** on that ("range start is greater than range end"). The
`q8_reverse_keyed_range_with_natural_bounds_panics` test pins this. So the
slick idiom turns a misread into a runtime crash in the hot path.

### Option 3: neither idiom at call sites

Keep plain `Price` keys on both sides (Option 1's storage), but wrap each map
in a `BookSide { side, levels }` that owns the direction. Callers ask for
`best()`, `levels()` or `within(limit)` and always get best first. The
`.rev()` happens in exactly one place, inside a small `Levels` iterator that
calls `next_back()` for bids and `next()` for asks over a single
`btree_map::Range`, so both sides return the same concrete type with no
boxing.

## Resolution

**Option 3.** Plain `Price` keys on both sides, with the direction
encapsulated in `BookSide`.

- `range` bounds read identically on bids and asks, so the Q7 band query has
  no flipped-bound trap.
- The "forgot `.rev()`" bug cannot happen outside `book.rs`, because no other
  crate can reach the map.
- Bids and asks are the same type, so aggregator code that works over "a
  side" is written once.
- The price comparison that does depend on direction is also in one place:
  `Side::is_better(a, b)`.

The one-line answer to "which will I misread in six months": `Reverse`,
because its misread panics; and `.rev()` scattered across call sites, because
its misread is silent. Encapsulation means I do not have to read either.
