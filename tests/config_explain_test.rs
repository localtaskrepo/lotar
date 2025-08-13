use assert_cmd::Command;
use predicates::prelude::*;

mod common;
use common::TestFixtures;
use common::env_mutex::lock_var;

#[test]
fn config_show_explain_includes_sources() {
    let test_fixtures = TestFixtures::new();
    let temp_dir = test_fixtures.temp_dir.path();

    // Set an env default to observe in explain output (guarded and unsafe as in other tests)
    let _guard = lock_var("LOTAR_DEFAULT_REPORTER");
    unsafe {
        std::env::set_var("LOTAR_DEFAULT_REPORTER", "env.reporter@example.com");
    }

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("show")
        .arg("--explain")
        .assert()
        .success()
        .stdout(predicate::str::contains("Configuration for project:"))
        .stdout(predicate::str::contains("Value sources:"))
        .stdout(predicate::str::contains("server_port:"))
        .stdout(predicate::str::contains("default_reporter:"));

    // Clean up env var side-effect for safety
    unsafe {
        std::env::remove_var("LOTAR_DEFAULT_REPORTER");
    }
}
