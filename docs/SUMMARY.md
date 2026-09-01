# Summary

[Introduction](index.md)

---

# Decisions

- [Decision log](decisions/index.md)
  - [0001 - Core price & quantity types](decisions/0001-core-types.md)
  - [0002 - Order book side storage](decisions/0002-book-storage.md)

# Learnings

- [Learnings](learnings/index.md)
  - [Venue wire formats](learnings/venue-wire-formats.md)

# Notes

- [Working notes](notes/index.md)
  - [Open questions](notes/questions.md)
    - [Q1 - Q64.96 precision demand](notes/questions/q01-q64-96-precision.md)
    - [Q2 - Low-price pair under a fixed 8-decimal scale](notes/questions/q02-low-price-pair-scale.md)
    - [Q3 - Notional overflow in the intermediate](notes/questions/q03-notional-overflow.md)
    - [Q4 - Should Price and Qty carry the same scale?](notes/questions/q04-shared-scale.md)
    - [Q5 - Does rust_decimal hash equal for equal values?](notes/questions/q05-rust-decimal-hash.md)
    - [Q6 - Symbol representation](notes/questions/q06-symbol-shape.md)
    - [Q7 - Which aggregator operation needs BTreeMap::range?](notes/questions/q07-btreemap-range.md)
    - [Q8 - Bid ordering idiom](notes/questions/q08-bid-ordering.md)
    - [Q9 - Coinbase l2update has no sequence number](notes/questions/q09-coinbase-no-sequence.md)
    - [Q10 - Where does gap detection live in the VenueFeed trait?](notes/questions/q10-venuefeed-gap-detection.md)
    - [Q11 - Backpressure contract across venues](notes/questions/q11-backpressure.md)
    - [Q12 - DashMap iteration order and consolidated VWAP](notes/questions/q12-dashmap-iteration.md)
    - [Q13 - What does consolidated depth mean when one leg is a Jupiter quote curve?](notes/questions/q13-consolidated-depth-jupiter.md)
    - [Q14 - Reconfirm venue wire formats against captured fixtures](notes/questions/q14-fixture-verification.md)

---

[ADR template](decisions/0000-template.md)
