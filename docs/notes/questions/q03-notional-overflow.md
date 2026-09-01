# Q3 - Notional overflow in the intermediate

- **Blocks:** [ADR 0001 - Core price & quantity types](../../decisions/0001-core-types.md)
- **Status:** Open.
- **Codified in:** not yet.

## Question

What is the largest `Price * Qty` notional I could see in a single level, in
scaled units? Does the **intermediate** fit in `i64`? In `i128`? Overflow in
the intermediate before the divide is the classic fixed-point bug, and it is
why `PrecisionOverflow` is on the core error list.

## Approach

_Not yet framed._

## Working notes

_Empty._

## Resolution

_Pending._
