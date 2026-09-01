# Learnings

Things that turned out to be true. Venue quirks, math I found easy to get wrong,
Rust patterns that earned their place, and mistakes I do not want to repeat.

Unlike my [decisions](../decisions/index.md), these are not numbered and not
immutable: I edit a learning freely as my understanding sharpens. Unlike my
[notes](../notes/index.md), a learning is something I have actually confirmed.

One file per topic, added to `docs/SUMMARY.md` under this section. Groupings I
expect to need as the project grows:

- `venues/` - per-exchange payload quirks, rate limits, resnap behaviour
- `tick-math.md` - Uniswap V3 `price = 1.0001^tick`, Q64.96, rounding traps
- `rust/` - async patterns, `DashMap` contention, supervised-task shapes

## Format

No template. A good learning answers three things:

1. **What I expected** - the belief I held going in.
2. **What actually happened** - with the payload, output, or benchmark that showed me.
3. **What that means for this codebase** - the concrete rule it gives me.

I cite evidence. A recorded fixture under `fixtures/` beats an API shape I only
half-remember.
