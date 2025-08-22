# Tests: naming and structure

Naming standard (singular suffix):
- Unit tests: `*_unit_test.rs`
- Integration suites (end-to-end/domain): `*_integration_test.rs`
- Feature suites (focused behavior groups): `*_features_test.rs`
- CLI end-to-end: `cli_*_test.rs`

General guidance:
- Prefer fewer, consolidated suites over many tiny files.
- Reuse shared helpers from `tests/common`.
- For environment variables, use `EnvVarGuard::set` or a scoped reset (`lock_var` + `remove_var`). Avoid global state races.
- For networked tests, use readiness checks and test-fast flags (e.g., `LOTAR_TEST_FAST_*`).

Runner policy:
- Use nextest by default for speed and reliability.
- Local: `cargo nextest run --all-features --failure-output=immediate-final --retries 0 --lib --bins`.
	- CI uses the same flags and writes JUnit to `target/nextest/junit.xml`.
	- Doc tests are run separately via `cargo test --doc --all-features` (not supported by nextest yet).

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
 - Keep logical groupings (command/topic first, detail after): e.g., `changelog_range_feature_test.rs`.
 - Use singular `_test.rs` (not `_tests.rs`).
 - Prefer `*_integration_test.rs` for end-to-end suites like `project_integration_test.rs`, `storage_integration_test.rs`.
