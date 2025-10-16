# Smoke Test Harness

This directory contains the TypeScript-based smoke-test harness for exercising the built LoTaR CLI and web UI end to end. The tests are designed to run against the compiled binary and the production web bundle, isolating each scenario inside a temporary workspace so they can execute in parallel without conflicting state.

## Layout

- `helpers/` — reusable utilities for running the CLI, managing temporary workspaces, seeding git repositories, launching the web server, and driving the browser via Playwright.
- `tests/` — top-level smoke suites. These currently contain placeholders describing the scenarios we intend to validate; you can flesh them out incrementally as we automate each flow.
- `vitest.config.ts` — Vitest configuration scoped to this harness. Tests run in forked worker processes and use generous timeouts to accommodate longer CLI/UI interactions.
- `tsconfig.json` — TypeScript compiler settings dedicated to the smoke harness.

## Running the suite

```bash
npm run smoke
```

The `smoke` script recompiles the frontend and CLI (`npm run build`), ensures Playwright browsers are installed, and then executes the smoke suite via Vitest. Each test case creates a dedicated temporary directory, sets the appropriate `LOTAR_*` environment variables, and, when needed, launches the `lotar serve` process on a dynamically allocated port.

## Development tips

- Use the helpers in `helpers/` to keep tests declarative. For example, `SmokeWorkspace` exposes `runLotar`, `initGit`, and filesystem helpers, while `startLotarServer` handles lifecycle management of the background web server.
- Prefer `describe.concurrent` suites and keep per-test state self-contained so the harness can run in parallel without coordination overhead.
- If you add browser-based checks, `withPage` wraps Playwright in a safe lifecycle that automatically closes pages and browsers even when assertions fail.
- To debug a single test, run `npx vitest watch --config smoke/vitest.config.ts --runInBand` and enable logging or `stdio: 'inherit'` on the CLI helpers as needed.

See the placeholder comments inside `tests/` for ideas on the first smoke flows to automate.
