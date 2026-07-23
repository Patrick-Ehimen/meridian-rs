# Git hooks

Repo-managed hooks. Not active by default — enable once per clone:

```
git config core.hooksPath .githooks
```

## Hooks

- `pre-commit` — runs `cargo fmt --check` and `cargo clippy -D warnings` on Rust changes.
- `pre-push` — runs `cargo test --workspace --all-features`.

Skip a single run with `git commit --no-verify` / `git push --no-verify` when you need to.
