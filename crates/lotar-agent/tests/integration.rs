//! The automation tests that need only the services (DEV-634), in one binary
//! as in crates/lotar-lib (DEV-302). Tests that run the `lotar` binary or the
//! REST server stay in crates/lotar-lib/tests.
//! New test files must be added below (Cargo.toml sets `autotests = false`).

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] fns")]

// For core's test environment, which names `crate::config` and `crate::utils`.
use lotar_agent::{config, utils};

#[path = "../../lotar-core/tests/common/unit_test_environment.rs"]
mod test_environment;

mod common;

mod dev118_automation_set_plan_test;
mod dev298_automation_current_conditions_test;
mod dev365_automation_alias_conditions_test;
mod dev373_automation_version_conditions_test;
mod dev383_automation_comment_author_test;
mod dev566_moved_during_job_test;
mod dev59_automation_composition_test;

#[test]
fn integration_tests_run_with_the_agent_hooks_installed() {
    assert!(lotar_agent::hooks::installed(), "common's ctor must call lotar_agent::install_hooks()");
}
