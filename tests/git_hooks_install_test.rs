mod common;

use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn copy_dir_all(src: &Path, dst: &Path) {
    assert!(
        src.exists(),
        "source directory '{}' does not exist",
        src.display()
    );

    fs::create_dir_all(dst).expect("failed to create destination directory");
    for entry in fs::read_dir(src).expect("failed to read source directory") {
        let entry = entry.expect("failed to list entry");
        let ty = entry.file_type().expect("failed to get file type");
        let src_path = entry.path();
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&src_path, &dest_path);
        } else if ty.is_file() {
            fs::copy(&src_path, &dest_path).expect("failed to copy file");
        }
    }
}

#[test]
fn installs_git_hooks_and_sets_config() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let hooks_src = repo_root.join(".githooks");
    let hooks_dest = temp.path().join(".githooks");

    copy_dir_all(&hooks_src, &hooks_dest);

    let status = Command::new("git")
        .current_dir(temp.path())
        .args(["init", "."])
        .status()
        .expect("failed to run git init");
    assert!(status.success(), "git init failed");

    let mut cmd = crate::common::lotar_cmd().expect("binary not built");
    cmd.current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["git", "hooks", "install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("core.hooksPath"));

    let output = Command::new("git")
        .current_dir(temp.path())
        .args(["config", "--local", "--get", "core.hooksPath"])
        .output()
        .expect("failed to read git config");
    assert!(output.status.success(), "git config --get failed");
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(value, ".githooks");

    // Second run should be a no-op and still succeed.
    let mut second = crate::common::lotar_cmd().expect("binary not built");
    second
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["git", "hooks", "install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already configured"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let script_path = temp.path().join(".githooks").join("pre-commit");
        let metadata = fs::metadata(&script_path).expect("missing pre-commit script");
        assert_ne!(
            metadata.permissions().mode() & 0o111,
            0,
            "script should be executable"
        );
    }
}
