# meridian-rs - engineering notebook

This is my notebook for meridian-rs, a multi-venue CEX + DEX orderbook aggregator:
real-time book and trade ingestion from Binance, Coinbase, Bybit, Uniswap V3 and
Jupiter, normalized into one internal representation and served over gRPC, REST,
WebSocket and a terminal UI.

I am not writing API documentation here; `cargo doc` covers that. I keep this book
to record the reasoning my source code cannot carry:

| Section | What I put there | When I write it |
| --- | --- | --- |
| [Decisions](decisions/index.md) | Numbered ADRs, one choice each, with the alternatives that lost | I picked between real options and the option I rejected was defensible |
| [Learnings](learnings/index.md) | Things that turned out to be true: venue quirks, tick math, Rust patterns | I was surprised, or I lost time to something non-obvious |
| [Notes](notes/index.md) | Scratch: open questions, half-formed ideas, things to revisit | I do not yet know enough to write either of the above |

My flow is `notes -> learnings -> decisions`. A note graduates once I understand
it, and a learning graduates once it forces a choice.

## Reading it

```bash
mdbook serve --open    # live-reload on :3000
mdbook build           # static output into ./book (gitignored)
```

## Ground rules I assume throughout

- Venue-specific parsing stays inside its adapter, behind my common `VenueFeed`
  trait, so quirks never leak into shared code.
- Sequence gaps force a resnap. I never silently continue past a dropped message.
- Uniswap V3 depth is *derived* from tick bitmap state. I never read it as a book.
- Consolidated depth is an analytical view, not tradeable liquidity.
- One venue failing must never take down the others.
