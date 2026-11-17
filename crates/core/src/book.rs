//! Normalized order books and the events that maintain them.
//!
//! Storage follows [ADR 0002]: each side is a `BTreeMap<Price, Qty>` keyed by
//! the plain price on both sides, wrapped in a [`BookSide`] that knows which
//! direction is "better". Callers never write `.rev()` and never see a
//! `Reverse` key; they ask a side for its levels and get them best first.
//!
//! # Sequencing contract
//!
//! Every delta carries the inclusive range of venue update ids it covers.
//! [`Book::apply`] sorts a delta into one of three cases against the book's
//! `last_update_id`:
//!
//! | Delta range | Meaning | Result |
//! | --- | --- | --- |
//! | ends at or before `last_update_id` | already reflected | `Ok(Stale)`, book untouched |
//! | starts at or before `last_update_id + 1`, ends after | contiguous or overlapping | `Ok(Applied)` |
//! | starts after `last_update_id + 1` | at least one message lost | `Err(SequenceGap)`, book untouched |
//!
//! Stale is a no-op rather than an error because Binance's documented sync
//! procedure buffers diffs from before the snapshot and expects them to be
//! dropped. Overlap is safe to apply because level updates carry absolute
//! quantities, never increments: replaying one sets a level to the value it
//! already has, or to a newer one.
//!
//! [ADR 0002]: ../../../docs/decisions/0002-book-storage.md

use std::collections::BTreeMap;
use std::collections::btree_map;
use std::iter::FusedIterator;

use crate::amount::{Price, Qty};
use crate::error::CoreError;
use crate::side::{Level, Side};
use crate::symbol::Symbol;
use crate::time::Timestamp;
use crate::venue::Venue;

/// One side of one book: price levels, iterated best first.
///
/// Both sides key on the plain [`Price`], so `range` bounds read the same way
/// on bids and asks. The side tag decides which end of the map is the touch.
/// See ADR 0002 and Q8 for why the direction lives here and not in the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookSide {
    side: Side,
    levels: BTreeMap<Price, Qty>,
}

impl BookSide {
    pub fn new(side: Side) -> Self {
        Self {
            side,
            levels: BTreeMap::new(),
        }
    }

    pub const fn side(&self) -> Side {
        self.side
    }

    /// Number of resting levels. Zero-quantity levels are never stored.
    pub fn len(&self) -> usize {
        self.levels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    /// The touch: highest bid or lowest ask.
    pub fn best(&self) -> Option<Level> {
        let entry = match self.side {
            Side::Bid => self.levels.last_key_value(),
            Side::Ask => self.levels.first_key_value(),
        };
        entry.map(|(&price, &qty)| Level::new(price, qty))
    }

    /// The resting quantity at exactly `price`, if any.
    pub fn qty_at(&self, price: Price) -> Option<Qty> {
        self.levels.get(&price).copied()
    }

    /// Sets the absolute quantity at `price`, returning what was there.
    ///
    /// A zero quantity deletes the level. This is the one place the CEX
    /// "qty 0 means delete" convention is enforced, so no zero-quantity level
    /// can ever be stored.
    pub fn set(&mut self, price: Price, qty: Qty) -> Option<Qty> {
        if qty.is_zero() {
            self.levels.remove(&price)
        } else {
            self.levels.insert(price, qty)
        }
    }

    pub fn clear(&mut self) {
        self.levels.clear();
    }

    /// Every level, best first: descending for bids, ascending for asks.
    pub fn levels(&self) -> Levels<'_> {
        Levels {
            inner: self.levels.range(..),
            side: self.side,
        }
    }

    /// Levels at or better than `limit`, best first.
    ///
    /// For asks that is every price `<= limit`; for bids every price
    /// `>= limit`. This is the band-depth primitive from Q7: "how much rests
    /// within 10 bps of mid" is `within(limit)` with the limit computed from
    /// mid. It costs `O(log n)` to find the edge plus the levels yielded, with
    /// no scan of the levels outside the band.
    pub fn within(&self, limit: Price) -> Levels<'_> {
        let inner = match self.side {
            Side::Bid => self.levels.range(limit..),
            Side::Ask => self.levels.range(..=limit),
        };
        Levels {
            inner,
            side: self.side,
        }
    }
}

/// Best-first iterator over a [`BookSide`].
///
/// Wraps a single `btree_map::Range` and pulls from the back for bids, so
/// both sides share one concrete type without boxing.
#[derive(Clone, Debug)]
pub struct Levels<'a> {
    inner: btree_map::Range<'a, Price, Qty>,
    side: Side,
}

impl Iterator for Levels<'_> {
    type Item = Level;

    fn next(&mut self) -> Option<Level> {
        let entry = match self.side {
            Side::Bid => self.inner.next_back(),
            Side::Ask => self.inner.next(),
        };
        entry.map(|(&price, &qty)| Level::new(price, qty))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl FusedIterator for Levels<'_> {}

/// A change to one level: set the quantity at `price` on `side` to `qty`.
///
/// `qty` is absolute, never an increment. Zero deletes the level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelUpdate {
    pub side: Side,
    pub price: Price,
    pub qty: Qty,
}

/// An incremental update covering venue update ids
/// `first_update_id..=last_update_id`.
///
/// Venues that number every message with a single id set both fields to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookDelta {
    pub venue: Venue,
    pub symbol: Symbol,
    pub first_update_id: u64,
    pub last_update_id: u64,
    pub ts_exchange: Timestamp,
    pub ts_local: Timestamp,
    pub updates: Vec<LevelUpdate>,
}

/// What an adapter emits: a full replacement or an incremental change.
///
/// One enum rather than two channels, because venues send both on the same
/// stream and the ordering between them matters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BookEvent {
    Snapshot(Book),
    Delta(BookDelta),
}

impl BookEvent {
    pub fn venue(&self) -> Venue {
        match self {
            Self::Snapshot(book) => book.venue(),
            Self::Delta(delta) => delta.venue,
        }
    }

    pub fn symbol(&self) -> Symbol {
        match self {
            Self::Snapshot(book) => book.symbol(),
            Self::Delta(delta) => delta.symbol,
        }
    }
}

/// The non-error results of [`Book::apply`].
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// The delta was applied and `last_update_id` advanced.
    Applied,
    /// The delta was entirely at or before `last_update_id` and was ignored.
    Stale,
}

/// One venue's order book for one symbol.
///
/// Fields are private so the two invariants hold for every `Book` that
/// exists: no zero-quantity level is stored, and the book is never crossed
/// after a successful construction or apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Book {
    venue: Venue,
    symbol: Symbol,
    bids: BookSide,
    asks: BookSide,
    last_update_id: u64,
    ts_exchange: Timestamp,
    ts_local: Timestamp,
}

impl Book {
    /// An empty book that has seen no updates. Mostly useful in tests; real
    /// books start from [`Book::from_snapshot`].
    pub fn empty(venue: Venue, symbol: Symbol) -> Self {
        Self {
            venue,
            symbol,
            bids: BookSide::new(Side::Bid),
            asks: BookSide::new(Side::Ask),
            last_update_id: 0,
            ts_exchange: Timestamp::EPOCH,
            ts_local: Timestamp::EPOCH,
        }
    }

    /// Builds a book from a venue snapshot.
    ///
    /// Zero-quantity levels are dropped. If a price repeats, the later level
    /// wins, matching how a delta would treat it.
    ///
    /// # Errors
    ///
    /// [`CoreError::CrossedBook`] if the snapshot's best bid is at or above
    /// its best ask.
    pub fn from_snapshot(
        venue: Venue,
        symbol: Symbol,
        last_update_id: u64,
        ts_exchange: Timestamp,
        ts_local: Timestamp,
        bids: impl IntoIterator<Item = Level>,
        asks: impl IntoIterator<Item = Level>,
    ) -> Result<Self, CoreError> {
        let mut book = Self::empty(venue, symbol);
        for level in bids {
            book.bids.set(level.price, level.qty);
        }
        for level in asks {
            book.asks.set(level.price, level.qty);
        }
        book.last_update_id = last_update_id;
        book.ts_exchange = ts_exchange;
        book.ts_local = ts_local;
        book.check_not_crossed()?;
        Ok(book)
    }

    pub const fn venue(&self) -> Venue {
        self.venue
    }

    pub const fn symbol(&self) -> Symbol {
        self.symbol
    }

    pub const fn bids(&self) -> &BookSide {
        &self.bids
    }

    pub const fn asks(&self) -> &BookSide {
        &self.asks
    }

    pub const fn side(&self, side: Side) -> &BookSide {
        match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        }
    }

    /// The last venue update id reflected in this book.
    pub const fn last_update_id(&self) -> u64 {
        self.last_update_id
    }

    /// Venue-side time of the last snapshot or applied delta.
    pub const fn ts_exchange(&self) -> Timestamp {
        self.ts_exchange
    }

    /// Local receive time of the last snapshot or applied delta.
    pub const fn ts_local(&self) -> Timestamp {
        self.ts_local
    }

    pub fn best_bid(&self) -> Option<Level> {
        self.bids.best()
    }

    pub fn best_ask(&self) -> Option<Level> {
        self.asks.best()
    }

    /// Best ask minus best bid, or `None` if either side is empty.
    pub fn spread(&self) -> Option<Price> {
        let bid = self.best_bid()?;
        let ask = self.best_ask()?;
        Some(ask.price.abs_diff(bid.price))
    }

    /// Applies an incremental update, enforcing the sequencing contract in
    /// the module docs.
    ///
    /// # Errors
    ///
    /// - [`CoreError::BookMismatch`] if the delta is for another venue or
    ///   symbol. The book is untouched.
    /// - [`CoreError::InvalidUpdateRange`] if `first_update_id >
    ///   last_update_id`. The book is untouched.
    /// - [`CoreError::SequenceGap`] if the delta starts after the next
    ///   expected id. The book is untouched and must be resnapped.
    /// - [`CoreError::CrossedBook`] if the applied delta leaves the book
    ///   crossed. The delta *has* been applied, since checking first would
    ///   mean copying the book on every message; the book must be resnapped.
    pub fn apply(&mut self, delta: &BookDelta) -> Result<ApplyOutcome, CoreError> {
        if delta.venue != self.venue || delta.symbol != self.symbol {
            return Err(CoreError::BookMismatch {
                venue: self.venue,
                symbol: self.symbol,
                got_venue: delta.venue,
                got_symbol: delta.symbol,
            });
        }
        if delta.first_update_id > delta.last_update_id {
            return Err(CoreError::InvalidUpdateRange {
                first: delta.first_update_id,
                last: delta.last_update_id,
            });
        }
        if delta.last_update_id <= self.last_update_id {
            return Ok(ApplyOutcome::Stale);
        }
        // Cannot overflow: last_update_id < delta.last_update_id <= u64::MAX.
        let expected = self.last_update_id + 1;
        if delta.first_update_id > expected {
            return Err(CoreError::SequenceGap {
                expected,
                got: delta.first_update_id,
            });
        }

        for update in &delta.updates {
            self.side_mut(update.side).set(update.price, update.qty);
        }
        self.last_update_id = delta.last_update_id;
        self.ts_exchange = delta.ts_exchange;
        self.ts_local = delta.ts_local;

        self.check_not_crossed()?;
        Ok(ApplyOutcome::Applied)
    }

    fn side_mut(&mut self, side: Side) -> &mut BookSide {
        match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        }
    }

    /// A locked book (bid == ask) counts as crossed: no venue in the current
    /// set rests a bid and an ask at the same price.
    fn check_not_crossed(&self) -> Result<(), CoreError> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) if bid.price >= ask.price => Err(CoreError::CrossedBook {
                best_bid: bid.price,
                best_ask: ask.price,
            }),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Reverse;

    use proptest::prelude::*;

    use super::*;
    use crate::amount::SCALE_FACTOR;

    fn price(whole: i64) -> Price {
        Price::from_scaled(whole * SCALE_FACTOR).expect("non-negative")
    }

    fn qty(whole: i64) -> Qty {
        Qty::from_scaled(whole * SCALE_FACTOR).expect("non-negative")
    }

    fn level(p: i64, q: i64) -> Level {
        Level::new(price(p), qty(q))
    }

    fn btc_usdt() -> Symbol {
        Symbol::new("BTC", "USDT").expect("valid")
    }

    fn snapshot(last_update_id: u64, bids: &[(i64, i64)], asks: &[(i64, i64)]) -> Book {
        Book::from_snapshot(
            Venue::Binance,
            btc_usdt(),
            last_update_id,
            Timestamp::from_unix_millis(1),
            Timestamp::from_unix_millis(2),
            bids.iter().map(|&(p, q)| level(p, q)),
            asks.iter().map(|&(p, q)| level(p, q)),
        )
        .expect("not crossed")
    }

    fn delta(first: u64, last: u64, updates: &[(Side, i64, i64)]) -> BookDelta {
        BookDelta {
            venue: Venue::Binance,
            symbol: btc_usdt(),
            first_update_id: first,
            last_update_id: last,
            ts_exchange: Timestamp::from_unix_millis(last),
            ts_local: Timestamp::from_unix_millis(last + 1),
            updates: updates
                .iter()
                .map(|&(side, p, q)| LevelUpdate {
                    side,
                    price: price(p),
                    qty: qty(q),
                })
                .collect(),
        }
    }

    fn prices(levels: Levels<'_>) -> Vec<Price> {
        levels.map(|l| l.price).collect()
    }

    // -- Storage and iteration order ---------------------------------------

    #[test]
    fn best_is_highest_bid_and_lowest_ask() {
        let book = snapshot(1, &[(99, 1), (98, 1), (97, 1)], &[(101, 1), (102, 1)]);
        assert_eq!(book.best_bid(), Some(level(99, 1)));
        assert_eq!(book.best_ask(), Some(level(101, 1)));
        assert_eq!(book.spread(), Some(price(2)));
    }

    #[test]
    fn levels_iterate_best_first_on_both_sides() {
        let book = snapshot(1, &[(97, 1), (99, 1), (98, 1)], &[(102, 1), (101, 1)]);
        assert_eq!(
            prices(book.bids().levels()),
            [price(99), price(98), price(97)]
        );
        assert_eq!(prices(book.asks().levels()), [price(101), price(102)]);
    }

    #[test]
    fn zero_qty_in_a_snapshot_is_dropped() {
        let book = snapshot(1, &[(99, 0), (98, 1)], &[(101, 1)]);
        assert_eq!(book.bids().len(), 1);
        assert_eq!(book.bids().qty_at(price(99)), None);
    }

    #[test]
    fn zero_qty_in_a_delta_removes_the_level() {
        let mut book = snapshot(10, &[(99, 1), (98, 1)], &[(101, 1)]);
        let outcome = book.apply(&delta(11, 11, &[(Side::Bid, 99, 0)]));
        assert_eq!(outcome, Ok(ApplyOutcome::Applied));
        assert_eq!(book.bids().qty_at(price(99)), None);
        assert_eq!(book.best_bid(), Some(level(98, 1)));
    }

    #[test]
    fn deleting_a_level_that_does_not_exist_is_harmless() {
        let mut book = snapshot(10, &[(99, 1)], &[(101, 1)]);
        let before = book.bids().clone();
        assert_eq!(
            book.apply(&delta(11, 11, &[(Side::Bid, 50, 0)])),
            Ok(ApplyOutcome::Applied)
        );
        assert_eq!(book.bids(), &before);
    }

    #[test]
    fn crossed_snapshot_is_rejected() {
        let err = Book::from_snapshot(
            Venue::Binance,
            btc_usdt(),
            1,
            Timestamp::EPOCH,
            Timestamp::EPOCH,
            [level(101, 1)],
            [level(100, 1)],
        )
        .expect_err("crossed");
        assert_eq!(
            err,
            CoreError::CrossedBook {
                best_bid: price(101),
                best_ask: price(100)
            }
        );
    }

    #[test]
    fn locked_book_counts_as_crossed() {
        let mut book = snapshot(10, &[(99, 1)], &[(101, 1)]);
        let err = book
            .apply(&delta(11, 11, &[(Side::Bid, 101, 1)]))
            .expect_err("locked");
        assert!(matches!(err, CoreError::CrossedBook { .. }));
    }

    // -- Sequencing ----------------------------------------------------------

    #[test]
    fn contiguous_delta_advances_the_sequence_and_timestamps() {
        let mut book = snapshot(10, &[(99, 1)], &[(101, 1)]);
        assert_eq!(
            book.apply(&delta(11, 13, &[(Side::Ask, 101, 5)])),
            Ok(ApplyOutcome::Applied)
        );
        assert_eq!(book.last_update_id(), 13);
        assert_eq!(book.ts_exchange(), Timestamp::from_unix_millis(13));
        assert_eq!(book.ts_local(), Timestamp::from_unix_millis(14));
        assert_eq!(book.asks().qty_at(price(101)), Some(qty(5)));
    }

    // Binance's sync procedure: the first diff after a snapshot with
    // lastUpdateId = 100 may be U = 95, u = 105. It straddles the snapshot
    // and must be applied, not rejected as a gap.
    #[test]
    fn overlapping_delta_is_applied() {
        let mut book = snapshot(100, &[(99, 1)], &[(101, 1)]);
        assert_eq!(
            book.apply(&delta(95, 105, &[(Side::Bid, 99, 2)])),
            Ok(ApplyOutcome::Applied)
        );
        assert_eq!(book.last_update_id(), 105);
    }

    #[test]
    fn stale_delta_is_ignored_and_leaves_the_book_untouched() {
        let mut book = snapshot(100, &[(99, 1)], &[(101, 1)]);
        let before = book.clone();
        assert_eq!(
            book.apply(&delta(90, 100, &[(Side::Bid, 99, 7)])),
            Ok(ApplyOutcome::Stale)
        );
        assert_eq!(book, before);
    }

    #[test]
    fn gap_is_an_error_and_leaves_the_book_untouched() {
        let mut book = snapshot(100, &[(99, 1)], &[(101, 1)]);
        let before = book.clone();
        assert_eq!(
            book.apply(&delta(102, 103, &[(Side::Bid, 99, 7)])),
            Err(CoreError::SequenceGap {
                expected: 101,
                got: 102
            })
        );
        assert_eq!(book, before);
    }

    #[test]
    fn inverted_update_range_is_rejected() {
        let mut book = snapshot(100, &[(99, 1)], &[(101, 1)]);
        assert_eq!(
            book.apply(&delta(105, 104, &[])),
            Err(CoreError::InvalidUpdateRange {
                first: 105,
                last: 104
            })
        );
    }

    #[test]
    fn delta_for_another_book_is_rejected() {
        let mut book = snapshot(100, &[(99, 1)], &[(101, 1)]);
        let mut wrong = delta(101, 101, &[(Side::Bid, 99, 7)]);
        wrong.venue = Venue::Bybit;
        let before = book.clone();
        let err = book.apply(&wrong).expect_err("wrong venue");
        assert!(matches!(err, CoreError::BookMismatch { .. }));
        assert_eq!(book, before);
    }

    #[test]
    fn book_event_reports_its_venue_and_symbol() {
        let event = BookEvent::Delta(delta(1, 1, &[]));
        assert_eq!(event.venue(), Venue::Binance);
        assert_eq!(event.symbol(), btc_usdt());
        let event = BookEvent::Snapshot(snapshot(1, &[], &[]));
        assert_eq!(event.venue(), Venue::Binance);
    }

    // -- Q7: the operation that needs `range` --------------------------------

    // Band depth: everything resting within a price limit, regardless of how
    // many levels that is. A top-N walk only needs ordered iteration; this
    // query needs a bounded slice by *price*, which is what `range` gives.
    #[test]
    fn q7_band_depth_uses_range_on_both_sides() {
        let book = snapshot(
            1,
            &[(100, 1), (99, 2), (98, 4), (90, 8)],
            &[(101, 1), (102, 2), (103, 4), (110, 8)],
        );

        let ask_band: Vec<_> = book.asks().within(price(102)).collect();
        assert_eq!(ask_band, [level(101, 1), level(102, 2)]);

        let bid_band: Vec<_> = book.bids().within(price(99)).collect();
        assert_eq!(bid_band, [level(100, 1), level(99, 2)]);

        let total = bid_band
            .iter()
            .try_fold(Qty::ZERO, |acc, l| acc.checked_add(l.qty))
            .expect("fits");
        assert_eq!(total, qty(3));
    }

    #[test]
    fn q7_band_limit_worse_than_the_whole_side_yields_everything() {
        let book = snapshot(1, &[(100, 1), (99, 2)], &[(101, 1), (102, 2)]);
        assert_eq!(book.bids().within(price(1)).count(), 2);
        assert_eq!(book.asks().within(price(1_000)).count(), 2);
    }

    #[test]
    fn q7_band_limit_better_than_the_touch_yields_nothing() {
        let book = snapshot(1, &[(100, 1)], &[(101, 1)]);
        assert_eq!(book.bids().within(price(105)).count(), 0);
        assert_eq!(book.asks().within(price(95)).count(), 0);
    }

    // -- Q8: why the bid key is not `Reverse<Price>` -------------------------

    // With `Reverse` keys, "bids between 99 and 101" written the natural way
    // has start > end in key order, and `BTreeMap::range` panics on that.
    // This is the six-months-later misread Q8 asked about, made concrete.
    #[test]
    #[should_panic(expected = "range start is greater than range end")]
    fn q8_reverse_keyed_range_with_natural_bounds_panics() {
        let mut bids: BTreeMap<Reverse<Price>, Qty> = BTreeMap::new();
        bids.insert(Reverse(price(100)), qty(1));
        let _ = bids.range(Reverse(price(99))..=Reverse(price(101))).count();
    }

    // The same query on a plain-keyed BookSide reads the same on either side.
    #[test]
    fn q8_plain_keys_take_natural_bounds_on_both_sides() {
        let book = snapshot(1, &[(100, 1), (99, 1), (95, 1)], &[(101, 1)]);
        assert_eq!(
            prices(book.bids().within(price(99))),
            [price(100), price(99)]
        );
    }

    // -- Properties ------------------------------------------------------------

    /// A delta as the generator sees it: level changes, how many update ids it
    /// spans, and how far it reaches back before the book's current id.
    #[derive(Clone, Debug)]
    struct GenDelta {
        updates: Vec<(Side, i64, i64)>,
        span: u64,
        reach_back: u64,
    }

    /// Bids live in 1..=50 and asks in 51..=100, so no legal sequence can
    /// cross the book. Quantity 0 (a delete) is drawn often on purpose.
    fn non_crossing_update() -> impl Strategy<Value = (Side, i64, i64)> {
        prop_oneof![
            (1i64..=50, 0i64..=3).prop_map(|(p, q)| (Side::Bid, p, q)),
            (51i64..=100, 0i64..=3).prop_map(|(p, q)| (Side::Ask, p, q)),
        ]
    }

    fn gen_delta() -> impl Strategy<Value = GenDelta> {
        (
            prop::collection::vec(non_crossing_update(), 0..8),
            0u64..4,
            0u64..6,
        )
            .prop_map(|(updates, span, reach_back)| GenDelta {
                updates,
                span,
                reach_back,
            })
    }

    fn assert_strictly_monotonic(side: &BookSide) {
        let ps = prices(side.levels());
        for pair in ps.windows(2) {
            assert!(
                side.side().is_better(pair[0], pair[1]),
                "{} levels out of order: {:?}",
                side.side(),
                ps
            );
        }
    }

    proptest! {
        // Model-based check: after any legal sequence of contiguous,
        // overlapping and stale deltas, the book matches a plain map that
        // applied the same updates, both sides are strictly ordered best
        // first, no zero level is stored, and the sequence never goes back.
        #[test]
        fn legal_delta_sequences_preserve_every_invariant(
            start in 0u64..1_000,
            deltas in prop::collection::vec(gen_delta(), 0..40),
        ) {
            let mut book = snapshot(start, &[], &[]);
            let mut model: BTreeMap<(Side, i64), i64> = BTreeMap::new();

            for d in deltas {
                let current = book.last_update_id();
                // reach_back > span produces a fully stale delta; otherwise
                // the delta overlaps or is contiguous and must apply.
                let first = (current + 1).saturating_sub(d.reach_back);
                let last = first + d.span;
                let outcome = book.apply(&delta(first, last, &d.updates));

                if last <= current {
                    prop_assert_eq!(outcome, Ok(ApplyOutcome::Stale));
                } else {
                    prop_assert_eq!(outcome, Ok(ApplyOutcome::Applied));
                    for &(side, p, q) in &d.updates {
                        if q == 0 {
                            model.remove(&(side, p));
                        } else {
                            model.insert((side, p), q);
                        }
                    }
                }

                prop_assert!(book.last_update_id() >= current);
                assert_strictly_monotonic(book.bids());
                assert_strictly_monotonic(book.asks());
                prop_assert!(book.bids().levels().all(|l| !l.qty.is_zero()));
                prop_assert!(book.asks().levels().all(|l| !l.qty.is_zero()));

                let stored: BTreeMap<(Side, i64), i64> = [book.bids(), book.asks()]
                    .into_iter()
                    .flat_map(|side| {
                        side.levels().map(move |l| {
                            let key = (side.side(), l.price.scaled() / SCALE_FACTOR);
                            (key, l.qty.scaled() / SCALE_FACTOR)
                        })
                    })
                    .collect();
                prop_assert_eq!(&stored, &model);
            }
        }

        // With prices drawn across the whole range the book can cross. The
        // contract is that `apply` never reports success on a crossed book,
        // and only reports CrossedBook when it really is crossed.
        #[test]
        fn apply_never_reports_success_on_a_crossed_book(
            updates in prop::collection::vec(
                (prop_oneof![Just(Side::Bid), Just(Side::Ask)], 1i64..=20, 0i64..=2),
                0..12,
            ),
        ) {
            let mut book = snapshot(0, &[], &[]);
            let result = book.apply(&delta(1, 1, &updates));
            let crossed = matches!(
                (book.best_bid(), book.best_ask()),
                (Some(b), Some(a)) if b.price >= a.price
            );
            match result {
                Ok(ApplyOutcome::Applied) => prop_assert!(!crossed),
                Err(CoreError::CrossedBook { .. }) => prop_assert!(crossed),
                other => prop_assert!(false, "unexpected outcome {:?}", other),
            }
        }

        // A gap must never mutate the book, whatever the delta contains.
        #[test]
        fn a_gap_never_mutates_the_book(
            start in 0u64..1_000,
            skip in 2u64..50,
            updates in prop::collection::vec(non_crossing_update(), 0..8),
        ) {
            let mut book = snapshot(start, &[(10, 1)], &[(60, 1)]);
            let before = book.clone();
            let first = start + skip;
            let result = book.apply(&delta(first, first, &updates));
            prop_assert_eq!(
                result,
                Err(CoreError::SequenceGap { expected: start + 1, got: first })
            );
            prop_assert_eq!(book, before);
        }
    }
}
