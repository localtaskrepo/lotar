//! lotar-agent's tests the Windows CI job runs natively, in their own small
//! binary (DEV-495), next to crates/lotar-lib's `windows_regressions`: the
//! job's `--test windows_regressions` selects both. Add a file here only when
//! its tests must run on Windows; each needs just the public library API.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] fns")]

mod automation_async_run_test;
