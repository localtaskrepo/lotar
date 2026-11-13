// DEV-319: the runtime .gitignore LoTaR writes keeps agent job files out of git.
use std::process::Command;

#[test]
fn runtime_gitignore_ignores_job_context_files() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    assert!(Command::new("git").arg("init").arg("-q").current_dir(repo).status().unwrap().success());
    let tasks = repo.join(".tasks");
    std::fs::create_dir_all(tasks.join("STD")).unwrap();
    lotar::storage::manager::ensure_runtime_gitignore(&tasks);
    std::fs::write(tasks.join("STD/STD-22.context"), "{}").unwrap();
    std::fs::write(tasks.join("STD/22.yml"), "title: x\n").unwrap();
    let ignored = |path: &str| Command::new("git").args(["check-ignore", "-q", path]).current_dir(repo).status().unwrap().success();
    assert!(ignored(".tasks/STD/STD-22.context"), "job context files are runtime state");
    assert!(!ignored(".tasks/STD/22.yml"), "tickets stay tracked");
}
