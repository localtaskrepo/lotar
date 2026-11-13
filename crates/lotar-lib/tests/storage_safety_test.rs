//! Storage locks, atomic writes and project prefixes; run natively on Windows CI (DEV-495).

use fs2::FileExt;
use lotar::storage::safety::{atomic_write_file, atomic_write_file_with_io, is_lock_contention, is_valid_project_prefix, sprint_lock_name, with_storage_lock};
use std::fs::{self, File};
use std::io::Write;
use std::time::Duration;

#[cfg(unix)]
#[test]
fn atomic_write_preserves_private_permissions_before_writing_and_after_publish() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("config.yml");
    fs::write(&target, "old private value").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    atomic_write_file_with_io(
        &target,
        |file| {
            assert_eq!(file.metadata()?.len(), 0);
            assert_eq!(file.metadata()?.permissions().mode() & 0o777, 0o600);
            file.write_all(b"new private value")?;
            file.sync_all()
        },
        |temp, destination| fs::rename(temp, destination),
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "new private value");
    assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
    atomic_write_file(&target, "replacement private value").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "replacement private value");
    assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
}

#[cfg(unix)]
#[test]
fn atomic_write_new_file_uses_normal_creation_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let normal = dir.path().join("normal.yml");
    let target = dir.path().join("new.yml");
    fs::write(&normal, "normal").unwrap();
    atomic_write_file(&target, "new").unwrap();
    assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, fs::metadata(&normal).unwrap().permissions().mode() & 0o777);
}

#[cfg(unix)]
#[test]
fn atomic_write_permission_lookup_error_never_prepares_or_publishes() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("config.yml");
    symlink("config.yml", &target).unwrap();
    assert!(
        atomic_write_file_with_io(
            &target,
            |_| panic!("unreadable destination metadata must prevent writing"),
            |_, _| panic!("unreadable destination metadata must prevent publishing"),
        )
        .is_err()
    );
    assert_eq!(fs::read_link(&target).unwrap(), std::path::Path::new("config.yml"));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn held_lock_times_out_without_running_callback_or_removing_temp_files() {
    let _lock_wait = crate::env_mutex::EnvVarGuard::set(lotar::storage::safety::STORAGE_LOCK_WAIT_ENV, "200");
    let dir = tempfile::tempdir().unwrap();
    let held = File::create(dir.path().join(".task.lock")).unwrap();
    held.lock_exclusive().unwrap();
    let orphan = dir.path().join(".7.yml.tmp-held-0");
    fs::write(&orphan, "untouched while locked").unwrap();
    let mut ran = false;
    let error = with_storage_lock(dir.path(), "task", || {
        ran = true;
        Ok::<_, std::io::Error>(())
    })
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    assert!(error.to_string().contains("retry"));
    assert!(!ran);
    assert!(orphan.exists());
    drop(held);
    with_storage_lock(dir.path(), "task", || Ok::<_, std::io::Error>(())).unwrap();
    assert!(orphan.exists());
}

#[test]
fn native_windows_lock_code_is_contention_even_without_would_block_kind() {
    // fs2 uses ERROR_LOCK_VIOLATION (33), whose ErrorKind is not WouldBlock.
    let error = std::io::Error::from_raw_os_error(33);
    assert!(is_lock_contention(&error, Some(33)));
}

#[test]
fn contention_matches_only_would_block_or_the_platform_native_code() {
    let platform = fs2::lock_contended_error();
    assert!(is_lock_contention(&platform, platform.raw_os_error()));
    assert!(is_lock_contention(&std::io::Error::new(std::io::ErrorKind::WouldBlock, "busy"), None,));
    assert!(!is_lock_contention(&std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"), Some(33),));
    assert!(!is_lock_contention(&std::io::Error::other("unknown"), None));
    assert!(!is_lock_contention(&std::io::Error::from_raw_os_error(5), Some(33)));
}

#[test]
fn storage_lock_retries_until_holder_releases() {
    let dir = tempfile::tempdir().unwrap();
    let held = File::create(dir.path().join(".task.lock")).unwrap();
    held.lock_exclusive().unwrap();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(held);
        });
        with_storage_lock(dir.path(), "task", || Ok::<_, std::io::Error>(())).unwrap();
    });
}

#[test]
fn storage_lock_open_failure_never_runs_callback() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".task.lock")).unwrap();
    let error = with_storage_lock(dir.path(), "task", || -> std::io::Result<()> {
        panic!("callback must not run without a lock");
    })
    .unwrap_err();
    assert!(error.to_string().contains("Cannot open storage lock"));
    assert_ne!(error.kind(), std::io::ErrorKind::WouldBlock);
}

#[test]
fn storage_lock_releases_on_error_and_panic() {
    let dir = tempfile::tempdir().unwrap();
    let result = with_storage_lock(dir.path(), "task", || Err::<(), _>(std::io::Error::other("mutation failed")));
    assert!(result.is_err());
    let probe = File::open(dir.path().join(".task.lock")).unwrap();
    probe.try_lock_exclusive().unwrap();
    drop(probe);
    assert!(
        std::panic::catch_unwind(|| {
            let _ = with_storage_lock(dir.path(), "task", || -> std::io::Result<()> {
                panic!("mutation panic");
            });
        })
        .is_err()
    );
    let probe = File::open(dir.path().join(".task.lock")).unwrap();
    probe.try_lock_exclusive().unwrap();
}

#[test]
fn accepts_realistic_project_prefixes() {
    for ok in ["DEV", "DEMO", "AUTH", "TP", "My_Project-2", "ÜBER"] {
        assert!(is_valid_project_prefix(ok), "expected valid: {ok}");
    }
}

#[test]
fn rejects_traversal_and_reserved_prefixes() {
    for bad in ["..", ".", "../evil", "../../etc", "/tmp/abs", "/abs", "a/b", "a\\b", "a b", "@sprints", ".hidden", "-flag", "", "a..b"] {
        assert!(!is_valid_project_prefix(bad), "expected invalid: {bad}");
    }
}

#[test]
fn atomic_write_replaces_content_and_leaves_no_temp_files() {
    let dir = std::env::temp_dir().join(format!("lotar-safety-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("5.yml");
    atomic_write_file(&target, "first").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "first");
    atomic_write_file(&target, "second").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "second");
    let leftovers: Vec<_> =
        fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_str().map(|n| n.contains(".tmp-")).unwrap_or(false)).collect();
    assert!(leftovers.is_empty(), "temp files left behind");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn atomic_write_publish_failure_preserves_destination_and_unrelated_temp() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("journal.yml");
    let unrelated = dir.path().join(".journal.yml.tmp-other-writer");
    fs::write(&target, "old journal").unwrap();
    fs::write(&unrelated, "other writer").unwrap();
    let error = atomic_write_file_with_io(
        &target,
        |file| {
            file.write_all(b"new journal")?;
            file.sync_all()
        },
        |temp, destination| {
            assert_eq!(fs::read_to_string(temp)?, "new journal");
            assert_eq!(fs::read_to_string(destination)?, "old journal");
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "publish denied"))
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(fs::read_to_string(&target).unwrap(), "old journal");
    assert_eq!(fs::read_to_string(&unrelated).unwrap(), "other writer");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn atomic_write_prepare_failure_preserves_destination_and_cleans_partial_temp() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("journal.yml");
    fs::write(&target, "old journal").unwrap();
    let error = atomic_write_file_with_io(
        &target,
        |file| {
            file.write_all(b"partial")?;
            Err(std::io::Error::other("injected write or sync failure"))
        },
        |_, _| panic!("failed temp preparation must never publish"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("injected"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "old journal");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn atomic_write_readonly_destination_fails_without_removing_old_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("journal.yml");
    fs::write(&target, "old journal").unwrap();
    let original_permissions = fs::metadata(&target).unwrap().permissions();
    let mut readonly = original_permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&target, readonly).unwrap();
    let result = atomic_write_file(&target, "new journal");
    let retained = fs::read_to_string(&target);
    fs::set_permissions(&target, original_permissions).unwrap();
    assert!(result.is_err());
    assert_eq!(retained.unwrap(), "old journal");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn storage_lock_runs_closure_and_creates_lock_file() {
    let dir = std::env::temp_dir().join(format!("lotar-lock-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let result = with_storage_lock(&dir, "task", || Ok::<_, std::io::Error>(41 + 1)).unwrap();
    assert_eq!(result, 42);
    assert!(dir.join(".task.lock").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn lock_acquisition_preserves_live_atomic_writer_temp_and_rename() {
    let dir = tempfile::tempdir().unwrap();
    for file_name in ["config.yml", "7.yml", "other.yml"] {
        let target = dir.path().join(file_name);
        let temp = dir.path().join(format!(".{file_name}.tmp-{}-live", std::process::id()));
        fs::write(&target, "previous value").unwrap();
        // Deterministically pause an atomic writer between write and rename.
        fs::write(&temp, "active writer value").unwrap();
        for lock_name in ["task", sprint_lock_name()] {
            with_storage_lock(dir.path(), lock_name, || -> std::io::Result<()> {
                assert_eq!(fs::read_to_string(&temp)?, "active writer value");
                Ok(())
            })
            .unwrap();
        }
        fs::rename(&temp, &target).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "active writer value");
    }
}
