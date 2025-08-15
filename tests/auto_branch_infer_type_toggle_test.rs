use assert_cmd::Command;
use tempfile::TempDir;

fn write_global(tasks_dir: &std::path::Path, enable: bool) {
    let content = format!(
        "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\nissue.tags: [*]\nauto.branch_infer_type: {}\n",
        if enable { "true" } else { "false" }
    );
    std::fs::write(tasks_dir.join("config.yml"), content).unwrap();
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
fn disables_branch_infer_when_flag_off() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let tasks = root.join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    write_global(&tasks, false);
    init_fake_git(root, "fix/example");

    // Also add a project to restrict allowed types to Feature, forcing default behavior
    let proj = tasks.join("TEST");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("config.yml"),
        "project.id: TEST\nissue.types: [Feature]\n",
    )
    .unwrap();

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
fn enables_branch_infer_when_flag_on() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let tasks = root.join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    write_global(&tasks, true);
    init_fake_git(root, "fix/example");

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
