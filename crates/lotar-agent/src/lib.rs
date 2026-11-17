// Library-only lint (shared lint levels live in Cargo.toml [lints]), as in
// crates/lotar-lib/src/lib.rs.
#![warn(clippy::needless_pass_by_value)]

//! Agent jobs, their queue and runner, and automation (DEV-634), on the task
//! services of crates/lotar-tasks. [`install_hooks`] connects the task
//! services back to automation and agent jobs.

// Re-exported so `crate::config`, `crate::services::task_service` and the
// like resolve here as in lotar-lib.
pub use lotar_tasks::{LoTaRError, LoTaRResult, Storage, Task, TaskFilter, TaskStatus, TasksDirectoryResolver};
pub use lotar_tasks::{api_events, api_types, config, errors, hooks, output, project, scanner, storage, types, utils, workspace};

pub mod automation;
pub mod services;
mod task_hooks;

pub use task_hooks::install_hooks;

#[cfg(test)]
#[path = "../../lotar-core/tests/common/env_mutex.rs"]
pub(crate) mod test_env;

#[cfg(test)]
#[path = "../../lotar-core/tests/common/unit_test_environment.rs"]
mod test_environment;

// The unit tests save tasks through the task services, which reach
// automation only once the hooks are installed.
#[cfg(test)]
#[ctor::ctor(unsafe)]
fn install_hooks_for_tests() {
    install_hooks();
}
