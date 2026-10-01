# Tests: naming and structure

Naming standard (singular suffix):
- Unit tests: `*_unit_test.rs`
- Integration suites (end-to-end/domain): `*_integration_test.rs`
- Feature suites (focused behavior groups): `*_features_test.rs`
- CLI end-to-end: `cli_*_test.rs`

General guidance:
- Prefer fewer, consolidated suites over many tiny files.
- Reuse shared helpers from `tests/common`.
- For environment variables, use `EnvVarGuard::set`/`EnvVarGuard::clear` so the per-variable mutex is held and the previous value is restored on drop; do not remove or set variables directly mid-test. Fixtures (`TestFixtures::new`, `common::temp_dir`) never mutate the process environment, and child commands get test defaults child-scoped via `common::lotar_cmd`.
- For networked tests, use readiness checks and test-fast flags (e.g., `LOTAR_TEST_FAST_*`).

Runner policy:
- Use nextest by default for speed and reliability.
- Local: `cargo nextest run --cargo-profile ci --all-features --failure-output=immediate-final --retries 0 --lib --bins --tests` (the exact CI command) or `npm run test:rust` → `scripts/rust-test-runner.mjs --cargo-profile ci` (all targets, no `--all-features`): the runner probes real Git capability first and runs the ordinary default profile when it works.
	- CI uses the same flags; JUnit lands at `target/nextest/default/junit.xml` (relative junit paths in `.config/nextest.toml` resolve under the per-profile output dir).
	- Doc tests are run separately via `cargo test --doc --all-features` (not supported by nextest yet).
- The legacy `cargo test` harness is intentionally disabled and prints guidance to use nextest (enforced by `tests/nextest_guard_test.rs`; doc tests above are the exception).
- Performance budgets are an explicit opt-in via `npm run test:rust:perf`; ordinary correctness runs still execute every test in `tests/performance_test.rs` with budgets skipped (see [performance budgets](../docs/developers/performance-budgets.md)).
- Git capability is a build-time contract on CI: Rust-building jobs set `LOTAR_REQUIRE_GIT=1`, so `build.rs` panics when creating a `.git` directory is denied, and the runner aborts before compiling or selecting anything. Git-dependent tests are never compiled out: they live in source-local `git_required` modules (classify per test — pure tests in the same file stay outside the module), each still calling `require_git()` so a raw `cargo nextest` selection fails closed instead of passing silently (smoke mirrors this with `LOTAR_SMOKE_REQUIRE_GIT=1`). The workflow itself runs no git commands.
- Coverage enforcement is declarative, not name-based: after each nextest run, CI records `cargo nextest list --run-ignored all --message-format json` for the identical selection (ignored tests are intentionally included in the listing so a selected-but-ignored test is rejected rather than silently filtered out; the run command itself keeps default ignore semantics) and `scripts/check-test-coverage.py` asserts set equality against the JUnit report — every selected test executed exactly once, none failed or skipped, no stale extras, selected-ignored rejected (JUnit may omit ignored tests, so the manifest's own flag is authoritative). Renames and new tests are picked up from the manifest automatically. Pairing identity is nextest's canonical binary id — each manifest rust-suites key must equal the suite's `binary-id` (kind-qualified, e.g. `lotar::bin/lotar-agent-wrapper`), which is exactly the JUnit classname, so bin-kind and future target kinds pair without any name inventory. The helper's `--self-test` runs as a CI step, and Windows enforces the `test(storage::safety::) | test(utils::scan::)` module selection, where platform-specific cases appear via that run's own manifest. The Windows lane that runs this selection and its fail-closed verification semantics are described in [platform test verification](../docs/developers/platform-test-verification.md).
- In sandboxes where `.git` creation is denied, everything still compiles: `npm run test:rust`/`test:rust:agent` auto-select the `gitless` nextest profile whose default filter is `not test(/(^|::)git_required::/)` (inheriting every default-profile setting), report why Git is unavailable plus the dynamically computed exclusion inventory, and write JUnit to `target/nextest/gitless/junit.xml` — excluded tests are reported as skipped, never passed. `LOTAR_REQUIRE_GIT=1` forbids the gitless profile entirely, so full-suite zero-skip assertions hold on CI and normal workstations. No copied test-name inventories: exclusion lists come from `cargo nextest list -E 'test(/(^|::)git_required::/)'` at run time.

 Lint/format parity with CI:
 - Format: `cargo fmt --all --check` (pre-commit auto-fixes when possible).
 - Clippy: `cargo clippy --all-targets --all-features -- -D warnings`.
	 - CI and git hooks use these exact flags, so warnings will fail the build/push.
	- Toolchain: repository pins Rust via `rust-toolchain.toml` to ensure clippy lints are consistent across local and CI.

 Consolidation targets:
 - Scanner integration in `scanner_integration_test.rs` and features in `scanner_features_test.rs`.
 - Stats snapshot-related tests under modules in `stats_snapshot_test.rs`.
 - Output formatting sanity under a submodule of `output_format_consistency_test.rs`.

 When renaming:
 - Keep logical groupings (command/topic first, detail after): e.g., `scan_bidir_references_test.rs`.
 - Use singular `_test.rs` (not `_tests.rs`).
 - Prefer `*_integration_test.rs` for end-to-end suites like `project_integration_test.rs`, `storage_integration_test.rs`.
