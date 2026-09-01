# Q9 - Coinbase l2update has no sequence number

- **Blocks:** Ingestion; likely its own ADR once framed.
- **Status:** Open.
- **Codified in:** not yet.

## Question

Coinbase `l2update` carries no sequence number. My `CLAUDE.md` rule is that a
sequence gap forces a resnap and I never silently continue past a dropped
message. How do I honour that on a venue that gives me nothing to detect a gap
*with*?

Options to investigate:

- Rely on the transport's ordering guarantee and document the weaker invariant.
- Use the `full` channel, which does carry `sequence`.
- Periodically resnap on a timer and reconcile.

This needs its own ADR once I understand it.

## Approach

_Not yet framed._

## Working notes

_Empty._

## Resolution

_Pending._
