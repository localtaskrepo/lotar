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
  - Smoke resolves the binary from `target/release/lotar` by default.
  - Fix: run `npm run build`, or set `LOTAR_BINARY_PATH` (or `LOTAR_BIN`) to a custom path.

- “Port already in use”
  - The harness normally auto-picks a free port; failures may indicate a stuck server.
  - Re-run with a single test in-band (`--runInBand`) so it’s easier to spot lifecycle issues.

- “SSE readiness / flaky waits”
  - Smoke uses `LOTAR_SSE_READY` hooks and server heartbeats; see `docs/help/serve.md` for the testing aids.

## Restricted / sandboxed agent environments

Some agent harnesses run the shell in a sandbox that blocks certain OS operations. LoTaR's tests are already hardened for this, so you usually don't need to act — just recognize the signatures:

- **`.git: Operation not permitted`** — the sandbox forbids creating anything named `.git`, so every test that runs `git init` would fail. This is auto-handled: `build.rs` probes for it and sets the `no_git_tests` cfg, which compiles those Rust tests out; smoke git tests skip via `gitAvailable()` (`describe.concurrent.skipIf`). If you see git tests failing, the auto-detection may have missed one — add the same gate. In normal CI these all run.
- **Chromium won't launch (`MachPortRendezvous … Permission denied`, then `Target page … closed`)** — the sandbox denies the browser's multi-process bootstrap. Set `LOTAR_SMOKE_CHROMIUM_ARGS="--no-sandbox,--no-zygote,--single-process"` (the smoke `withBrowser` helper reads this; empty by default so CI is unaffected).
- **`mcp.protocol` framed-transport test waits for a `tools/listChanged` notification** after a config write — this depends on file-change detection. The MCP config watcher now polls as a fallback (alongside the kernel watcher), so it fires reliably even when kernel file-watching is blocked; the binary itself answers framed MCP fine.

## Useful env vars

- `LOTAR_BINARY_PATH` / `LOTAR_BIN`: override the binary used by smoke.
- `LOTAR_TASKS_DIR`, `LOTAR_HOME`: smoke sets these per-test (see `smoke/helpers/workspace.ts`).
- `LOTAR_SMOKE_CHROMIUM_ARGS`: comma-separated Chromium launch flags for restricted sandboxes (e.g. `--no-sandbox,--no-zygote,--single-process`); empty by default.
- `RUST_LOG=debug` or `LOTAR_DEBUG=1`: can help when diagnosing server/CLI behavior (keep logs free of secrets/PII).

## Debugging approach

- Prefer `npx vitest watch --config smoke/vitest.config.ts --runInBand` for a tight loop.
- If needed, temporarily enable inherited stdio in the smoke helpers while debugging (but keep changes scoped and revert before finalizing).
