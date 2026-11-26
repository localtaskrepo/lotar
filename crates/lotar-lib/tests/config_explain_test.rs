use crate::common;
use common::TestFixtures;
use common::env_mutex::EnvVarGuard;

#[test]
fn config_show_explain_includes_sources() {
    let test_fixtures = TestFixtures::new();
    let temp_dir = test_fixtures.temp_dir.path();

    // Set an env default to observe in explain output
    let _guard = EnvVarGuard::set("LOTAR_DEFAULT_REPORTER", "env.reporter@example.com");

    let mut cmd = crate::common::lotar_cmd().unwrap();
    let output = cmd.current_dir(temp_dir).arg("config").arg("show").arg("--explain").assert().success().get_output().stdout.clone();

    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains("Global configuration – canonical YAML:"), "explain output should include global heading\n{stdout}");
    assert!(stdout.contains("default:"), "canonical YAML should include default section when explaining overrides\n{stdout}");
    assert!(stdout.contains("reporter: env.reporter@example.com # (env)"), "explain output should annotate env reporter\n{stdout}");

    // restored by guard on drop
}

#[test]
fn config_show_project_explain_labels_env_over_project() {
    let test_fixtures = TestFixtures::new();
    let temp_dir = test_fixtures.temp_dir.path();
    let tasks = temp_dir.join(".tasks");
    std::fs::create_dir_all(tasks.join("ENV")).unwrap();
    std::fs::write(tasks.join("ENV/config.yml"), "default:\n  reporter: owner1\n").unwrap();
    let _guard = EnvVarGuard::set("LOTAR_DEFAULT_REPORTER", "env.reporter@example.com");

    let mut cmd = crate::common::lotar_cmd().unwrap();
    let output = cmd.current_dir(temp_dir).args(["config", "show", "--project", "ENV", "--explain", "--full"]).assert().success().get_output().stdout.clone();

    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains("reporter: env.reporter@example.com # (env)"), "env outranks project config (DEV-404)\n{stdout}");
}

/// DEV-907: the YAML shows agent.worktree, and a key the home config
/// doesn't set isn't credited to it.
#[test]
fn config_show_full_with_home_config_shows_worktree_and_labels_unset_keys_default() {
    let fixtures = TestFixtures::new();
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(".lotar"), "default:\n  priority: High\nagent:\n  worktree:\n    max_parallel_jobs: 3\n").unwrap();

    let output = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(fixtures.get_temp_path())
        .env("HOME", home.path())
        .env("LOTAR_IGNORE_HOME_CONFIG", "0")
        .args(["config", "show", "--explain", "--full"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains("priority: High # (home)"), "a key the home config sets is credited to it\n{stdout}");
    assert!(stdout.contains("strict-members: false # (default)"), "a key the home config doesn't set is not (home)\n{stdout}");
    assert!(stdout.contains("\n  worktree:\n    enabled: "), "agent.worktree is in the YAML\n{stdout}");
    assert!(stdout.contains("\n    max_parallel_jobs: 3"), "{stdout}");
}
