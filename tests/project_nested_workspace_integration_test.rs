use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// Reuse a simple CWD lock to avoid parallel test interference
static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_cwd(dir: &PathBuf, f: impl FnOnce()) {
    let lock = CWD_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap();
    let old = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir).unwrap();
    f();
    std::env::set_current_dir(old).unwrap();
}

#[test]
fn prefers_nearest_manifest_across_types() {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();

    // Outer manifests
    let outer = repo.join("package.json");
    fs::write(&outer, r#"{ "name": "outer-monorepo" }"#).unwrap();

    // Nested path: packages/api/server
    let server = repo.join("packages").join("api").join("server");
    fs::create_dir_all(&server).unwrap();

    // packages/api has package.json
    fs::create_dir_all(server.parent().unwrap()).unwrap();
    fs::write(
        server.parent().unwrap().join("package.json"),
        r#"{ "name": "api" }"#,
    )
    .unwrap();

    // server has its own Cargo.toml
    fs::write(
        server.join("Cargo.toml"),
        r#"[package]
name = "server"
version = "0.1.0"
"#,
    )
    .unwrap();

    with_cwd(&server, || {
        let name = lotar::project::detect_project_name();
        // Nearest manifest (Cargo.toml in server) wins
        assert_eq!(name.as_deref(), Some("server"));
    });
}

#[test]
fn stops_at_repo_root_even_if_parent_has_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let outer = tmp.path().join("outer");
    fs::create_dir_all(&outer).unwrap();
    // Parent above repo has a manifest we must NOT use
    fs::write(outer.join("Cargo.toml"), "[package]\nname=\"outer\"\n").unwrap();

    // Repo root uses a .git FILE to simulate worktree/submodule
    let repo = outer.join("repo");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join(".git"), "gitdir: /tmp/somewhere").unwrap();

    // Inside repo, nested directory with no manifest; but repo root has package.json
    fs::write(repo.join("package.json"), r#"{ "name": "inner-monorepo" }"#).unwrap();
    let nested = repo.join("apps").join("web");
    fs::create_dir_all(&nested).unwrap();

    with_cwd(&nested, || {
        let name = lotar::project::detect_project_name();
        // Must stop at repo root and pick inner-monorepo, not outer
        assert_eq!(name.as_deref(), Some("inner-monorepo"));
    });
}

#[test]
fn submodule_like_repo_uses_inner_git_file_root() {
    let tmp = tempfile::TempDir::new().unwrap();
    let outer = tmp.path().join("outer");
    fs::create_dir_all(outer.join(".git")).unwrap();

    // Outer repo has a name
    fs::write(outer.join("package.json"), r#"{ "name": "outer-repo" }"#).unwrap();

    // Inner path simulating a submodule: .git FILE pointing elsewhere
    let sub = outer.join("vendor").join("lib");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join(".git"), "gitdir: ../.git/modules/lib").unwrap();

    // Inner repo manifest
    fs::write(sub.join("Cargo.toml"), "[package]\nname=\"lib\"\n").unwrap();

    // Work inside a deeper folder of the submodule
    let deep = sub.join("src");
    fs::create_dir_all(&deep).unwrap();

    with_cwd(&deep, || {
        let name = lotar::project::detect_project_name();
        assert_eq!(name.as_deref(), Some("lib"));
    });
}
