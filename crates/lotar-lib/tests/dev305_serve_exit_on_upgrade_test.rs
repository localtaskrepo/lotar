//! DEV-305: `lotar serve --exit-on-upgrade` exits (status 0) when the binary
//! it was started from is replaced, here the way Homebrew upgrades: a
//! symlink moves to a new versioned copy. Without the flag it keeps running.
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Install {
    _dir: tempfile::TempDir,
    root: PathBuf,
    link: PathBuf,
}

/// A fake installation: `<root>/0.1.0/lotar` behind `<root>/bin/lotar`.
fn install() -> Install {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let version = root.join("0.1.0");
    std::fs::create_dir_all(&version).unwrap();
    std::fs::copy(crate::common::lotar_bin(), version.join("lotar")).unwrap();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    let link = root.join("bin/lotar");
    std::os::unix::fs::symlink(version.join("lotar"), &link).unwrap();
    std::fs::create_dir_all(root.join("ws/.tasks")).unwrap();
    Install { _dir: dir, root, link }
}

/// Install `0.2.0` and point the symlink at it, as `brew upgrade` does.
fn upgrade(install: &Install) {
    let version = install.root.join("0.2.0");
    std::fs::create_dir_all(&version).unwrap();
    std::fs::copy(crate::common::lotar_bin(), version.join("lotar")).unwrap();
    std::fs::remove_file(&install.link).unwrap();
    std::os::unix::fs::symlink(version.join("lotar"), &install.link).unwrap();
}

fn start(install: &Install, extra: &[&str]) -> Child {
    let mut child = Command::new(&install.link)
        .current_dir(install.root.join("ws"))
        .env("LOTAR_BINARY_WATCH_INTERVAL_MS", "100")
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env_remove("LOTAR_SERVE_EXIT_ON_UPGRADE")
        .args(["serve", "--port", "0", "--tasks-dir", ".tasks"])
        .args(extra)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for_banner(&mut child);
    child
}

/// The `URL:` banner is printed once the server is listening.
fn wait_for_banner(child: &mut Child) {
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("URL:") {
                let _ = tx.send(());
            }
        }
    });
    if rx.recv_timeout(Duration::from_secs(60)).is_err() {
        let _ = child.kill();
        panic!("lotar serve did not start");
    }
}

fn wait_exit(child: &mut Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let start = Instant::now();
    while start.elapsed() < limit {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

fn stderr_of(child: &mut Child) -> String {
    let mut text = String::new();
    if let Some(stderr) = child.stderr.take() {
        let _ = std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut text);
    }
    text
}

#[test]
fn serve_exits_after_an_upgrade_when_asked() {
    let install = install();
    let mut child = start(&install, &["--exit-on-upgrade"]);
    // Running as installed: nothing happens.
    assert!(wait_exit(&mut child, Duration::from_millis(600)).is_none());

    upgrade(&install);
    let status = wait_exit(&mut child, Duration::from_secs(20)).unwrap_or_else(|| {
        let _ = child.kill();
        panic!("lotar serve kept running after the upgrade");
    });
    assert!(status.success(), "{status:?}");
    let stderr = stderr_of(&mut child);
    assert!(stderr.contains("binary changed"), "{stderr}");
}

#[test]
fn serve_keeps_running_after_an_upgrade_by_default() {
    let install = install();
    let mut child = start(&install, &[]);
    upgrade(&install);
    let exited = wait_exit(&mut child, Duration::from_millis(1500));
    let _ = child.kill();
    let _ = child.wait();
    assert!(exited.is_none(), "exited without --exit-on-upgrade: {exited:?}");
}
