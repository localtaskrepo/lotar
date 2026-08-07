---
name: nextest-targeted-testing
description: Use this to run the Rust suite quickly with `cargo nextest`, starting targeted and widening to full CI parity.
---

## Fast loop

- Rust-only (CI-like profile): `npm run test:rust`

## Target a subset

NPM passes args through after `--`:

- Filter by substring (common case):
  - `npm run test:rust -- <substring>`

- If you need exact names, list tests:
  - `cargo nextest list --cargo-profile ci`

- If you need test binary args (e.g., `--nocapture`):
  - `cargo nextest run --cargo-profile ci <filter> -- --nocapture`

## Strategy

- Start as narrow as possible (one failing test), then widen.
- After fixing the immediate issue, run the full suite:
  - `npm test` (Rust + UI)
  - `npm run smoke`

## Policy

- Do NOT use `cargo test` in this repo.

## Sandboxed environments

- If `npm run test:rust` reports fewer tests than CI (~756 vs ~795), that's expected: `build.rs` sets `no_git_tests` when `.git` creation is blocked, compiling out git-dependent tests so they don't show as false negatives. CI runs the full set. See `smoke-suite-debugging` for details.
