# 0003 - Venue adapters push one ordered event stream per session

- **Status:** Accepted
- **Date:** 2026-11-17
- **Affects:** `crates/core`, every crate under `crates/venues`, the supervisor (step 12)

## Context

Five adapters will implement this interface and the aggregator will consume
it, so its shape is expensive to change later. The forces:

- **Heterogeneous adapters in one place.** The aggregator holds N adapters
  of different types. A giant enum over adapter types would make core depend
  on every venue crate. It needs trait objects, so the trait must be
  dyn-compatible.
- **Reconnect is where connectors rot.** Every venue disconnects. If each
  adapter writes its own reconnect loop, there are five backoff
  implementations, and at least one will forget jitter.
- **Gaps must never be skipped** (CLAUDE.md), and only the adapter can
  recover from one ([Q10](../notes/questions/q10-venuefeed-gap-detection.md)).
- **One venue must not hurt another**, including through a shared queue
  ([Q11](../notes/questions/q11-backpressure.md)).
- **OFI needs cross-kind ordering**: whether a trade printed before or after
  a book update matters.
- **Errors split into two kinds**: retry (TCP reset, gap, malformed
  payload) and stop (bad credentials, rejected subscription).

## Options considered

### A - Pull: `async fn next_event(&mut self) -> Result<VenueEvent, FeedError>`

The consumer drives. Easy to test. But the adapter's socket is only read
while the consumer is inside `next_event`, so a slow aggregator pass stalls
pings and heartbeats for every venue it is not currently polling. Reconnect
logic ends up interleaved with the consumer's loop.

### B - Push, long-lived: adapter owns a task and reconnects internally

`fn start(self) -> Receiver<VenueEvent>`. Decoupled, the common connector
shape. But reconnect and backoff live in every adapter, and the consumer
can only learn about failures from status events the adapter chooses to
send.

### C - Push, one session per call: `async fn run(&mut self, symbols, sink) -> Result<(), FeedError>`

`run` connects, snapshots and streams into a bounded per-venue sink until
the session ends, then returns why. The caller (the supervisor) decides
from `FeedError::is_fatal` whether to call `run` again after a backoff.
Adapters contain no retry loop at all.

### Sub-choices

- **Async in a trait:** native `async fn` (not dyn-compatible) vs
  `async-trait` (boxes the future).
- **Stream shape:** one `VenueEvent` enum stream vs separate book, trade and
  status streams.
- **Error shape:** one `FeedError` enum in core vs an associated
  `type Error` per adapter.

## Decision

Adapters implement option C, using `async-trait` for dyn compatibility, and
push a single ordered `VenueEvent` stream into a bounded per-venue channel.
In `crates/core/src/feed.rs`:

```rust
#[async_trait]
pub trait VenueFeed: Send {
    fn venue(&self) -> Venue;
    async fn run(&mut self, symbols: &[Symbol], sink: &FeedSink) -> Result<(), FeedError>;
}

pub enum VenueEvent { Book(BookEvent), Trade(Trade), Status(ConnStatus) }
pub enum ConnStatus { Connecting, Live, Resnapping { symbol }, Disconnected { reason, fatal } }

pub enum FeedError {          // transient: Disconnected, Protocol, Book
    ...                       // fatal:     Rejected, ConsumerClosed, WrongVenue
}
impl FeedError { pub const fn is_fatal(&self) -> bool; }

pub fn feed_channel(venue: Venue, capacity: usize) -> (FeedSink, FeedReceiver);
pub async fn run_session(feed: &mut dyn VenueFeed, symbols: &[Symbol], sink: &FeedSink)
    -> Result<(), FeedError>;
```

The stream contract (Connecting first, a snapshot before any delta for a
symbol, Live once all are snapped, only deltas the adapter's own
`Book::apply` accepted, Resnapping then a snapshot on an in-place repair,
Disconnected last) is written in the `feed` module docs.

The supervisor restart loop is not part of this decision. Core provides
`run_session`, one session plus its closing `Disconnected` status; step 12
wraps it in backoff with jitter and decides which crate owns that loop.

## Why I rejected the others

- **A** couples socket liveness to consumer speed across all venues, the
  cross-venue interference Q11 rules out.
- **B** puts a reconnect loop in every adapter. C keeps adapters to "one
  session, then say why it ended", which is also what makes them testable
  without timers: a session is a finite script, as `MockFeed` shows.
- **Native `async fn` in traits** cannot be used through `dyn VenueFeed`.
  `async-trait` costs one boxed future per `run` call, which is once per
  session, not per message.
- **Separate streams** lose the ordering between a trade and the book
  update around it, which OFI needs.
- **Associated `type Error`** would make `Box<dyn VenueFeed>` impossible
  without fixing the error type anyway, and every consumer would need to
  classify each adapter's errors. One enum with one `is_fatal` match puts
  the classification in a single place. Each adapter converts its private
  errors into a variant with a short reason string, so no `Box<dyn Error>`
  crosses the boundary.

## Consequences

**What this makes easy.**

- Adapters are linear code: connect, subscribe, snapshot, loop over
  messages, return on the first problem. No retry state.
- The aggregator holds `Vec<Box<dyn VenueFeed>>` and one `FeedReceiver` per
  venue.
- Downstream tests run against `MockFeed` (`meridian-core` feature
  `testing`), with no network and no timers.
- Classifying a new error kind is a compile error until it is decided:
  `is_fatal` is an exhaustive match.

**What this makes hard.**

- Changing the symbol set means ending the session and starting another,
  which costs a resnap. Fine at startup-configured symbol lists; revisit if
  symbols are added at runtime.
- Each adapter holds its own `Book` per symbol for gap detection (Q10),
  duplicating the aggregator's copy.
- `FeedError` reasons are strings, so an adapter's detailed error is
  flattened at the boundary. Adapters should log the full error with
  `tracing` before converting.
- Core now depends on `tokio` (`mpsc`) and `async-trait` at runtime. Both
  were already declared dependencies; this is the first code to use them.

**Tests that pin it.** In `crates/core/src/feed.rs`:

- `heterogeneous_feeds_fit_in_one_collection_and_spawn` - dyn compatibility
  and `Send`.
- `every_scripted_event_arrives_in_order_then_disconnected`,
  `mid_stream_error_reaches_the_consumer_after_prior_events`,
  `fatal_error_is_flagged_fatal_on_the_stream`.
- `a_feed_can_run_again_after_a_transient_failure` - the session model.
- `two_feeds_multiplexed_keep_per_venue_order` - `select!` across venues.
- `a_full_channel_blocks_the_feed_instead_of_dropping`,
  `dropped_consumer_is_a_fatal_error` - Q11.
- `transient_and_fatal_errors_are_classified`.

## I should revisit this if

- **Symbols need adding at runtime** without a resnap of the others. Then a
  control channel into `run`, or one session per symbol.
- **Profiling shows a per-message cost** in the boxed future or the
  `mpsc` hop. Unlikely: the box is per session, and one channel hop per
  message is well under a microsecond.
- **Coinbase (Q9) turns out to need periodic forced resnaps.** That fits
  this shape (`Resnapping` then snapshot), but if it needs timer state the
  supervisor should own, the split between adapter and supervisor moves.
- **A venue needs multiple connections per session**, for example
  separate book and trade sockets. Then `run` merges them internally, and
  the cross-kind ordering guarantee weakens to "per socket"; OFI must know.
