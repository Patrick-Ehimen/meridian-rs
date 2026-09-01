# meridian-rs

Multi-exchange orderbook aggregator built in Rust. Providing CEX + DEX feeds, consolidated depth, arbitrage detection, and a live terminal UI.

Ingests real-time book and trade data from Binance, Coinbase, Bybit, Uniswap V3
and Jupiter, normalizes it into one internal representation, and derives
consolidated depth, VWAP, order-flow imbalance and arbitrage signals. It serves
the result over gRPC, REST, WebSocket and a `ratatui` terminal UI.

> **Status: early.** The Cargo workspace, CI and documentation are in place. The
> core domain types are still being designed. See the
> [decision log](docs/decisions/index.md). No venue adapter is implemented yet.

## Layout

| Path | What lives there |
| --- | --- |
| [crates/core/](crates/core/) | Shared domain types every other crate speaks |
| [crates/venues/](crates/venues/) | One adapter per venue, behind a common `VenueFeed` trait |
| [crates/aggregator/](crates/aggregator/) | Consolidated depth, VWAP, OFI |
| [crates/server/](crates/server/) | gRPC, REST and WebSocket surfaces over one shared state |
| [crates/tui/](crates/tui/) | Terminal UI |
| [docs/](docs/) | Engineering notebook: decisions, learnings, open questions |

Venue-specific parsing stays inside its adapter; nothing downstream ever sees a
venue's native payload shape.

## Build

```bash
cargo build
cargo test --workspace
cargo ci            # fmt --check + clippy -D warnings + test (see .cargo/config.toml)
```

## Docs

The engineering notebook is an mdBook. It records the reasoning the source cannot
carry: architecture decision records, confirmed venue quirks, and unresolved
questions.

```bash
mdbook serve --open   # live-reload on :3000
mdbook build          # static output into ./book (gitignored)
```

Start at [docs/index.md](docs/index.md).

## License

[MIT](LICENSE)
