use serde_json::Value;
use std::process::Command as ProcCommand;
use tempfile::TempDir;

mod common;

fn run_git(repo: &std::path::Path, args: &[&str], envs: &[(&str, &str)]) {
    let mut cmd = ProcCommand::new("git");
    cmd.current_dir(repo).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("failed to run git");
    if !out.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn init_repo(temp: &TempDir) {
    let root = temp.path();
    run_git(root, &["init"], &[]);
    run_git(root, &["config", "user.name", "Test User"], &[]);
    run_git(root, &["config", "user.email", "test@example.com"], &[]);
    run_git(root, &["config", "commit.gpgsign", "false"], &[]);
}

fn write_file(root: &std::path::Path, rel: &str, content: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
}

fn add_and_commit(
    repo: &std::path::Path,
    rel: &str,
    author: (&str, &str),
    date_rfc3339: &str,
    msg: &str,
) {
    run_git(repo, &["add", rel], &[]);
    let envs = [
        ("GIT_AUTHOR_NAME", author.0),
        ("GIT_AUTHOR_EMAIL", author.1),
        ("GIT_COMMITTER_NAME", author.0),
        ("GIT_COMMITTER_EMAIL", author.1),
        ("GIT_AUTHOR_DATE", date_rfc3339),
        ("GIT_COMMITTER_DATE", date_rfc3339),
    ];
    run_git(repo, &["commit", "-m", msg], &envs);
}

/// Tests that need a real Git repository (see `common::require_git`).
/// The `git_required` module path is what the gitless nextest profile
/// excludes when the runtime Git capability probe fails (DEV-79).
mod git_required {
    use super::*;

    #[test]
    fn task_history_diff_at() {
        crate::common::require_git();
        let temp = crate::common::temp_dir();
        let root = temp.path();
        init_repo(&temp);

        write_file(root, ".tasks/TEST/config.yml", "project_name: TEST\n");
        write_file(
            root,
            ".tasks/TEST/1.yml",
            "title: One\nstatus: TODO\nmodified: 2025-08-01T10:00:00Z\n",
        );
        add_and_commit(
            root,
            ".tasks/TEST/1.yml",
            ("A", "a@example.com"),
            "2025-08-01T10:00:00Z",
            "add 1",
        );
        write_file(
            root,
            ".tasks/TEST/1.yml",
            "title: One2\nstatus: IN_PROGRESS\nmodified: 2025-08-02T09:00:00Z\n",
        );
        add_and_commit(
            root,
            ".tasks/TEST/1.yml",
            ("B", "b@example.com"),
            "2025-08-02T09:00:00Z",
            "edit 1",
        );

        // History JSON
        let out = crate::common::lotar_cmd()
            .unwrap()
            .current_dir(root)
            .args(["--format", "json", "task", "history", "TEST-1", "-L", "5"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], "ok");
        let items = v["items"].as_array().unwrap();
        assert!(items.len() >= 2);

        // Diff JSON (latest)
        let out = crate::common::lotar_cmd()
            .unwrap()
            .current_dir(root)
            .args(["--format", "json", "task", "diff", "TEST-1"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(v["patch"].as_str().unwrap().contains("diff"));

        // At JSON (first commit)
        // Extract last item commit (oldest) from history
        let first_commit = items.last().unwrap()["commit"].as_str().unwrap();
        let out = crate::common::lotar_cmd()
            .unwrap()
            .current_dir(root)
            .args(["--format", "json", "task", "at", "TEST-1", first_commit])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(v["content"].as_str().unwrap().contains("title: One"));
    }

    /// DEV-217: a freshly initialised repository has no commits yet; its
    /// tasks simply have no history instead of failing.
    #[test]
    fn task_history_is_empty_before_the_first_commit() {
        crate::common::require_git();
        let temp = crate::common::temp_dir();
        let root = temp.path();
        init_repo(&temp);
        write_file(root, ".tasks/TEST/config.yml", "project_name: TEST\n");
        write_file(
            root,
            ".tasks/TEST/1.yml",
            "title: One\nstatus: TODO\nmodified: 2025-08-01T10:00:00Z\n",
        );

        let out = crate::common::lotar_cmd()
            .unwrap()
            .current_dir(root)
            .args(["--format", "json", "task", "history", "TEST-1"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["items"].as_array().unwrap().len(), 0);
    }
}

/// DEV-95: the REST history endpoint and the CLI must resolve the
/// repo-relative task path the same way when the tasks directory is reached
/// through a symlink (for example macOS `/tmp` -> `/private/tmp`).
#[cfg(unix)]
mod symlinked_workspace {
    use super::*;

    mod git_required {
        use super::*;
        use crate::common::env_mutex::{EnvVarGuard, lock_var};
        use lotar::api_server::{ApiServer, HttpRequest};
        use std::collections::HashMap;

        #[test]
        fn rest_and_cli_history_agree_through_symlinked_tasks_dir() {
            crate::common::require_git();
            let temp = crate::common::temp_dir();
            let repo = std::fs::canonicalize(temp.path()).unwrap();
            init_repo(&temp);
            write_file(&repo, ".tasks/TEST/config.yml", "project_name: TEST\n");
            write_file(&repo, ".tasks/TEST/1.yml", "title: One\nstatus: TODO\n");
            add_and_commit(
                &repo,
                ".tasks/TEST/1.yml",
                ("A", "a@example.com"),
                "2025-08-01T10:00:00Z",
                "add 1",
            );

            // Reach the same workspace through a symlink, as with /tmp on macOS.
            let link_parent = crate::common::temp_dir();
            let link = link_parent.path().join("workspace-link");
            std::os::unix::fs::symlink(&repo, &link).unwrap();
            let linked_tasks = link.join(".tasks");

            let cli = crate::common::lotar_cmd()
                .unwrap()
                .current_dir(&repo)
                .env("LOTAR_TASKS_DIR", &linked_tasks)
                .args(["--format", "json", "task", "history", "TEST-1"])
                .output()
                .unwrap();
            assert!(
                cli.status.success(),
                "CLI history failed: {}",
                String::from_utf8_lossy(&cli.stderr)
            );
            let cli_json: Value = serde_json::from_slice(&cli.stdout).unwrap();
            let cli_commits: Vec<String> = cli_json["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["commit"].as_str().unwrap().to_string())
                .collect();
            assert_eq!(cli_commits.len(), 1, "CLI output: {cli_json}");

            // The server derives the repo root from its (canonical) working
            // directory while the tasks dir is the symlinked path.
            let _cwd_lock = lock_var("LOTAR_CWD");
            let _tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &linked_tasks.to_string_lossy());
            let prev_cwd = std::env::current_dir().unwrap();
            std::env::set_current_dir(&repo).unwrap();
            let mut api = ApiServer::new();
            lotar::routes::initialize(&mut api);
            let mut query = HashMap::new();
            query.insert("id".to_string(), "TEST-1".to_string());
            let resp = api.handle_request(&HttpRequest {
                method: "GET".into(),
                path: "/api/tasks/history".into(),
                query,
                headers: HashMap::new(),
                body: Vec::new(),
            });
            std::env::set_current_dir(prev_cwd).unwrap();

            let body: Value = serde_json::from_slice(&resp.body).unwrap();
            assert_eq!(resp.status, 200, "REST history failed: {body}");
            let rest_commits: Vec<String> = body["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["commit"].as_str().unwrap().to_string())
                .collect();
            assert_eq!(rest_commits, cli_commits);
        }
    }
}
