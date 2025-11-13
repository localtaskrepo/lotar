mod common;

use tempfile::TempDir;

fn write_global_with_aliases(tasks_dir: &std::path::Path, body: &str) {
    std::fs::write(tasks_dir.join("config.yml"), body).unwrap();
}

fn init_fake_git(repo_root: &std::path::Path, branch: &str) {
    let git = repo_root.join(".git");
    std::fs::create_dir_all(&git).unwrap();
    std::fs::write(git.join("HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
    let refs_heads = git.join("refs").join("heads");
    let branch_path = refs_heads.join(branch);
    if let Some(parent) = branch_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&branch_path, "").unwrap();
}

#[test]
fn infers_status_from_branch_alias() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let tasks = root.join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    let cfg = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
issue.tags: ['*']
branch:
  status_aliases:
    wip: InProgress
"#;
    write_global_with_aliases(&tasks, cfg);
    init_fake_git(root, "wip/doing-work");

    let assert = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .env("LOTAR_TASKS_DIR", tasks.to_string_lossy().to_string())
        .env("LOTAR_TEST_SILENT", "1")
        .env("HOME", root.to_string_lossy().to_string())
        .args([
            "add",
            "Test",
            "--project=TEST",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .success();
    let out = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        out.contains("\"status_value\":\"InProgress\""),
        "Output: {out}"
    );
}

#[test]
fn infers_priority_from_branch_alias() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let tasks = root.join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    let cfg = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High, Critical]
issue.tags: ['*']
branch:
  priority_aliases:
    hotfix: Critical
"#;
    write_global_with_aliases(&tasks, cfg);
    init_fake_git(root, "hotfix/urgent-fix");

    let assert = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .env("LOTAR_TASKS_DIR", tasks.to_string_lossy().to_string())
        .env("LOTAR_TEST_SILENT", "1")
        .env("HOME", root.to_string_lossy().to_string())
        .args([
            "add",
            "Test",
            "--project=TEST",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .success();
    let out = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(out.contains("\"priority\":\"Critical\""), "Output: {out}");
}

#[test]
fn toggles_disable_inference() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let tasks = root.join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    let cfg = r#"default.project: TEST
default.priority: Medium
default.status: Todo
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug]
issue.priorities: [Low, Medium, High]
issue.tags: ['*']
auto.branch_infer_status: false
auto.branch_infer_priority: false
branch:
  status_aliases: { wip: InProgress }
  priority_aliases: { hotfix: High }
"#;
    write_global_with_aliases(&tasks, cfg);
    init_fake_git(root, "wip/something");

    let assert = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .env("LOTAR_TASKS_DIR", tasks.to_string_lossy().to_string())
        .env("LOTAR_TEST_SILENT", "1")
        .env("HOME", root.to_string_lossy().to_string())
        .args([
            "add",
            "Test",
            "--project=TEST",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .success();
    let out = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(out.contains("\"status_value\":\"Todo\""), "Output: {out}");
    assert!(out.contains("\"priority\":\"Medium\""), "Output: {out}");
}

#[test]
fn project_alias_overrides_global() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let tasks = root.join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    // Global says feat -> High, project overrides to Low
    let cfg = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug]
issue.priorities: [Low, Medium, High]
issue.tags: ['*']
branch:
  priority_aliases: { feat: High }
"#;
    write_global_with_aliases(&tasks, cfg);
    let proj_dir = tasks.join("TEST");
    std::fs::create_dir_all(&proj_dir).unwrap();
    let proj_cfg = r#"project.name: TEST
branch:
  priority_aliases: { feat: Low }
"#;
    std::fs::write(proj_dir.join("config.yml"), proj_cfg).unwrap();
    init_fake_git(root, "feat/new");

    let assert = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .env("LOTAR_TASKS_DIR", tasks.to_string_lossy().to_string())
        .env("LOTAR_TEST_SILENT", "1")
        .env("HOME", root.to_string_lossy().to_string())
        .args([
            "add",
            "Test",
            "--project=TEST",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .success();
    let out = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(out.contains("\"priority\":\"Low\""), "Output: {out}");
}
