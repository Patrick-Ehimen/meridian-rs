# Q12 - DashMap iteration order and consolidated VWAP

- **Blocks:** Aggregation. Also feeds back into [ADR 0001](../../decisions/0001-core-types.md) via the arithmetic-associativity argument.
- **Status:** Open.
- **Codified in:** not yet.

## Question

The `DashMap` iteration order is not deterministic. If consolidated VWAP sums
per-venue notionals in map iteration order, does the result vary run to run?
With exact arithmetic addition is associative and the answer is no, which is
an argument that belongs in ADR 0001. Confirm the dependency explicitly.

## Approach

_Not yet framed._

## Working notes

_Empty._

## Resolution

_Pending._
