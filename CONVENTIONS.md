# Writing conventions

The rules I hold myself to for everything I write into this repo: notebook pages,
doc comments, code comments, commit messages, and PR bodies.

## I do not use Unicode dashes

I never write these characters:

| Character | Name | Codepoint |
| --- | --- | --- |
| `—` | em dash | U+2014 |
| `–` | en dash | U+2013 |
| `―` | horizontal bar | U+2015 |
| `−` | minus sign | U+2212 |

I use a plain ASCII hyphen (`-`), or I restructure the sentence. I write without
them in the first place rather than search-replacing afterwards.

### Where I use a hyphen

Label and value pairs in lists, tables, and headings:

```markdown
- `WARN` - recovered, worth noticing.
- `venues/` - per-exchange payload quirks.
# 0001 - Core price & quantity types
```

Numeric ranges, unspaced:

```markdown
3-5 venues, ~5-10x slower, steps 01-11
```

Arithmetic, using ASCII `-` rather than U+2212:

```text
venue_book_lag_ms = ts_local - ts_exchange
OFI -= old bid qty
```

### Where I drop it instead

A mid-sentence aside almost always reads better once I recast it into two
sentences, or join it with a colon or semicolon:

| Instead of this | I write this |
| --- | --- |
| not API documentation - `cargo doc` covers that | not API documentation; `cargo doc` covers that |
| immutable once `Accepted` - if it changes, write a new one | immutable once `Accepted`. If it changes, I write a new one |
| name the real constraints - a venue's payload format | name the real constraints: a venue's payload format |

## Voice

I write these pages in the first person. This is my notebook, and the decisions,
mistakes, and open questions in it are mine. "I rejected `f64` because..." tells
me more when I reread it in six months than "`f64` was rejected because...".

## Enforcement

`scripts/check-dashes.sh` fails on any of the four characters:

```bash
./scripts/check-dashes.sh              # every git-tracked file
./scripts/check-dashes.sh FILE...      # specific files
```

My pre-commit hook runs it against staged files on every commit, including
markdown-only commits. Hooks are opt-in per clone:

```bash
git config core.hooksPath .githooks
```

This page and the script itself are allowlisted, since both have to contain the
characters to document and match them.

The check deliberately calls `command grep`. A plain `grep` may be wrapped by a
gitignore-aware tool, which silently skips untracked and ignored files and will
report a clean tree that is not clean.
