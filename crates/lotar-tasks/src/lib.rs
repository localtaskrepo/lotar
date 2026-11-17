// Library-only lint (shared lint levels live in Cargo.toml [lints]), as in
// crates/lotar-lib/src/lib.rs.
#![warn(clippy::needless_pass_by_value)]

//! The task services of the lotar library (DEV-634): tasks, lists and
//! queries, relationships, sprints, releases, references and attachments, on
//! lotar-core. Automation and agent jobs (crates/lotar-agent) sit above it;
//! the task services reach them only through [`hooks`].

// Re-exported so `crate::config` and the like resolve here as in lotar-lib.
pub use lotar_core::{api_events, api_types, config, errors, output, project, scanner, storage, types, utils, workspace};

pub use errors::{LoTaRError, LoTaRResult};
pub use storage::{TaskFilter, manager::Storage, task::Task};
pub use types::TaskStatus;
pub use workspace::TasksDirectoryResolver;

pub mod hooks;
pub mod services;

#[cfg(test)]
#[path = "../../lotar-core/tests/common/env_mutex.rs"]
pub(crate) mod test_env;

#[cfg(test)]
#[path = "../../lotar-core/tests/common/unit_test_environment.rs"]
mod test_environment;
