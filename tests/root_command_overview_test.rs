use predicates::prelude::*;

mod common;

#[test]
fn task_command_without_subcommand_lists_options() {
    let fixtures = common::TestFixtures::new();

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["task"]) // intentionally omit a subcommand
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Available task subcommands")
                .and(predicate::str::contains("add"))
                .and(predicate::str::contains("list")),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn config_command_without_subcommand_lists_options() {
    let fixtures = common::TestFixtures::new();

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["config"]) // no subcommand
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Available config subcommands")
                .and(predicate::str::contains("show"))
                .and(predicate::str::contains("set")),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn sprint_command_without_subcommand_lists_options() {
    let fixtures = common::TestFixtures::new();

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint"]) // missing subcommand
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Available sprint subcommands")
                .and(predicate::str::contains("create"))
                .and(predicate::str::contains("list")),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn git_command_without_subcommand_lists_options() {
    let fixtures = common::TestFixtures::new();

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["git"]) // no subcommand
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Available git subcommands")
                .and(predicate::str::contains("hooks")),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn git_hooks_without_subcommand_lists_options() {
    let fixtures = common::TestFixtures::new();

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["git", "hooks"]) // missing nested subcommand
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Available git hooks subcommands")
                .and(predicate::str::contains("install")),
        )
        .stderr(predicate::str::is_empty());
}
