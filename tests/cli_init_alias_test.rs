use std::path::Path;

use tempfile::TempDir;

mod common;

#[test]
fn lotar_init_alias_matches_config_init() {
    common::reset_lotar_test_environment();
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    let shared_args = ["--project=alias-init", "--template=agile", "--dry-run"];
    let mut config_args = vec!["config", "init"];
    config_args.extend(shared_args);

    let mut init_args = vec!["init"];
    init_args.extend(shared_args);

    let (config_stdout, config_stderr) = run_lotar(&config_args, temp_dir.path());
    let (init_stdout, init_stderr) = run_lotar(&init_args, temp_dir.path());

    assert_eq!(
        init_stdout, config_stdout,
        "stdout mismatch between alias and canonical command"
    );
    assert_eq!(
        init_stderr, config_stderr,
        "stderr mismatch between alias and canonical command"
    );
}

fn run_lotar(args: &[&str], dir: &Path) -> (String, String) {
    let mut cmd = common::lotar_cmd().expect("Failed to build lotar command");
    let output = cmd
        .current_dir(dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(args)
        .output()
        .expect("Failed to run lotar command");

    assert!(
        output.status.success(),
        "lotar {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}
