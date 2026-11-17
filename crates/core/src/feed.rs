//! The venue adapter interface.
//!
//! Per [ADR 0003], every adapter implements [`VenueFeed`]: one `run` call is
//! one session (connect, snapshot, stream) that pushes [`VenueEvent`]s into a
//! bounded per-venue channel and returns when the session ends. Reconnecting
//! is not the adapter's job. The adapter returns a [`FeedError`], and whoever
//! drives it (the supervisor, step 12) decides from
//! [`FeedError::is_fatal`] whether to start another session.
//!
//! # Stream contract
//!
//! Within one session, an adapter sends:
//!
//! 1. `Status(Connecting)` before anything else.
//! 2. A `Book(Snapshot)` for a symbol before any `Book(Delta)` for it.
//! 3. `Status(Live)` once every subscribed symbol has a snapshot.
//! 4. Only deltas that its own [`Book::apply`](crate::Book::apply) accepted.
//!    Gap detection happens in the adapter, because only the adapter can
//!    recover from a gap (Q10).
//! 5. On a gap it can repair in place: `Status(Resnapping { symbol })`, then
//!    a fresh snapshot for that symbol. Otherwise it returns a transient
//!    error and the session ends.
//!
//! [`run_session`] adds the last event of every session,
//! `Status(Disconnected)`, so consumers learn the venue stopped even when
//! the adapter itself could not say so.
//!
//! [ADR 0003]: ../../../docs/decisions/0003-venue-feed-shape.md

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::book::BookEvent;
use crate::error::CoreError;
use crate::symbol::Symbol;
use crate::trade::Trade;
use crate::venue::Venue;

/// Default capacity of a per-venue feed channel.
///
/// Large enough to absorb a burst (a Binance depth stream at 100 ms across a
/// handful of symbols is tens of messages a second), small enough that a
/// stalled consumer shows up as a blocked adapter within seconds rather than
/// as memory growth. See Q11.
pub const DEFAULT_FEED_CAPACITY: usize = 4096;

/// Connection state, surfaced so the aggregator can stop trusting a venue's
/// book while it is being rebuilt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnStatus {
    /// A session is starting. Any book state held for this venue from an
    /// earlier session is stale until the next snapshot for each symbol.
    Connecting,
    /// Every subscribed symbol has a current snapshot.
    Live,
    /// The book for `symbol` is stale; a snapshot for it follows.
    Resnapping { symbol: Symbol },
    /// The session ended. `fatal` mirrors [`FeedError::is_fatal`]: if true,
    /// no further session will be attempted.
    Disconnected { reason: String, fatal: bool },
}

/// Everything an adapter emits, on one ordered stream.
///
/// One stream rather than three because ordering across kinds matters: OFI
/// needs to know a trade printed after a particular book update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VenueEvent {
    Book(BookEvent),
    Trade(Trade),
    Status(ConnStatus),
}

impl VenueEvent {
    /// The venue named inside the event, or `None` for a status, which is
    /// attributed by the channel it arrives on.
    pub fn venue(&self) -> Option<Venue> {
        match self {
            Self::Book(event) => Some(event.venue()),
            Self::Trade(trade) => Some(trade.venue),
            Self::Status(_) => None,
        }
    }
}

/// Why a session ended, as seen across the adapter boundary.
///
/// Each adapter converts its private errors (JSON, WebSocket, HTTP, RPC)
/// into one of these with a short reason string. Nothing crosses the
/// boundary as `Box<dyn Error>`. Whether an error is fatal is decided by its
/// variant, in one exhaustive match in [`FeedError::is_fatal`], so adding a
/// variant forces the decision.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum FeedError {
    /// The transport dropped: TCP reset, WebSocket close, timeout, HTTP 5xx.
    #[error("{venue}: disconnected: {reason}")]
    Disconnected { venue: Venue, reason: String },

    /// A payload could not be parsed or made no sense. Treated as transient:
    /// a fresh session with a fresh snapshot is the recovery.
    #[error("{venue}: protocol error: {reason}")]
    Protocol { venue: Venue, reason: String },

    /// A core book invariant failed: a sequence gap the adapter chose not to
    /// repair in place, or a crossed book. A new session resnaps.
    #[error("{venue}: book invariant violated: {source}")]
    Book {
        venue: Venue,
        #[source]
        source: CoreError,
    },

    /// The venue refused us: bad credentials, rejected subscription, unknown
    /// symbol. Retrying cannot help until configuration changes.
    #[error("{venue}: rejected: {reason}")]
    Rejected { venue: Venue, reason: String },

    /// The receiving side of the feed channel was dropped. Nobody is
    /// listening, so there is nothing to reconnect for.
    #[error("{venue}: consumer closed the feed channel")]
    ConsumerClosed { venue: Venue },

    /// An event for one venue was sent on another venue's channel. That is
    /// an adapter bug, and delivering it would corrupt the wrong book.
    #[error("{sink} feed was sent an event for {event}")]
    WrongVenue { sink: Venue, event: Venue },
}

impl FeedError {
    /// True if starting another session cannot succeed without a code or
    /// configuration change.
    pub const fn is_fatal(&self) -> bool {
        match self {
            Self::Disconnected { .. } | Self::Protocol { .. } | Self::Book { .. } => false,
            Self::Rejected { .. } | Self::ConsumerClosed { .. } | Self::WrongVenue { .. } => true,
        }
    }

    /// The venue whose session produced this error.
    pub const fn venue(&self) -> Venue {
        match self {
            Self::Disconnected { venue, .. }
            | Self::Protocol { venue, .. }
            | Self::Book { venue, .. }
            | Self::Rejected { venue, .. }
            | Self::ConsumerClosed { venue } => *venue,
            Self::WrongVenue { sink, .. } => *sink,
        }
    }
}

/// The sending half of a per-venue feed channel, handed to an adapter.
///
/// Sends wait when the channel is full: backpressure blocks the adapter's
/// reader rather than dropping data (Q11). A venue that outruns its consumer
/// eventually disconnects loudly, which is recoverable; a silently dropped
/// delta is not.
#[derive(Clone, Debug)]
pub struct FeedSink {
    venue: Venue,
    tx: mpsc::Sender<VenueEvent>,
}

impl FeedSink {
    pub const fn venue(&self) -> Venue {
        self.venue
    }

    /// Sends one event, waiting for capacity.
    ///
    /// # Errors
    ///
    /// - [`FeedError::WrongVenue`] if the event names another venue.
    /// - [`FeedError::ConsumerClosed`] if the receiver was dropped.
    pub async fn send(&self, event: VenueEvent) -> Result<(), FeedError> {
        if let Some(event_venue) = event.venue()
            && event_venue != self.venue
        {
            return Err(FeedError::WrongVenue {
                sink: self.venue,
                event: event_venue,
            });
        }
        self.tx
            .send(event)
            .await
            .map_err(|_| FeedError::ConsumerClosed { venue: self.venue })
    }

    /// Sends a status event. Shorthand for `send(VenueEvent::Status(..))`.
    ///
    /// # Errors
    ///
    /// As [`FeedSink::send`].
    pub async fn status(&self, status: ConnStatus) -> Result<(), FeedError> {
        self.send(VenueEvent::Status(status)).await
    }
}

/// The receiving half of a per-venue feed channel.
#[derive(Debug)]
pub struct FeedReceiver {
    venue: Venue,
    rx: mpsc::Receiver<VenueEvent>,
}

impl FeedReceiver {
    pub const fn venue(&self) -> Venue {
        self.venue
    }

    /// The next event, or `None` once every sink is dropped and the channel
    /// is drained.
    pub async fn recv(&mut self) -> Option<VenueEvent> {
        self.rx.recv().await
    }
}

/// A bounded channel for one venue's events.
///
/// One channel per venue, never shared, so a flooding venue fills only its
/// own buffer and an idle venue is unaffected (Q11). A `capacity` of zero is
/// raised to one, since `tokio::sync::mpsc` panics on zero.
pub fn feed_channel(venue: Venue, capacity: usize) -> (FeedSink, FeedReceiver) {
    let (tx, rx) = mpsc::channel(capacity.max(1));
    (FeedSink { venue, tx }, FeedReceiver { venue, rx })
}

/// A venue adapter.
///
/// Dyn-compatible, so the aggregator can hold `Vec<Box<dyn VenueFeed>>`.
/// That is why this uses `async-trait`: a native `async fn` in a trait is not
/// dyn-compatible, and `async-trait` gets there by boxing the returned future
/// (one allocation per `run` call, which happens once per session, not per
/// message).
#[async_trait]
pub trait VenueFeed: Send {
    /// The venue this adapter connects to. Must match the sink it is given.
    fn venue(&self) -> Venue;

    /// Runs one session for `symbols`, pushing events into `sink` per the
    /// stream contract in the module docs.
    ///
    /// Takes `&mut self` so a supervisor can call it again on the same
    /// adapter after a transient failure, keeping configuration and any
    /// venue-symbol mapping built at startup. The symbol set is fixed for a
    /// session; changing it means ending the session and starting another.
    ///
    /// # Errors
    ///
    /// Any [`FeedError`]. Returns `Ok(())` only when the venue ended the
    /// stream cleanly.
    async fn run(&mut self, symbols: &[Symbol], sink: &FeedSink) -> Result<(), FeedError>;
}

/// Runs one session and reports its end on the stream.
///
/// After `run` returns, sends `Status(Disconnected)` with the error text and
/// [`FeedError::is_fatal`] (or a non-fatal "session ended" on `Ok`), unless
/// the consumer is gone. The step 12 supervisor wraps this in its restart
/// loop with backoff.
///
/// # Errors
///
/// [`FeedError::WrongVenue`] without running if the feed and sink disagree
/// on the venue; otherwise whatever `run` returned.
pub async fn run_session(
    feed: &mut dyn VenueFeed,
    symbols: &[Symbol],
    sink: &FeedSink,
) -> Result<(), FeedError> {
    if feed.venue() != sink.venue() {
        return Err(FeedError::WrongVenue {
            sink: sink.venue(),
            event: feed.venue(),
        });
    }

    let result = feed.run(symbols, sink).await;
    let status = match &result {
        Ok(()) => ConnStatus::Disconnected {
            reason: "session ended".to_owned(),
            fatal: false,
        },
        Err(FeedError::ConsumerClosed { .. }) => return result,
        Err(err) => ConnStatus::Disconnected {
            reason: err.to_string(),
            fatal: err.is_fatal(),
        },
    };
    // Best effort. If this fails the consumer has gone, and the original
    // result is still the more useful thing to return.
    let _ = sink.status(status).await;
    result
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::testing::{MockFeed, Script};
    use crate::{Book, BookDelta, Timestamp};

    fn btc() -> Symbol {
        Symbol::new("BTC", "USDT").expect("valid")
    }

    fn snapshot(venue: Venue, id: u64) -> VenueEvent {
        let book =
            Book::from_snapshot(venue, btc(), id, Timestamp::EPOCH, Timestamp::EPOCH, [], [])
                .expect("empty book is not crossed");
        VenueEvent::Book(BookEvent::Snapshot(book))
    }

    fn delta(venue: Venue, id: u64) -> VenueEvent {
        VenueEvent::Book(BookEvent::Delta(BookDelta {
            venue,
            symbol: btc(),
            first_update_id: id,
            last_update_id: id,
            ts_exchange: Timestamp::EPOCH,
            ts_local: Timestamp::EPOCH,
            updates: Vec::new(),
        }))
    }

    /// A well-formed session: connecting, snapshot, live, then `n` deltas.
    fn session(venue: Venue, n: u64) -> Vec<Script> {
        let mut script = vec![
            Script::Event(VenueEvent::Status(ConnStatus::Connecting)),
            Script::Event(snapshot(venue, 0)),
            Script::Event(VenueEvent::Status(ConnStatus::Live)),
        ];
        script.extend((1..=n).map(|id| Script::Event(delta(venue, id))));
        script
    }

    async fn drain(rx: &mut FeedReceiver) -> Vec<VenueEvent> {
        let mut out = Vec::new();
        while let Some(event) = rx.recv().await {
            out.push(event);
        }
        out
    }

    fn scripted_events(script: &[Script]) -> Vec<VenueEvent> {
        script
            .iter()
            .filter_map(|s| match s {
                Script::Event(e) => Some(e.clone()),
                Script::Fail(_) => None,
            })
            .collect()
    }

    // -- Dyn compatibility -----------------------------------------------------

    #[tokio::test]
    async fn heterogeneous_feeds_fit_in_one_collection_and_spawn() {
        let mut feeds: Vec<Box<dyn VenueFeed>> = vec![
            Box::new(MockFeed::new(Venue::Binance, session(Venue::Binance, 1))),
            Box::new(MockFeed::new(Venue::Bybit, session(Venue::Bybit, 1))),
        ];
        let mut feed = feeds.remove(0);
        let (sink, mut rx) = feed_channel(Venue::Binance, 16);
        // Spawning proves Box<dyn VenueFeed> and its run future are Send.
        let handle = tokio::spawn(async move { run_session(feed.as_mut(), &[btc()], &sink).await });
        assert_eq!(handle.await.expect("no panic"), Ok(()));
        // Connecting, snapshot, live, one delta, then Disconnected.
        assert_eq!(drain(&mut rx).await.len(), 4 + 1);
    }

    // -- Delivery ----------------------------------------------------------------

    #[tokio::test]
    async fn every_scripted_event_arrives_in_order_then_disconnected() {
        let script = session(Venue::Binance, 10);
        let expected = scripted_events(&script);
        let mut feed = MockFeed::new(Venue::Binance, script);
        let (sink, mut rx) = feed_channel(Venue::Binance, 4);

        let consumer = tokio::spawn(async move { drain(&mut rx).await });
        assert_eq!(run_session(&mut feed, &[btc()], &sink).await, Ok(()));
        drop(sink);

        let mut got = consumer.await.expect("no panic");
        assert_eq!(
            got.pop(),
            Some(VenueEvent::Status(ConnStatus::Disconnected {
                reason: "session ended".to_owned(),
                fatal: false
            }))
        );
        assert_eq!(got, expected);
    }

    #[tokio::test]
    async fn mid_stream_error_reaches_the_consumer_after_prior_events() {
        let mut script = session(Venue::Binance, 2);
        let err = FeedError::Disconnected {
            venue: Venue::Binance,
            reason: "ws closed 1006".to_owned(),
        };
        script.push(Script::Fail(err.clone()));
        script.push(Script::Event(delta(Venue::Binance, 99)));
        let mut feed = MockFeed::new(Venue::Binance, script);
        let (sink, mut rx) = feed_channel(Venue::Binance, 16);

        assert_eq!(
            run_session(&mut feed, &[btc()], &sink).await,
            Err(err.clone())
        );
        drop(sink);

        let got = drain(&mut rx).await;
        assert_eq!(
            got.len(),
            5 + 1,
            "the event after the failure is never sent"
        );
        assert_eq!(
            got.last(),
            Some(&VenueEvent::Status(ConnStatus::Disconnected {
                reason: err.to_string(),
                fatal: false
            }))
        );
    }

    #[tokio::test]
    async fn fatal_error_is_flagged_fatal_on_the_stream() {
        let err = FeedError::Rejected {
            venue: Venue::Binance,
            reason: "invalid api key".to_owned(),
        };
        let mut feed = MockFeed::new(Venue::Binance, vec![Script::Fail(err.clone())]);
        let (sink, mut rx) = feed_channel(Venue::Binance, 16);

        assert_eq!(run_session(&mut feed, &[btc()], &sink).await, Err(err));
        drop(sink);
        assert!(matches!(
            drain(&mut rx).await.as_slice(),
            [VenueEvent::Status(ConnStatus::Disconnected {
                fatal: true,
                ..
            })]
        ));
    }

    #[tokio::test]
    async fn a_feed_can_run_again_after_a_transient_failure() {
        let mut script = session(Venue::Binance, 1);
        script.push(Script::Fail(FeedError::Disconnected {
            venue: Venue::Binance,
            reason: "reset".to_owned(),
        }));
        let mut feed = MockFeed::new(Venue::Binance, script);
        let (sink, _rx) = feed_channel(Venue::Binance, 64);

        for _ in 0..3 {
            let err = run_session(&mut feed, &[btc()], &sink)
                .await
                .expect_err("scripted failure");
            assert!(!err.is_fatal());
        }
        assert_eq!(feed.runs(), 3);
    }

    // -- Multiplexing ------------------------------------------------------------

    #[tokio::test]
    async fn two_feeds_multiplexed_keep_per_venue_order() {
        let a_script = session(Venue::Binance, 20);
        let b_script = session(Venue::Bybit, 20);
        let a_expected = scripted_events(&a_script);
        let b_expected = scripted_events(&b_script);

        let (a_sink, mut a_rx) = feed_channel(Venue::Binance, 2);
        let (b_sink, mut b_rx) = feed_channel(Venue::Bybit, 2);
        let mut a_feed = MockFeed::new(Venue::Binance, a_script);
        let mut b_feed = MockFeed::new(Venue::Bybit, b_script);
        tokio::spawn(async move { run_session(&mut a_feed, &[btc()], &a_sink).await });
        tokio::spawn(async move { run_session(&mut b_feed, &[btc()], &b_sink).await });

        let (mut a_got, mut b_got) = (Vec::new(), Vec::new());
        let (mut a_open, mut b_open) = (true, true);
        while a_open || b_open {
            tokio::select! {
                e = a_rx.recv(), if a_open => match e {
                    Some(e) => a_got.push(e),
                    None => a_open = false,
                },
                e = b_rx.recv(), if b_open => match e {
                    Some(e) => b_got.push(e),
                    None => b_open = false,
                },
            }
        }

        // Each stream ends with the session's Disconnected status.
        assert!(
            a_got
                .pop()
                .is_some_and(|e| matches!(e, VenueEvent::Status(_)))
        );
        assert!(
            b_got
                .pop()
                .is_some_and(|e| matches!(e, VenueEvent::Status(_)))
        );
        assert_eq!(a_got, a_expected);
        assert_eq!(b_got, b_expected);
    }

    // -- Backpressure and boundaries ---------------------------------------------

    // A full channel blocks the adapter; it never drops. Once the consumer
    // drains, the adapter resumes and every event arrives.
    #[tokio::test]
    async fn a_full_channel_blocks_the_feed_instead_of_dropping() {
        let script = session(Venue::Binance, 5);
        let expected = scripted_events(&script);
        let mut feed = MockFeed::new(Venue::Binance, script);
        let (sink, mut rx) = feed_channel(Venue::Binance, 1);

        let producer = tokio::spawn(async move { run_session(&mut feed, &[btc()], &sink).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !producer.is_finished(),
            "producer must wait on a full channel"
        );

        let mut got = drain(&mut rx).await;
        assert_eq!(producer.await.expect("no panic"), Ok(()));
        got.pop();
        assert_eq!(got, expected);
    }

    #[tokio::test]
    async fn dropped_consumer_is_a_fatal_error() {
        let mut feed = MockFeed::new(Venue::Binance, session(Venue::Binance, 1));
        let (sink, rx) = feed_channel(Venue::Binance, 16);
        drop(rx);

        let err = run_session(&mut feed, &[btc()], &sink)
            .await
            .expect_err("nobody listening");
        assert_eq!(
            err,
            FeedError::ConsumerClosed {
                venue: Venue::Binance
            }
        );
        assert!(err.is_fatal());
    }

    #[tokio::test]
    async fn sink_rejects_an_event_for_another_venue() {
        let (sink, _rx) = feed_channel(Venue::Binance, 16);
        assert_eq!(
            sink.send(delta(Venue::Coinbase, 1)).await,
            Err(FeedError::WrongVenue {
                sink: Venue::Binance,
                event: Venue::Coinbase
            })
        );
    }

    #[tokio::test]
    async fn feed_and_sink_must_agree_on_venue() {
        let mut feed = MockFeed::new(Venue::Bybit, session(Venue::Bybit, 1));
        let (sink, _rx) = feed_channel(Venue::Binance, 16);
        let err = run_session(&mut feed, &[btc()], &sink)
            .await
            .expect_err("mismatch");
        assert!(matches!(err, FeedError::WrongVenue { .. }));
        assert_eq!(feed.runs(), 0, "the feed must not be started");
    }

    #[test]
    fn zero_capacity_is_raised_instead_of_panicking() {
        let _ = feed_channel(Venue::Binance, 0);
    }

    // -- Error classification --------------------------------------------------

    #[test]
    fn transient_and_fatal_errors_are_classified() {
        let v = Venue::Binance;
        let r = || "x".to_owned();
        let transient = [
            FeedError::Disconnected {
                venue: v,
                reason: r(),
            },
            FeedError::Protocol {
                venue: v,
                reason: r(),
            },
            FeedError::Book {
                venue: v,
                source: CoreError::SequenceGap {
                    expected: 2,
                    got: 5,
                },
            },
        ];
        let fatal = [
            FeedError::Rejected {
                venue: v,
                reason: r(),
            },
            FeedError::ConsumerClosed { venue: v },
            FeedError::WrongVenue {
                sink: v,
                event: Venue::Bybit,
            },
        ];
        assert!(transient.iter().all(|e| !e.is_fatal()));
        assert!(fatal.iter().all(FeedError::is_fatal));
        assert!(transient.iter().chain(&fatal).all(|e| e.venue() == v));
    }

    #[test]
    fn error_text_names_the_venue_and_the_cause() {
        let err = FeedError::Book {
            venue: Venue::Binance,
            source: CoreError::SequenceGap {
                expected: 2,
                got: 5,
            },
        };
        assert_eq!(
            err.to_string(),
            "binance: book invariant violated: sequence gap: expected update 2, delta starts at 5"
        );
    }
}
