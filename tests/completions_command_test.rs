use predicates::prelude::*;
use std::fs;

mod common;

#[test]
fn completions_generate_can_write_file_and_print() {
    let fixtures = common::TestFixtures::new();
    let target_path = fixtures
        .temp_dir
        .path()
        .join("generated-completions")
        .join("lotar.bash");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args([
        "completions",
        "generate",
        "--shell",
        "bash",
        "--output",
        target_path.to_str().expect("valid utf-8 path"),
        "--print",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("✅").and(predicate::str::contains("complete -F")));

    let generated = fs::read_to_string(&target_path).expect("completion file created");
    assert!(
        generated.contains("complete -F"),
        "generated completion should contain bash function registration"
    );
}

#[test]
fn completions_install_honors_xdg_paths() {
    let fixtures = common::TestFixtures::new();
    let home = fixtures.temp_dir.path().join("home");
    let xdg_home = home.join("xdg");
    fs::create_dir_all(&xdg_home).expect("create xdg directory");

    let expected_path = xdg_home.join("bash-completion/completions/lotar");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_home)
        .args(["completions", "install", "--shell", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed bash completion"));

    let contents = fs::read_to_string(&expected_path)
        .unwrap_or_else(|_| panic!("expected completion at {}", expected_path.display()));
    assert!(
        contents.contains("complete -F"),
        "installed completion should contain bash function registration"
    );
}

#[test]
fn completions_without_subcommand_lists_actions() {
    let fixtures = common::TestFixtures::new();

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["completions"]) // no subcommand
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Available completions subcommands")
                .and(predicate::str::contains("generate"))
                .and(predicate::str::contains("install")),
        )
        .stderr(predicate::str::is_empty());
}
