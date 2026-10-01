# Performance budgets

Wall-clock budgets for the CLI live in `tests/performance_test.rs` and are
enforced **only in the dedicated perf mode**. Ordinary correctness runs never
fail due solely to host load, while performance regressions stay measurable.

## Commands and modes

| Mode | Command | Budgets | Execution |
| --- | --- | --- | --- |
| Correctness (default) | `npm run test:rust` | Not evaluated; each budget logs `[perf-budget] SKIP ... reason=flag-unset` | `profile.default` (`num-cpus`, configured retries) |
| Perf (opt-in) | `npm run test:rust:perf` | Enforced; failure fails the run | `profile.perf`: serial (`test-threads = 1`), `--retries 0`, no fail-fast |

Both modes run **all** tests in `tests/performance_test.rs`, including every
functional success/data assertion. There are no ignored or cfg-compiled-out
performance tests; the only difference is whether absolute wall-clock
assertions fire.

## Mode rationale

- The opt-in is explicit and one-directional: `npm run test:rust:perf`
  (`node scripts/perf-runner.mjs`) enables `LOTAR_TEST_PERF_BUDGETS=1` for
  its child and rejects an inherited non-opt-in value instead of overriding
  or silently dropping enforcement. `--dry-run` prints the resolved child
  environment and command.
- Controlled execution: the serial `perf` profile in `.config/nextest.toml`
  removes parallel-load noise; `--retries 0` means a blown budget cannot be
  retried away; fail-fast stays off so every budget is measured and
  summarized per label even after one exceeds.
- No silent fallback: the runner fails if cargo-nextest is missing, or if
  the run does not report every expected budget exactly once with a
  matching threshold.

## Baselines and thresholds

The budget values are the pre-existing thresholds of the suite, carried over
unchanged as the baseline (1–30s per label; 13 budgets across 9 tests, see
`EXPECTED_BUDGETS` in `scripts/perf-runner.mjs`). Treat any measurement —
local or CI — as a baseline comparison, not a strict SLA: local machines are
uncontrolled runners. Do **not** raise a threshold to make a run pass;
justify any change with measurement evidence first.

## CI measurement

`.github/workflows/performance.yml` is an optional, manually dispatched
(`workflow_dispatch`) job that builds the frontend and Rust `ci` profile,
runs `npm run test:rust:perf` with budgets explicitly enabled, and uploads
`target/nextest/perf/perf-junit.xml` plus `target/nextest/perf-run.log` as
baseline artifacts. It is intentionally separate from the regular CI lane;
`ci.yml` is unaffected.

Related docs: [Testing & verification](./testing.md) for the required
checklist and [Environment variables](./environment.md) for other toggles.
