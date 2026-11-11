//! DEV-287: identity comes from the [user] section only, pulls and scans
//! don't take metadata from the current branch, team owners aren't
//! assignees, and hooks install doesn't silence existing hooks.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;

fn git(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git").current_dir(root).env_remove("GIT_DIR").args(args).output().unwrap()
}

fn lotar(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    common::lotar_cmd().unwrap().current_dir(root).env_remove("LOTAR_DEFAULT_REPORTER").args(args).output().unwrap()
}

#[test]
fn reporter_comes_from_the_user_section() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    git(root, &["init", "-q"]);
    git(root, &["config", "merge.npm-merge-driver.name", "automatically merge npm lockfiles"]);
    git(root, &["config", "user.name", "Real Person"]);
    let out = lotar(root, &["whoami"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Real Person"), "{text}");
    assert!(!text.contains("lockfiles"), "{text}");
}

#[test]
fn team_owners_are_not_auto_assigned() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.name", "Dev One"]);
    std::fs::create_dir_all(root.join(".github")).unwrap();
    std::fs::write(root.join(".github/CODEOWNERS"), "* @acme/platform-team @bob\n").unwrap();
    assert!(lotar(root, &["add", "work", "-p", "CO"]).status.success());
    assert!(lotar(root, &["status", "CO-1", "InProgress"]).status.success());
    let yaml = std::fs::read_to_string(fixtures.tasks_root.join("CO/1.yml")).unwrap();
    assert!(yaml.contains("assignee: bob"), "{yaml}");
    assert!(!yaml.contains("platform-team"), "{yaml}");
    assert_eq!(yaml.matches("field: assignee").count(), 1, "one assignment: {yaml}");
}

#[test]
fn hooks_install_keeps_existing_hooks_running() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    git(root, &["init", "-q"]);
    std::fs::create_dir_all(root.join(".githooks")).unwrap();
    std::fs::write(root.join(".githooks/pre-commit"), "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::write(root.join(".git/hooks/pre-push"), "#!/bin/sh\nexit 0\n").unwrap();
    let out = lotar(root, &["git", "hooks", "install"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("pre-push"));
    assert!(lotar(root, &["git", "hooks", "install", "--force"]).status.success());
}

#[test]
fn scans_do_not_take_type_from_the_branch() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    git(root, &["init", "-q", "-b", "hotfix/urgent-login"]);
    std::fs::write(fixtures.tasks_root.join("config.yml"), "default:\n  project: SC\nbranch:\n  type_aliases:\n    hotfix: Bug\n").unwrap();
    std::fs::write(root.join("a.rs"), "// TODO: from a comment\n").unwrap();
    assert!(lotar(root, &["scan"]).status.success());
    let yaml = std::fs::read_to_string(fixtures.tasks_root.join("SC/1.yml")).unwrap();
    assert!(!yaml.contains("type: Bug"), "{yaml}");
    // Interactive add still infers from the branch.
    assert!(lotar(root, &["add", "typed"]).status.success());
    let yaml = std::fs::read_to_string(fixtures.tasks_root.join("SC/2.yml")).unwrap();
    assert!(yaml.contains("type: Bug"), "{yaml}");
}
