# Q5 - Does rust_decimal hash equal for equal values?

- **Blocks:** [ADR 0001 - Core price & quantity types](../../decisions/0001-core-types.md)
- **Status:** Open.
- **Codified in:** not yet.

## Question

If I go with `rust_decimal`: does `Decimal::from_str("1.10")` hash equal to
`Decimal::from_str("1.1")`? They compare equal. If the hashes differ, using
`Price` as a `HashMap` key is a latent bug. Verify, do not assume. It is a
five-line test.

## Approach

_Not yet framed._

## Working notes

_Empty._

## Resolution

_Pending._
