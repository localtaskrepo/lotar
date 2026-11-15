// Library-only lint (shared lint levels live in Cargo.toml [lints]), as in
// crates/lotar-lib/src/lib.rs.
#![warn(clippy::needless_pass_by_value)]

//! The core of the lotar library (DEV-499): config, storage, tasks, output
//! and the REST types. It never depends on the layers above it
//! (crates/lotar-lib), which re-exports these modules.

pub mod api_events;
pub mod api_types;
pub mod config;
pub mod errors;
pub mod output;
pub mod project;
pub mod scanner;
pub mod storage;
pub mod types;
pub mod utils;
pub mod workspace;

#[cfg(test)]
#[path = "../tests/common/env_mutex.rs"]
pub(crate) mod test_env;

// Shared with lotar-lib's unit tests; never compiled into non-test builds.
#[cfg(test)]
#[path = "../tests/common/unit_test_environment.rs"]
mod test_environment;
