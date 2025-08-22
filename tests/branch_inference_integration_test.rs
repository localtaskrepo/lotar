use assert_cmd::Command;
use tempfile::TempDir;

mod common;

fn write_global(tasks_dir: &std::path::Path) {
    // Allow Feature/Bug/Chore so inference can map
    let content = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
issue.tags: [*]
"#;
    std::fs::write(tasks_dir.join("config.yml"), content).unwrap();
}

fn init_fake_git(repo_root: &std::path::Path, branch: &str) {
    // Create .git dir and set HEAD ref like a normal repo
    let git = repo_root.join(".git");
    std::fs::create_dir_all(&git).unwrap();
    std::fs::write(git.join("HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
    let refs_heads = git.join("refs").join("heads");
    // Ensure full path for branch like feat/foo exists
    let branch_path = refs_heads.join(branch);
    if let Some(parent) = branch_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    // an empty file for the branch ref is enough for our read_current_branch
    std::fs::write(&branch_path, "").unwrap();
}

#[test]
fn infers_feature_on_feat_branch() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let tasks = root.join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    write_global(&tasks);
    init_fake_git(root, "feat/api-endpoint");

    let assert = Command::cargo_bin("lotar")
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
    assert!(out.contains("\"type\":\"Feature\""), "Output: {out}");
}

#[test]
fn infers_bug_on_fix_branch() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let tasks = root.join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    write_global(&tasks);
    init_fake_git(root, "fix/login-crash");

    let assert = Command::cargo_bin("lotar")
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
    assert!(out.contains("\"type\":\"Bug\""), "Output: {out}");
}

#[test]
fn falls_back_when_type_not_allowed() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let tasks = root.join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    // Only Feature allowed to force fallback
    let content = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature]
issue.priorities: [Low, Medium, High]
issue.tags: [*]
"#;
    std::fs::write(tasks.join("config.yml"), content).unwrap();
    init_fake_git(root, "fix/should-fallback");

    // Also add a project-specific config to be explicit about allowed types
    let proj_dir = tasks.join("TEST");
    std::fs::create_dir_all(&proj_dir).unwrap();
    let proj_cfg = r#"project.id: TEST
issue.types: [Feature]
"#;
    std::fs::write(proj_dir.join("config.yml"), proj_cfg).unwrap();

    let assert = Command::cargo_bin("lotar")
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
    assert!(out.contains("\"type\":\"Feature\""), "Output: {out}");
}
