---
name: smoke-suite-debugging
description: Use this when smoke tests fail (binary/web assets, Playwright setup, server lifecycle, ports, env vars).
---

## Quick checklist

1) Ensure Playwright browsers exist
- `npm run playwright:install`

2) Ensure build artifacts are fresh
- Full build + smoke: `npm run smoke`
- Quick smoke (assumes you already built): `npm run test:smoke:quick`

3) Run one smoke test first
- By name:
  - `npm run test:smoke:quick -- -t "<substring>"`
- By file:
  - `npm run test:smoke:quick -- smoke/tests/<suite>.smoke.spec.ts`

## Common failure modes

- “Binary not found”
  - Smoke prefers `target/smoke/lotar`, falling back to `target/release/lotar`.
  - Fix: run `npm run build:smoke`, or set `LOTAR_BINARY_PATH` (or `LOTAR_BIN`) to a freshly built custom binary.

- “Port already in use”
  - By default the harness passes `--port 0`: the OS assigns the port at bind time (no probe-then-bind race) and the actual port is read back from the post-bind readiness banner, so default runs do not collide.
  - A pinned explicit port that is busy fails strictly by design. That may point to a server-lifecycle issue, but never kill or reconfigure a server this run did not spawn — ownership of a foreign listener cannot be inferred from the port alone.
  - To isolate lifecycle issues, re-run with `--maxWorkers=1 --maxConcurrency=1 --no-file-parallelism`. Vitest does not support `--runInBand`.

- “SSE readiness / flaky waits”
  - Smoke uses `LOTAR_SSE_READY` hooks and server heartbeats; see `docs/help/serve.md` for the testing aids.

## Restricted / sandboxed agent environments

Some agent harnesses run the shell in a sandbox that blocks certain OS operations. LoTaR's tests are already hardened for this, so you usually don't need to act — just recognize the signatures:

- **`.git: Operation not permitted`** — the sandbox forbids creating anything named `.git`, so tests that run a real `git init` cannot use Git. Whether Git works is decided per invocation by an isolated probe, not per host: the Rust runner's probe can pass while the Node smoke probe fails in the same environment (or vice versa), so never generalize one probe's result. Nothing is compiled out:
  - `npm run test:rust`/`test:rust:agent` probe at run time and, when Git is unavailable, select the `gitless` nextest profile whose default filter excludes exactly the source-local `git_required` modules — all code still compiles, and exclusions are reported as skipped, never as passes.
  - Raw `cargo nextest` (no npm runner) still selects `git_required` tests; they fail closed via `require_git()` instead of passing silently.
  - Smoke git tests skip honestly via `describe.concurrent.skipIf(!gitAvailable())`.
  - Designated full-coverage runners set `LOTAR_REQUIRE_GIT=1` (build refuses) and `LOTAR_SMOKE_REQUIRE_GIT=1` (smoke probe fatal): missing Git must FAIL — no gitless fallback, compile-outs, or early-return passes. Do not add new gates or compile-out cfgs; see the [tests README](../../../tests/README.md), [testing-strategy](../testing-strategy/SKILL.md), and [platform guide](../../../docs/developers/platform-test-verification.md).
- **Chromium won't launch (`MachPortRendezvous … Permission denied`, then `Target page … closed`)** — the sandbox denies the browser's multi-process bootstrap. Set `LOTAR_SMOKE_CHROMIUM_ARGS="--no-sandbox,--no-zygote,--single-process"` (the smoke `withBrowser` helper reads this; empty by default so CI is unaffected).
- **`mcp.protocol` framed-transport test waits for a `tools/listChanged` notification** after a config write — this depends on file-change detection. The MCP config watcher now polls as a fallback (alongside the kernel watcher), so it fires reliably even when kernel file-watching is blocked; the binary itself answers framed MCP fine.

## Useful env vars

- `LOTAR_BINARY_PATH` / `LOTAR_BIN`: override the binary used by smoke.
- `LOTAR_TASKS_DIR`, `LOTAR_HOME`: smoke sets these per-test (see `smoke/helpers/workspace.ts`).
- `LOTAR_SMOKE_CHROMIUM_ARGS`: comma-separated Chromium launch flags for restricted sandboxes (e.g. `--no-sandbox,--no-zygote,--single-process`); empty by default.
- `RUST_LOG=debug` or `LOTAR_DEBUG=1`: can help when diagnosing server/CLI behavior (keep logs free of secrets/PII).

## Debugging approach

- Prefer `npx vitest watch --config smoke/vitest.config.ts --maxWorkers=1 --maxConcurrency=1 --no-file-parallelism` for a serialized watch loop.
- If needed, temporarily enable inherited stdio in the smoke helpers while debugging (but keep changes scoped and revert before finalizing).
