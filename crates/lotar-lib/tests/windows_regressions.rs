//! Tests the Windows CI job runs natively, in their own small binary (DEV-495).
//!
//! The Windows job links only this binary and `lotar-agent-wrapper`'s tests:
//! building the library's unit tests or the integration binary costs minutes
//! there. Linux runs these with every other test target. Add a file here only
//! when its tests must run on Windows; each needs just the public library API.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] fns")]

mod automation_async_run_test;
mod storage_safety_test;
mod utils_scan_test;

#[path = "common/env_mutex.rs"]
mod env_mutex;
