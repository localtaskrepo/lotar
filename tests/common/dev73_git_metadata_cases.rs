//! Exercise the production readers with ordinary entry-path names, not protected
//! Git markers. No Git processes or checkout metadata are touched by these cases.
use super::{metadata_dirs, read_branch_at, read_remotes_at};
use std::fs;
use std::path::PathBuf;

use crate::test_env;

const CONFIG: &str = "[user]\n\tname = Fixture User\n\temail = fixture@example.test\n[remote \"origin\"]\n\turl = https://example.test/shared.git\n[remote \"upstream\"]\n\turl = ssh://example.test/upstream.git\n";

struct Fixture {
    _tmp: tempfile::TempDir,
    normal_entry: PathBuf,
    linked_entry: PathBuf,
    admin: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let normal_entry = tmp.path().join("primary repository/metadata");
    let linked = tmp.path().join("linked workspace");
    let admin = normal_entry.join("worktrees/linked");
    fs::create_dir_all(&admin).unwrap();
    fs::create_dir_all(&linked).unwrap();
    fs::write(
        normal_entry.join("HEAD"),
        "ref: refs/heads/feature/shared\n",
    )
    .unwrap();
    fs::write(normal_entry.join("config"), CONFIG).unwrap();
    fs::write(admin.join("HEAD"), "ref: refs/heads/fix/linked\n").unwrap();
    fs::write(admin.join("commondir"), "../..\n").unwrap();
    let linked_entry = linked.join("metadata-entry");
    fs::write(
        &linked_entry,
        "gitdir: ../primary repository/metadata/worktrees/linked\n",
    )
    .unwrap();
    Fixture {
        _tmp: tmp,
        normal_entry,
        linked_entry,
        admin,
    }
}

fn remotes() -> Vec<String> {
    vec![
        "https://example.test/shared.git".into(),
        "ssh://example.test/upstream.git".into(),
    ]
}

#[test]
fn normal_metadata_reads_preserve_all_files() {
    let f = fixture();
    let files = [
        f.normal_entry.join("HEAD"),
        f.normal_entry.join("config"),
        f.linked_entry.clone(),
        f.admin.join("HEAD"),
        f.admin.join("commondir"),
    ];
    let before: Vec<_> = files.iter().map(|path| fs::read(path).unwrap()).collect();
    assert_eq!(
        read_branch_at(&f.normal_entry).as_deref(),
        Some("feature/shared")
    );
    assert_eq!(read_remotes_at(&f.normal_entry), remotes());
    assert_eq!(
        files
            .iter()
            .map(|path| fs::read(path).unwrap())
            .collect::<Vec<_>>(),
        before
    );
}

#[test]
fn relative_gitdir_and_common_paths_use_their_own_bases() {
    let f = fixture();
    assert_eq!(
        read_branch_at(&f.linked_entry).as_deref(),
        Some("fix/linked")
    );
    assert_eq!(read_remotes_at(&f.linked_entry), remotes());
    let (admin, common) = metadata_dirs(&f.linked_entry).unwrap();
    assert_eq!(admin, fs::canonicalize(&f.admin).unwrap());
    assert_eq!(common, fs::canonicalize(&f.normal_entry).unwrap());
}

#[test]
fn absolute_gitdir_and_common_paths_support_crlf_and_spaces() {
    let f = fixture();
    fs::write(
        &f.linked_entry,
        format!("gitdir: {}\r\n", f.admin.display()),
    )
    .unwrap();
    fs::write(
        f.admin.join("commondir"),
        format!("{}\r\n", f.normal_entry.display()),
    )
    .unwrap();
    fs::write(f.admin.join("HEAD"), "ref: refs/heads/hotfix/absolute\r\n").unwrap();
    assert_eq!(
        read_branch_at(&f.linked_entry).as_deref(),
        Some("hotfix/absolute")
    );
    assert_eq!(read_remotes_at(&f.linked_entry), remotes());
}

#[test]
fn gitdir_without_commondir_uses_its_own_config() {
    let f = fixture();
    fs::remove_file(f.admin.join("commondir")).unwrap();
    fs::write(
        f.admin.join("config"),
        "[remote \"local\"]\nurl = https://example.test/local.git\n",
    )
    .unwrap();
    assert_eq!(
        read_branch_at(&f.linked_entry).as_deref(),
        Some("fix/linked")
    );
    assert_eq!(
        read_remotes_at(&f.linked_entry),
        ["https://example.test/local.git"]
    );
}

#[test]
fn entries_keep_independent_heads_and_refresh_shared_remotes() {
    let f = fixture();
    let second = f._tmp.path().join("second-entry");
    let admin = f.normal_entry.join("worktrees/second");
    fs::create_dir_all(&admin).unwrap();
    fs::write(&second, format!("gitdir: {}\n", admin.display())).unwrap();
    fs::write(admin.join("HEAD"), "ref: refs/heads/chore/second\n").unwrap();
    fs::write(admin.join("commondir"), "../..\n").unwrap();
    assert_eq!(read_branch_at(&second).as_deref(), Some("chore/second"));
    assert_eq!(
        read_branch_at(&f.linked_entry).as_deref(),
        Some("fix/linked")
    );
    assert_eq!(
        read_branch_at(&f.normal_entry).as_deref(),
        Some("feature/shared")
    );
    assert_eq!(read_remotes_at(&second), remotes());
    fs::write(
        f.normal_entry.join("config"),
        "[remote \"origin\"]\nurl = https://example.test/changed.git\n",
    )
    .unwrap();
    assert_eq!(
        read_remotes_at(&f.linked_entry),
        ["https://example.test/changed.git"]
    );
}

#[test]
fn malformed_or_missing_gitdir_never_falls_back() {
    let f = fixture();
    for value in [
        "",
        "gitdir:",
        "gitdir: \n",
        "not a git marker\n",
        "gitdir: missing\n",
        "gitdir: ../primary repository/metadata/worktrees/linked\nextra\n",
    ] {
        fs::write(&f.linked_entry, value).unwrap();
        assert_eq!(read_branch_at(&f.linked_entry), None, "{value:?}");
        assert!(read_remotes_at(&f.linked_entry).is_empty(), "{value:?}");
    }
    fs::write(&f.linked_entry, b"gitdir: \xff\n").unwrap();
    assert_eq!(metadata_dirs(&f.linked_entry), None);
}

#[test]
fn malformed_common_directory_fails_closed_even_with_a_valid_head() {
    let f = fixture();
    for value in ["", "\n", "missing\n", "../..\nextra\n", "../..\0\n"] {
        fs::write(f.admin.join("commondir"), value).unwrap();
        assert_eq!(read_branch_at(&f.linked_entry), None, "{value:?}");
        assert!(read_remotes_at(&f.linked_entry).is_empty(), "{value:?}");
    }
}

#[test]
fn detached_nonhead_empty_and_malformed_heads_return_none() {
    let f = fixture();
    for value in [
        "",
        "0123456789abcdef\n",
        "ref: refs/tags/release\n",
        "ref: refs/heads/\n",
        "ref: refs/heads/fix/with spaces\n",
        "ref: refs/heads/fix/valid\nextra\n",
    ] {
        fs::write(f.normal_entry.join("HEAD"), value).unwrap();
        assert_eq!(read_branch_at(&f.normal_entry), None, "{value:?}");
    }
    fs::remove_file(f.normal_entry.join("HEAD")).unwrap();
    assert_eq!(read_branch_at(&f.normal_entry), None);
}

#[test]
fn metadata_reads_ignore_repository_routing_environment() {
    let f = fixture();
    let _git_dir = test_env::EnvVarGuard::set("GIT_DIR", &f.normal_entry.to_string_lossy());
    let _common_dir =
        test_env::EnvVarGuard::set("GIT_COMMON_DIR", &f.normal_entry.to_string_lossy());
    assert_eq!(
        read_branch_at(&f.linked_entry).as_deref(),
        Some("fix/linked")
    );
    assert_eq!(read_remotes_at(&f.linked_entry), remotes());
}

#[test]
fn common_configuration_is_selected_for_identity_context() {
    let f = fixture();
    let (_, common) = metadata_dirs(&f.linked_entry).unwrap();
    assert_eq!(fs::read_to_string(common.join("config")).unwrap(), CONFIG);
    assert_eq!(
        read_branch_at(&f.linked_entry).as_deref(),
        Some("fix/linked")
    );
}

#[cfg(unix)]
#[test]
fn path_payloads_preserve_legitimate_trailing_spaces() {
    let f = fixture();
    let admin = f.admin.with_file_name("linked admin ");
    fs::rename(&f.admin, &admin).unwrap();
    fs::write(&f.linked_entry, format!("gitdir: {}\n", admin.display())).unwrap();
    assert_eq!(
        read_branch_at(&f.linked_entry).as_deref(),
        Some("fix/linked")
    );
    assert_eq!(read_remotes_at(&f.linked_entry), remotes());
}

#[test]
fn identity_fingerprint_tracks_shared_config_and_worktree_head_without_time_waits() {
    use crate::utils::identity::git_cache_fingerprint;
    let f = fixture();
    let future = std::time::UNIX_EPOCH + std::time::Duration::from_secs(4_102_444_800);
    let stamp = |path: &std::path::Path, offset| {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(
                fs::FileTimes::new().set_modified(future + std::time::Duration::from_secs(offset)),
            )
            .unwrap();
    };
    stamp(&f.normal_entry.join("config"), 10);
    stamp(&f.admin.join("HEAD"), 10);
    let first = git_cache_fingerprint(&f.linked_entry);
    assert_eq!(git_cache_fingerprint(&f.linked_entry), first);
    stamp(&f.normal_entry.join("config"), 20);
    let config_changed = git_cache_fingerprint(&f.linked_entry);
    assert_ne!(config_changed, first);
    stamp(&f.admin.join("HEAD"), 30);
    assert_ne!(git_cache_fingerprint(&f.linked_entry), config_changed);
}

#[test]
fn identity_fingerprint_changes_when_metadata_target_changes() {
    use crate::utils::identity::git_cache_fingerprint;
    let f = fixture();
    let first = git_cache_fingerprint(&f.linked_entry);
    let other = f._tmp.path().join("other metadata");
    fs::create_dir_all(&other).unwrap();
    for file in ["config", "HEAD"] {
        let source = if file == "HEAD" {
            f.admin.join(file)
        } else {
            f.normal_entry.join(file)
        };
        let destination = other.join(file);
        fs::copy(&source, &destination).unwrap();
        fs::File::options()
            .write(true)
            .open(&destination)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(fs::metadata(source).unwrap().modified().unwrap()),
            )
            .unwrap();
    }
    fs::write(&f.linked_entry, format!("gitdir: {}\n", other.display())).unwrap();
    assert_ne!(git_cache_fingerprint(&f.linked_entry), first);
}

#[cfg(unix)]
#[test]
fn dangling_common_symlink_is_not_treated_as_absent() {
    let f = fixture();
    fs::remove_file(f.admin.join("commondir")).unwrap();
    std::os::unix::fs::symlink("missing", f.admin.join("commondir")).unwrap();
    fs::write(f.admin.join("config"), CONFIG).unwrap();
    assert_eq!(read_branch_at(&f.linked_entry), None);
    assert!(read_remotes_at(&f.linked_entry).is_empty());
}

#[cfg(unix)]
#[test]
fn metadata_directory_symlinks_keep_normal_behavior() {
    let f = fixture();
    let entry = f._tmp.path().join("symlink-entry");
    std::os::unix::fs::symlink(&f.normal_entry, &entry).unwrap();
    assert_eq!(read_branch_at(&entry).as_deref(), Some("feature/shared"));
    assert_eq!(read_remotes_at(&entry), remotes());
}
