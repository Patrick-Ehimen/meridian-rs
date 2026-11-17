# Q10 - Where does gap detection live in the VenueFeed trait?

- **Blocks:** Ingestion; VenueFeed trait design in step 02.
- **Status:** Resolved.
- **Codified in:** `crates/core/src/feed.rs` (stream contract in the module docs), `crates/core/src/book.rs` (`Book::apply`).

## Question

Does `VenueFeed` expose gap detection as a shared algorithm, or does each
adapter own its rule and report only the outcome upward? The four sequencing
schemes in the survey suggest the latter, but that is a trait design question
for step 02.

## Approach

Separate the two halves of gap handling and ask where each one can live:

1. **Translation:** turning a venue's sequencing scheme into "this message
   covers update ids `first..=last`".
2. **Detection:** deciding, given the book's last id, whether a message is
   stale, contiguous or a gap.
3. **Recovery:** resnapping. Whoever detects the gap has to be able to
   trigger this, or detection is useless.

## Working notes

### Translation is venue-specific

The [venue wire formats](../../learnings/venue-wire-formats.md) survey has
four schemes across five venues: Binance's `U`/`u` update-id range, Bybit's
per-topic `u` plus a service-wide `seq` (with `u == 1` as a restart sentinel
that must be treated as a fresh snapshot), Uniswap V3's block number, and
nothing at all on Coinbase `l2update` (Q9) or on polled Jupiter quotes. No
shared algorithm can read all of them. Each adapter has to map its scheme onto
`first_update_id` / `last_update_id`. That is the only venue-specific part.

### Detection is already shared

Once ids are translated, the rule is the same everywhere, and it already
exists: `Book::apply` sorts every delta into stale, contiguous/overlapping
or gap (ADR 0002). Writing it again per adapter would be four chances to get
the overlap case wrong.

### Recovery decides the location

The adapter owns the WebSocket and the REST snapshot endpoint. The
aggregator owns neither. If the aggregator detected a gap, it would need a
back channel to ask the adapter to resnap, and in the meantime it would hold
a book it knows is wrong. If the adapter detects it, recovery is a local
function call.

So the adapter keeps its own `Book` per symbol, calls `Book::apply` on each
translated delta, and forwards a delta downstream only after it applied
cleanly. On `SequenceGap` it either resnaps in place (emit
`Status(Resnapping { symbol })`, then a fresh snapshot) or returns
`FeedError::Book` and lets the supervisor start a new session.

### What the trait exposes

Only the outcome, as the question guessed. The trait has no gap-detection
method. The guarantee is in the stream contract: between `Live` and the next
`Resnapping` or `Disconnected`, every delta for a symbol is contiguous with
the last snapshot. The aggregator can still run `Book::apply` on what it
receives, but a `SequenceGap` there means an adapter bug, not a lost message.

## Resolution

**Each adapter owns translation; detection uses the shared `Book::apply`;
both happen inside the adapter, because only the adapter can recover.** The
trait exposes the outcome through `ConnStatus` (`Resnapping`, `Disconnected`)
and `FeedError::Book`, never the mechanism.

The cost is that the adapter holds a copy of each book it serves, and the
aggregator holds another. That is memory, not latency, and it buys a stream
that downstream code can trust without re-checking.
