//! DEV-861: a test job that waits for a release file ends once the test
//! process is gone, also when no Drop wrote the file (a failed or killed
//! test). Before, such loops and the detached worker waiting on them ran
//! until reboot.
#![cfg(unix)]

use crate::common;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn exited_within(child: &mut Child, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn a_release_loop_ends_when_its_test_process_is_gone() {
    let dir = common::temp_dir();
    let release = dir.path().join("release");
    // Stands in for the test process: killed, so it writes no release file.
    let mut owner = Command::new("sleep").arg("60").spawn().unwrap();
    let mut job = Command::new("sh")
        .args(["-c", &format!("while [ ! -f '{}' ] && {}; do sleep 0.05; done", release.display(), common::while_process_runs(owner.id()))])
        .spawn()
        .unwrap();

    assert!(!exited_within(&mut job, Duration::from_millis(500)), "the loop ended while its test still ran");
    owner.kill().unwrap();
    owner.wait().unwrap();
    let ended = exited_within(&mut job, Duration::from_secs(10));
    if !ended {
        let _ = job.kill();
    }
    assert!(ended, "the loop kept running after its test process was gone");
    assert!(!release.exists());
}
