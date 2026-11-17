//! Test doubles for code that consumes venue feeds.
//!
//! Compiled for this crate's own tests and, for downstream crates, behind
//! the `testing` feature:
//!
//! ```toml
//! [dev-dependencies]
//! meridian-core = { path = "../core", features = ["testing"] }
//! ```

use async_trait::async_trait;

use crate::feed::{FeedError, FeedSink, VenueEvent, VenueFeed};
use crate::symbol::Symbol;
use crate::venue::Venue;

/// One step of a [`MockFeed`] script.
#[derive(Clone, Debug)]
pub enum Script {
    /// Send this event, waiting for channel capacity like a real adapter.
    Event(VenueEvent),
    /// End the session with this error. Later steps are not reached.
    Fail(FeedError),
}

/// A [`VenueFeed`] that replays a fixed script.
///
/// Every `run` replays the script from the start, so a supervisor test can
/// drive repeated sessions against the same instance and count them with
/// [`MockFeed::runs`]. The `symbols` argument is ignored; the script decides
/// what is sent.
#[derive(Clone, Debug)]
pub struct MockFeed {
    venue: Venue,
    script: Vec<Script>,
    runs: usize,
}

impl MockFeed {
    pub fn new(venue: Venue, script: Vec<Script>) -> Self {
        Self {
            venue,
            script,
            runs: 0,
        }
    }

    /// How many times `run` has been called.
    pub const fn runs(&self) -> usize {
        self.runs
    }
}

#[async_trait]
impl VenueFeed for MockFeed {
    fn venue(&self) -> Venue {
        self.venue
    }

    async fn run(&mut self, _symbols: &[Symbol], sink: &FeedSink) -> Result<(), FeedError> {
        self.runs += 1;
        for step in &self.script {
            match step {
                Script::Event(event) => sink.send(event.clone()).await?,
                Script::Fail(err) => return Err(err.clone()),
            }
        }
        Ok(())
    }
}
