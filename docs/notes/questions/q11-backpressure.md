# Q11 - Backpressure contract across venues

- **Blocks:** Ingestion.
- **Status:** Resolved.
- **Codified in:** `crates/core/src/feed.rs` (`feed_channel`, `FeedSink`, `DEFAULT_FEED_CAPACITY`, test `a_full_channel_blocks_the_feed_instead_of_dropping`).

## Question

What is the backpressure contract when one venue floods and another is idle?

## Approach

Answer three sub-questions in order: how many channels (shared or per
venue), bounded or not, and what a producer does when its channel is full.

## Working notes

### Shared channel or one per venue?

With one shared channel, a flooding venue fills it, and the idle venue's
next message waits behind the flood. Worse, if the shared channel blocks
producers, the flood blocks the idle venue's *reader*, and an idle WebSocket
that stops reading pings gets disconnected. One venue's burst would take
down another venue's connection, which breaks the CLAUDE.md isolation rule.

With one channel per venue, a flood fills only its own buffer. The idle
venue's channel is empty and its messages pass straight through.

### Bounded or unbounded?

Unbounded hides a stalled consumer until the process runs out of memory, and
the failure shows up far from its cause. Bounded turns the same stall into a
blocked sender, which is visible, local to one venue, and recoverable.
Bounded, always.

`DEFAULT_FEED_CAPACITY` is 4096. A Binance diff stream at 100 ms is 10
messages per second per symbol; a few symbols plus trades is tens per
second. 4096 is minutes of buffer for a short consumer hiccup, and still
small enough that a real stall blocks the sender quickly.

### Full channel: block or drop?

For the book stream, dropping is never acceptable: a dropped delta is
exactly the silent sequence gap CLAUDE.md forbids. So the sender awaits.

That pushes the stall back to the venue. The adapter stops reading its
socket, TCP buffers fill, and eventually the venue disconnects us. That ends
the session with a transient error, the supervisor starts a new one, and the
new one resnaps. Loud, bounded, self-healing.

Dropping with a counter is fine for a future debug tap that nobody relies
on. That tap gets its own channel, so its policy never touches the book
stream.

## Resolution

**One bounded `mpsc` channel per venue; a full channel blocks that venue's
adapter; nothing on the book stream is ever dropped.**

- A flooding venue backs up only into its own adapter. An idle venue is
  unaffected.
- A consumer that falls behind for long enough causes that venue to
  disconnect and resnap, which is a recoverable, visible event rather than
  silent drift or an OOM.
- `feed_channel(venue, capacity)` is the only way to build a feed channel,
  so the bound cannot be forgotten.

The test `a_full_channel_blocks_the_feed_instead_of_dropping` pins it:
with capacity 1 and no reader, the session does not finish; once the
reader drains, every event arrives in order.

This contract covers the adapter-to-aggregator hop only. Fan-out from the
aggregator to the servers (step 09) has different consumers and will need
its own answer, probably `broadcast` with `Lagged` handling.
