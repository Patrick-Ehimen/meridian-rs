# Q5 - Does rust_decimal hash equal for equal values?

- **Blocks:** [ADR 0001 - Core price & quantity types](../../decisions/0001-core-types.md), conditionally.
- **Status:** Resolved.
- **Codified in:** `crates/core/src/rust_decimal_hash_check.rs`.

## Question

If I go with `rust_decimal`: does `Decimal::from_str("1.10")` hash equal to
`Decimal::from_str("1.1")`? They compare equal. If the hashes differ, using
`Price` as a `HashMap` key is a latent bug. Verify, do not assume. It is a
five-line test.

## Approach

Only load-bearing if ADR 0001 keeps `rust_decimal` on the table. Q1-Q4
lean toward integer storage (options A1 or A2), in which case this check
documents whether the fallback is safe rather than gating the decision.

Rust's `Hash` contract: `a == b` implies `hash(a) == hash(b)`. If `Decimal`
violates it, then `HashMap<Decimal, _>` silently loses entries when a
lookup uses a differently-scaled but equal-valued key. That is the exact
"latent bug" the question calls out.

Method: write the five-line test. Also check a broader set of
representations (`1`, `1.0`, `1.00`, `1.000000000000`), and demonstrate
the concrete HashMap failure mode a violation would produce.

## Working notes

### Test result (rust_decimal 1.43.0)

`rust_decimal 1.43.0` **honours the Hash contract**.

- `Decimal::from_str("1.10")` (scale 2) and `Decimal::from_str("1.1")`
  (scale 1) compare equal AND hash equal (both hash to
  `14756677409293871304`).
- Four variants of `1` (`"1"`, `"1.0"`, `"1.00"`, `"1.000000000000"`)
  all hash to the same value (`9886797495896358906`).
- A `HashMap<Decimal, _>` inserted with the `"1.10"` spelling can be
  looked up successfully with the `"1.1"` spelling.

### Codified in tests

`crates/core/src/rust_decimal_hash_check.rs` has three tests:

- `equal_decimals_must_hash_equal` - the direct check on the two
  representations named in the question.
- `multiple_representations_of_one_hash_equal` - broader coverage across
  scales 0-12 for a value of 1.
- `hashmap_lookup_survives_equal_variants` - user-visible failure mode
  the hash contract exists to prevent.

Added `rust_decimal = "1"` as a `dev-dependency` on `crates/core`. Not
promoted to a real dependency because Q1-Q4 do not require it yet.

### A historical note

Earlier versions of `rust_decimal` did have this bug (Hash used the raw
internal representation without normalising for scale). It was fixed
upstream, and 1.43.0 is safe. If ADR 0001 does adopt `rust_decimal`,
the version pin should sit at 1.43 or newer.

## Resolution

**`rust_decimal 1.43.0` is safe as a `HashMap` key.** Equal-valued
Decimals with different scales produce identical hashes, and HashMap
round-trip works across scale-preserving spellings.

**What this pins down for ADR 0001.**

- Option D (Q2's `rust_decimal`-based `Price` representation) is no
  longer disqualified by this concern.
- The remaining trade-offs for D versus A1/A2 sit on **speed** and
  **binary size**: integer storage is faster and simpler than decimal
  arithmetic, but D avoids picking a fixed `N` at all.
- If ADR 0001 does adopt D, pin `rust_decimal >= 1.43` in the workspace
  Cargo.toml so a future patch bump cannot regress the fix.
