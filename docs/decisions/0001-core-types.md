# 0001 - Core price & quantity types

- **Status:** Proposed. I have not decided yet, see [Open questions](../notes/questions.md)
- **Date:** 2026-08-24
- **Affects:** `crates/core`

## Context

The constraint that should drive my choice: `Price` has to hold Binance's
2-decimal USDT quotes and Uniswap V3's `sqrtPriceX96` (Q64.96) without loss. I
need to work out what precision Q64.96 actually demands before I pick. That is
the whole decision.

## Options considered

_Not yet written._

## Decision

_Not yet made._

## Why I rejected the others

_Pending._

## Consequences

_Pending._
