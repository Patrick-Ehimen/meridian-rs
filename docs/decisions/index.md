# Decision log

My Architecture Decision Records. One file per decision, numbered in the order I
*made* them, never renumbered. Once I mark a record `Accepted` I treat it as
immutable. If I change my mind later I write a new ADR and mark the old one
`Superseded by 00NN`, so the reasoning I had at the time survives.

I copy [the template](0000-template.md) to start one:

```bash
cp docs/decisions/0000-template.md docs/decisions/00NN-short-slug.md
```

Then I add it to `docs/SUMMARY.md`. mdBook builds from that file, so an ADR I
forget to list there will not appear in the book.

## What earns an ADR

I write one when the choice was genuinely contested and the option I rejected was
reasonable: numeric representation, threading and ownership model, how I handle
sequence gaps, where normalization happens, what "depth" means for each venue
class.

Not every commit needs one. If there was only ever one sane option, it belongs in
[learnings](../learnings/index.md) instead.

## Status values

| Status | What it means |
| --- | --- |
| `Proposed` | I have written it down but not committed to it in code |
| `Accepted` | In force. My codebase reflects it |
| `Superseded by 00NN` | I replaced it, but kept it for the historical reasoning |

## Index

| # | Decision | Status |
| --- | --- | --- |
| [0001](0001-core-types.md) | Core price & quantity types | Proposed |
| [0002](0002-book-storage.md) | Order book side storage | Proposed |
