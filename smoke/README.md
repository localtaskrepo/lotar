# Smoke Test Harness

This directory contains the TypeScript-based smoke-test harness for exercising the built LoTaR CLI and web UI end to end. The tests are designed to run against the compiled binary and the production web bundle, isolating each scenario inside a temporary workspace so they can execute in parallel without conflicting state.

## Layout

- `helpers/` — reusable utilities for running the CLI, managing temporary workspaces, seeding git repositories, launching the web server, and driving the browser via Playwright.
- `tests/` — top-level smoke suites. These currently contain placeholders describing the scenarios we intend to validate; you can flesh them out incrementally as we automate each flow.
- `vitest.config.ts` — Vitest configuration scoped to this harness. Tests run in worker threads and use generous timeouts to accommodate longer CLI/UI interactions.
- `tsconfig.json` — TypeScript compiler settings dedicated to the smoke harness.

## Running the suite

```bash
npm run smoke
```

The `smoke` script recompiles the frontend and CLI (`npm run build`), ensures Playwright browsers are installed, and then executes the smoke suite via Vitest. Each test case creates a dedicated temporary directory, sets the appropriate `LOTAR_*` environment variables, and, when needed, launches the `lotar serve` process on a dynamically allocated port.

## Development tips

- Use the helpers in `helpers/` to keep tests declarative. For example, `SmokeWorkspace` exposes `runLotar`, `initGit`, and filesystem helpers, while `startLotarServer` handles lifecycle management of the background web server.
- Stop every server you start (`await server.stop()` in a `finally`). A run that ends with a server still running fails, and a detached reaper (`helpers/server-guard.ts`) kills the servers of a run that fails or is killed.
- A test that times out keeps running in the background. Open browsers, servers and workspaces through `withBrowser`/`withPage`, `startLotarServer` and `SmokeWorkspace.create`: after a file with a failed test, `helpers/setup.ts` closes what they still hold (`helpers/owed-cleanup.ts`), so the run reports the failure and leaves no temp entries behind.
- One full test run per machine at a time (DEV-586, DEV-784): a full smoke run takes the machine's check lock, port 47586 (`scripts/check-lock.mjs`), and waits while another full run (smoke, UI, Rust or lotar's merge checks) holds it, because runs side by side starve each other's browsers into timeouts. A run filtered by file or test name (`-t`) doesn't wait; its own processes get `LOTAR_CHECK_LOCK_HELD=1`, as do a full run's, so nothing in a run waits for the lock again.
- Spec files that wait on wall-clock budgets are in `WALL_CLOCK_SPECS` in `vitest.config.ts` (DEV-785): they run in the `wall-clock` project after the other files, one file at a time. A spec that fails under load and passes alone goes on that list.
- Prefer `describe.concurrent` suites and keep per-test state self-contained so the harness can run in parallel without coordination overhead.
- If you add browser-based checks, `withPage` wraps Playwright in a safe lifecycle that automatically closes pages and browsers even when assertions fail.
- To debug a single test, run `npx vitest watch --config smoke/vitest.config.ts --runInBand`; `SMOKE_DEBUG=1` logs workspace and server details from the helpers.

See the placeholder comments inside `tests/` for ideas on the first smoke flows to automate.
