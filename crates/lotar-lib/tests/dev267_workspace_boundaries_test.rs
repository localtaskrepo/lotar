//! DEV-267: read-only commands do not create workspaces, and workspace
//! discovery stays inside the git repository and below a home `.tasks`.

use crate::common;
fn lotar(dir: &std::path::Path, home: &std::path::Path, args: &[&str]) -> String {
    let output = common::lotar_cmd()
        .unwrap()
        .current_dir(dir)
        .env("HOME", home)
        .env_remove("LOTAR_TASKS_DIR")
        .env("LOTAR_IGNORE_ENV_TASKS_DIR", "1")
        .args(args)
        .output()
        .unwrap();
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn git_init(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
}

#[test]
fn read_only_commands_create_nothing() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    for args in [&["list"][..], &["show", "X-1"], &["stats", "tags"], &["--format", "json", "list"]] {
        lotar(root.path(), home.path(), args);
        assert!(!root.path().join(".tasks").exists(), "{args:?} created a workspace");
    }
}

#[test]
fn nested_repositories_and_home_get_their_own_workspace() {
    let home = tempfile::tempdir().unwrap();
    let home_path = home.path();
    std::fs::create_dir_all(home_path.join(".tasks")).unwrap();
    std::fs::write(home_path.join(".tasks/config.yml"), "default:\n  project: HOMEWS\n").unwrap();

    // A plain folder below home does not adopt ~/.tasks.
    let plain = home_path.join("notes/sub");
    std::fs::create_dir_all(&plain).unwrap();
    lotar(&plain, home_path, &["add", "plain"]);
    assert!(plain.join(".tasks").exists());
    assert!(!home_path.join(".tasks/HOMEWS").exists());

    // A nested repository does not write into the outer repository.
    let outer = home_path.join("outer");
    git_init(&outer);
    lotar(&outer, home_path, &["add", "outer"]);
    let inner = outer.join("inner");
    git_init(&inner);
    lotar(&inner, home_path, &["add", "inner"]);
    assert!(inner.join(".tasks").exists(), "inner repo gets its own workspace");

    // Inside a repository, subfolders still find the repository's workspace.
    let sub = outer.join("src/deep");
    std::fs::create_dir_all(&sub).unwrap();
    lotar(&sub, home_path, &["add", "from subfolder"]);
    assert!(!sub.join(".tasks").exists());

    // Running in the home folder itself still uses ~/.tasks.
    let out = lotar(home_path, home_path, &["add", "at home"]);
    assert!(out.contains("HOMEWS-1"), "{out}");
}
