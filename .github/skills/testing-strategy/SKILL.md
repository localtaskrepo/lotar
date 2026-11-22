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
- Tests remove every temp file and folder they create. `npm run test:rust`,
  `test:ui` and the smoke suite run with a fresh temp folder and fail when
  anything is left in it (`scripts/rust-test-runner.mjs`, `scripts/vitest-temp-guard.mjs`;
  the vitest guard ignores Chromium's own `.org.chromium.Chromium.*` files).
- One full run per machine at a time (DEV-784): a full Rust, UI or smoke run
  and lotar's merge checks take the machine's check lock, port 47586
  (`scripts/check-lock.mjs`), and print `Waiting for the machine's check lock
  ..., held by <run>` while another holds it. That wait is not a hang. Runs
  filtered by file, test name or nextest filter don't wait, so iterate with them.
- Pure documentation needs link/example/consistency checks. Run executable
  examples in isolated temporary workspaces. Embedded prompts and behavioral
  configuration need relevant behavior checks even when their files are Markdown.

## Rust (nextest)

- Full (`--cargo-profile dev`, the same build `npm run build:smoke` makes; CI uses `ci`): `npm run test:rust` (runtime Git probe: default profile when Git works, else the `gitless` profile with reported `git_required` exclusions — see [tests README](../../../crates/lotar-lib/tests/README.md))
- Filter by substring: `npm run test:rust -- <substring>`
- Integration tests build into one binary (`crates/lotar-lib/tests/integration.rs`, DEV-302); each `crates/lotar-lib/tests/<file>.rs` is a module, so test names start with the file name. Select one file with `-E 'test(/^<file>::/)'`, and list new test files in `crates/lotar-lib/tests/integration.rs`.
- List exact test names: `cargo nextest list --cargo-profile dev`
- Pass-through test-binary args (e.g. `--nocapture`): `cargo nextest run --cargo-profile dev <filter> -- --nocapture`
- Pass `--cargo-profile dev` to raw nextest too: without it cargo uses its `test` profile, which builds every dependency once more (about 2 GB) instead of reusing the dev build (DEV-579).
- Raw nextest (no npm runner) still selects `git_required` tests; they fail closed via `require_git()` when Git is unavailable — never a silent pass.
- Fast clippy-only loop (lib+bins, no `--all-targets`/`--all-features`): `npm run lint:backend:fast`
- `npm run test:rust` and `test:rust:agent` run `npm run build:web` first; `include_dir!` (crates/lotar-web-assets) requires the generated `target/web-embed` directory. In a fresh worktree run `npm run build:web` yourself before other cargo commands (raw nextest, clippy).
- Don't narrow Rust runs with `-p lotar-lib`: its tests run the `lotar` binary of the root package, which only a workspace run rebuilds. Filter with `-E` instead.
- The tests of `crates/lotar-core` (DEV-499), `crates/lotar-tasks` and `crates/lotar-agent` (DEV-634) run no binary, so `-p` works for them: `cargo nextest run --cargo-profile dev -p lotar-agent` builds that crate and the ones below it. Agent and automation tests that need only the services go in `crates/lotar-agent` (unit tests, or `tests/` modules listed in `tests/integration.rs`, or in `tests/windows_regressions.rs` when the Windows CI job must run them); those that run the `lotar` binary or REST stay in `crates/lotar-lib/tests`.
- Sandbox denial is not a code failure: when cargo stops before compiling with `sccache` (the `rustc-wrapper` in `~/.cargo/config.toml`) and `Operation not permitted`, the agent sandbox blocked the cache server. Rerun the same command with the harness's approval for unsandboxed execution; if that is refused, rerun it with `RUSTC_WRAPPER=` (empty disables the wrapper; a slower, uncached build). Report which retry ran.

## UI unit tests (vitest)

- Full: `npm run test:ui`
- By name: `npm run test:ui -- -t "<substring>"`
- Single file: `npm run test:ui -- view/<path>/<file>.test.ts`
- Typecheck frontend and smoke: `npm run lint:frontend`; oxlint + ESLint (Vue templates): `npm run lint:js`; Prettier check: `npm run fmt:js:check`

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
