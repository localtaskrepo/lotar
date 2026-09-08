---
name: testing-strategy
description: Choose meaningful behavior checks and run targeted Rust, UI, or smoke tests before the required final gates; includes low-noise commands.
---

## Strategy

- Start with the affected behavior (one failing test or file), then run the
  applicable final gates in [AGENTS.md](../../../AGENTS.md). Repeat successful
  checks only for subsequent changes, failures, or unresolved risks.
- For a bug, reproduce its actual symptom with a focused failing test or command
  before fixing when feasible. Minimize inputs, test a falsifiable hypothesis,
  and remove temporary instrumentation. If reproduction is unavailable, state
  the evidence and limitation instead of blocking all useful analysis.
- Test observable behavior at an existing public boundary with independent
  expected values, not private implementation structure or tautological assertions.
  For test-first work, implement one failing behavior, make it pass, then proceed
  to the next slice. Ask about a testing boundary only when it changes the agreed
  design, not as a mandatory gate before every test.
- Pure documentation needs link/example/consistency checks. Run executable
  examples in isolated temporary workspaces. Embedded prompts and behavioral
  configuration need relevant behavior checks even when their files are Markdown.

## Rust (nextest)

- Full (CI-like profile): `npm run test:rust`
- Filter by substring: `npm run test:rust -- <substring>`
- List exact test names: `cargo nextest list --cargo-profile ci`
- Pass-through test-binary args (e.g. `--nocapture`): `cargo nextest run --cargo-profile ci <filter> -- --nocapture`
- Fast clippy-only loop (lib+bins, no `--all-targets`/`--all-features`): `npm run lint:backend:fast`
- Fresh worktree: run `npm run build:web` before compiling Rust; `include_dir!` requires the generated `target/web-embed` directory.

## UI unit tests (vitest)

- Full: `npm run test:ui`
- By name: `npm run test:ui -- -t "<substring>"`
- Single file: `npm run test:ui -- view/<path>/<file>.test.ts`
- Typecheck/lint frontend: `npm run lint:frontend`

## Smoke (Playwright + vitest harness)

- Full (builds first): `npm run smoke`
- Quick (no rebuild — assumes fresh artifacts): `npm run test:smoke:quick`
- By name: `npm run test:smoke:quick -- -t "<substring>"`
- By file: `npm run test:smoke:quick -- smoke/tests/<suite>.smoke.spec.ts`
- Serialized watch loop: `npx vitest watch --config smoke/vitest.config.ts --maxWorkers=1 --maxConcurrency=1 --no-file-parallelism`
- Install browsers if missing: `npm run playwright:install`

If smoke fails on environment/binary/server issues, switch to the `smoke-suite-debugging` skill.

## Low-noise output (agent / CI logs)

Prefer these commands with ANSI disabled and compact reporters:

- Lint: `npm run lint:agent`
- Rust + UI tests: `npm run test:agent`
- Rust-only: `npm run test:rust:agent`
- UI-only: `npm run test:ui:agent`
- Smoke (builds): `npm run test:smoke:agent` · quick: `npm run test:smoke:quick:agent`

These can satisfy the corresponding final gates when run without filters; do not
rerun the noisy variants solely for their names. One execution difference matters:
`test:rust:agent` sets `--retries 0`, overriding configured nextest retries. A pass
is sufficient; if it fails, inspect the failure and use the normal Rust command
when configured retry behavior is relevant. Report flaky recovery, not a clean
first pass. Quick smoke is for iteration with fresh artifacts, not a substitute
for the build-first final smoke gate.

Behavioral testing and reproduction techniques adapted from AI Hero's `tdd` and
`diagnosing-bugs`; see [sources and license](../THIRD_PARTY_NOTICES).
