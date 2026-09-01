# 00NN - short title, stated as the decision

- **Status:** Proposed
- **Date:** YYYY-MM-DD
- **Affects:** `crates/...`

## Context

What forces this choice, in concrete terms. I name the real constraints here: a
venue's actual payload format, a precision requirement, a latency budget. I state
numbers where I have numbers. If I have done this well, the decision reads as
inevitable to someone who has never seen my code.

## Options considered

### A - first option

How it works. What it costs me.

### B - second option

How it works. What it costs me.

## Decision

The option I chose, in one sentence, in the present tense: "I store prices as ...".

## Why I rejected the others

The specific property that disqualified each option I turned down. If I rejected
one on taste rather than on a hard constraint, I say so plainly. That is honest,
and it is the part I will want to reread later.

## Consequences

What this makes easy for me, what it makes hard, and what now has to hold
elsewhere in the codebase. I list the tests that pin the invariant.

## I should revisit this if

The conditions that would make me reopen it: a new venue, a precision requirement
this cannot meet, a benchmark result.
