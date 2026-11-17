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
// Automation and agent jobs are crates/lotar-agent, the task services below
// them crates/lotar-tasks (DEV-634). The task services reach automation
// through hooks, which every entry point installs with `install_hooks()`.
pub use lotar_agent::{automation, hooks, install_hooks};

pub mod api_server;
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

// Unit tests run without main(), so they install the hooks themselves.
#[cfg(test)]
#[ctor::ctor(unsafe)]
fn install_hooks_for_tests() {
    install_hooks();
}

#[cfg(test)]
mod hooks_installed_test {
    #[test]
    fn unit_tests_run_with_the_agent_hooks_installed() {
        assert!(crate::hooks::installed(), "lotar_agent::install_hooks() must run before any task mutation");
    }
}
