# Platform test verification

How platform-sensitive test coverage is verified when the development machine
cannot run every platform, and what the Windows CI lane in
`.github/workflows/ci.yml` (job `windows_checks`) actually proves.

## Scope of the Windows lane

`windows_checks` runs on `windows-latest` and keeps its existing coverage
unchanged (the `test(storage::safety::) | test(utils::scan::)` nextest module
selection plus the manifest-vs-JUnit coverage gate), and adds a browser-free
portable agent smoke selection:

```
npm run test:smoke:quick -- \
  smoke/tests/fixtures.agent.smoke.spec.ts \
  smoke/tests/api.agent-jobs.smoke.spec.ts \
  smoke/tests/cli.automation.smoke.spec.ts \
  smoke/tests/api.automation.smoke.spec.ts \
  --reporter=default \
  --reporter=junit \
  --outputFile.junit=target/smoke-report/portable-agent-smoke.junit.xml
```

(On the runner the JUnit path is absolute under `$GITHUB_WORKSPACE`; see the
workflow step for the exact pwsh form.)

These four specs are chosen because they are browser-free (no Playwright
install, no Chromium) and git-independent: fixture launchers, the agent job
REST lifecycle, CLI automation, and REST automation endpoints exercise the
real `lotar.exe` through the smoke helpers only.

The nextest module selection and coverage gate in this lane follow the shared
naming, runner, and coverage-enforcement policy documented in
[tests/README.md](../../tests/README.md).

Environment contract on the runner:

- `LOTAR_BINARY_PATH=<workspace>/target/ci/lotar.exe` — the supported variable
  consumed by `smoke/helpers/binary.ts` (`LOTAR_BIN` is the legacy alias).
  When it is unset and no `target/smoke`/`target/release` binary exists, the
  helper fails loudly; it never builds.
- `LOTAR_SMOKE_REQUIRE_GIT=1` — designated-runner contract from
  `smoke/helpers/git.ts`: the real `git init` probe is fatal instead of
  producing silently skipped git-dependent specs.
- Node comes from `engines.node` in `package.json` via the same
  determine-then-setup steps as the other jobs, and `npm ci` requires the
  lockfile. `npm run build:web` must precede the ci-profile cargo build
  because `include_dir!("target/web-embed")` (src/web_server.rs) requires the
  generated directory at compile time; the API-only smoke specs never serve
  UI assets, but the binary must still compile on a cold runner.

## Fail-closed semantics

- No `continue-on-error` anywhere in the lane.
- vitest exits non-zero on any test failure and when a filter selects no test
  files, so an empty or failing selection is red by construction. There is no
  pass-count floor: counts may grow with the specs; the selection is the
  contract.
- The JUnit report is uploaded with `if: always()` and
  `if-no-files-found: error`: a red run still leaves its real results as an
  artifact, and a run that crashed before the reporter wrote anything fails
  the upload instead of uploading silence.

## What can and cannot be verified locally

What a development host can prove depends on the host, not on this
repository:

- A local mirror run proves the selection, the helper contracts, and the
  code paths the host can execute (on a POSIX host: Unix shebang launchers
  and Unix spawn shapes).
- Windows-specific branches — the `.cmd` launcher invoked through `cmd /c`,
  and the Rust side's `cmd` spawn quoting for paths with spaces (BatBadBut
  escaping) — require an actual Windows environment: a user-provided Windows
  machine or a Windows CI runner. If no Windows environment is available,
  a local mirror cannot prove them, and Windows verification stays unclaimed
  — never mark it verified based on local mirrors alone.
- Run the checks available to you locally first (mirror runs, YAML and
  command-shape validation). Git writes and triggering remote CI require
  explicit authorization (see AGENTS.md). Genuine Windows evidence is the
  result of a real Windows run — the `windows-portable-smoke-report`
  artifact on CI, or equivalent logs from a user-provided Windows host.

## Reproducing the lane

Windows (PowerShell, from the repository root). Node must satisfy
`engines.node` in `package.json`. Prepare in this order — `npm run build:web`
is required before the Rust build because `include_dir!("target/web-embed")`
needs the generated directory at compile time:

```powershell
npm ci
npm run build:web
# CI-equivalent strict Git handling (optional; set before the build, as the
# runner does): LOTAR_REQUIRE_GIT is the build-time designated-runner flag,
# LOTAR_SMOKE_REQUIRE_GIT makes the smoke Git probe fatal instead of skipping.
$env:LOTAR_REQUIRE_GIT = '1'
$env:LOTAR_SMOKE_REQUIRE_GIT = '1'
cargo build --profile ci --all-features

$env:LOTAR_BINARY_PATH = "$PWD\target\ci\lotar.exe"
New-Item -ItemType Directory -Force -Path target/smoke-report | Out-Null
npm run test:smoke:quick -- `
  smoke/tests/fixtures.agent.smoke.spec.ts `
  smoke/tests/api.agent-jobs.smoke.spec.ts `
  smoke/tests/cli.automation.smoke.spec.ts `
  smoke/tests/api.automation.smoke.spec.ts `
  --reporter=default `
  --reporter=junit `
  "--outputFile.junit=$PWD/target/smoke-report/portable-agent-smoke.junit.xml"
```

POSIX mirror (macOS/Linux; proves selection, env contract, and Unix paths
only — not the Windows branches). Build a fresh binary first (`npm run
build:smoke`) whenever `src/` changed after the last build:

```sh
export LOTAR_BINARY_PATH="$PWD/target/smoke/lotar"
export LOTAR_SMOKE_REQUIRE_GIT=1   # only on a runner where real git works
mkdir -p target/smoke-report
npm run test:smoke:quick -- \
  smoke/tests/fixtures.agent.smoke.spec.ts \
  smoke/tests/api.agent-jobs.smoke.spec.ts \
  smoke/tests/cli.automation.smoke.spec.ts \
  smoke/tests/api.automation.smoke.spec.ts \
  --reporter=default \
  --reporter=junit \
  --outputFile.junit="$PWD/target/smoke-report/portable-agent-smoke.junit.xml"
```

`smoke/tests/fixtures.agent.smoke.spec.ts` is self-contained (Node fixtures
only) and runs without any lotar binary; the other three need the binary.

## Expected results and exit codes

- Exit 0: every selected test passed. Expect skipped=0 for this selection;
  a skip here means a platform guard fired unexpectedly and should be
  investigated, not ignored.
- Non-zero: at least one failure, or the filters matched no files. Inspect
  the JUnit artifact for the real per-test outcomes.
- The storage/scan nextest steps keep their own manifest-vs-JUnit gate and
  are unaffected by the smoke selection.

## Cleanup and safety scope

- All servers and child processes are owned by the smoke helpers
  (`startLotarServer` binds `--port 0` and always reaps; gate fixtures
  drain their children). Never start manual, unmanaged `lotar serve`
  instances for verification, and never terminate processes or listeners you
  do not own — a foreign listener that happens to sit on the default serve
  port (8080) is an example, not an exception.
- Workspaces are isolated temp directories; the helpers set `LOTAR_HOME` and
  `LOTAR_TASKS_DIR` per workspace, so no host configuration is touched.
- Verification logs and artifacts live under `target/test-fix-handoff/`
  (untracked) — never under the task YAML, and never containing secrets.

## Integration ordering

The `Run rust-test-runner unit tests` steps (in `ui_tests` and
`windows_checks`) run `node --test scripts/tests/rust-test-runner.test.mjs`.
Those steps require that file to exist: a tree that carries the workflow
steps without the runner's test file fails them by design. Workflow changes
touching those steps and the runner's tests must therefore be integrated
together, with the runner's test file present no later than the steps.
