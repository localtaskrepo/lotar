// Library-only lint (shared lint levels live in Cargo.toml [lints]): test
// helpers routinely take owned values for convenience, so this stays off for
// integration tests.
#![warn(clippy::needless_pass_by_value)]

// A designated runner (LOTAR_REQUIRE_GIT=1, set by CI) must never end up with
// git-dependent tests compiled out: build.rs panics under that env when the
// git capability probe fails. DEV-79 no longer emits `no_git_tests`, so
// nothing selects tests with that cfg anymore; this guard stays as insurance
// against an externally forced `no_git_tests` RUSTFLAGS override.
#[cfg(all(lotar_require_git, no_git_tests))]
compile_error!(
    "LOTAR_REQUIRE_GIT=1 and no_git_tests are both active: git-dependent tests would silently compile out on a runner that requires real git coverage"
);

// The core modules are a crate of their own (DEV-499); re-exported so
// `crate::config` here and `lotar::config` in tests and binaries keep working.
pub use lotar_core::{api_events, api_types, config, errors, output, project, scanner, storage, types, utils, workspace};

pub mod api_server;
pub mod automation;
pub mod cli;
pub mod help;
pub mod mcp;
pub mod routes;
pub mod services;
pub mod web_server;

pub use errors::{LoTaRError, LoTaRResult};
pub use storage::{TaskFilter, manager::Storage, task::Task};
pub use types::TaskStatus;
pub use workspace::TasksDirectoryResolver;

// Test helpers shared with the integration tests and lotar-core's unit
// tests (DEV-90); never compiled into non-test builds.
#[cfg(test)]
#[path = "../../lotar-core/tests/common/env_mutex.rs"]
pub(crate) mod test_env;

#[cfg(test)]
#[path = "../../lotar-core/tests/common/unit_test_environment.rs"]
mod test_environment;
