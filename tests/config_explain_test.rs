use assert_cmd::Command;

mod common;
use common::TestFixtures;
use common::env_mutex::EnvVarGuard;

#[test]
fn config_show_explain_includes_sources() {
    let test_fixtures = TestFixtures::new();
    let temp_dir = test_fixtures.temp_dir.path();

    // Set an env default to observe in explain output
    let _guard = EnvVarGuard::set("LOTAR_DEFAULT_REPORTER", "env.reporter@example.com");

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let output = cmd
        .current_dir(temp_dir)
        .arg("config")
        .arg("show")
        .arg("--explain")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    assert!(
        stdout.contains("Global configuration – canonical YAML:"),
        "explain output should include global heading\n{stdout}"
    );
    assert!(
        stdout.contains("default:"),
        "canonical YAML should include default section when explaining overrides\n{stdout}"
    );
    assert!(
        stdout.contains("reporter: env.reporter@example.com # (env)"),
        "explain output should annotate env reporter\n{stdout}"
    );

    // restored by guard on drop
}
