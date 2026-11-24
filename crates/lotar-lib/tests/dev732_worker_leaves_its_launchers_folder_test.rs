//! DEV-732: the queue worker a command starts doesn't keep that command's
//! folder as its working directory. A worker started from a job's worktree
//! kept it after the worktree was removed, and every Claude session it then
//! started failed on `process.cwd`.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use lotar::services::automation_service::AutomationService;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn wait_for(path: &Path, limit: Duration) -> String {
    let deadline = Instant::now() + limit;
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && text.ends_with('\n')
        {
            return text.trim().to_string();
        }
        assert!(Instant::now() < deadline, "the job did not record its worker's folder");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_worker_started_from_another_folder_works_in_the_tasks_folders_parent() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let launcher = root.join("launcher");
    std::fs::create_dir_all(&launcher).unwrap();
    let record = root.join("worker-cwd");
    let worker_pid = root.join("worker-pid");
    // The job writes down the pid and folder of the worker it runs under.
    let script = format!(
        "{}; printf '%s\\n' \"$w\" > \"{}\"; if [ -e /proc/$w/cwd ]; then d=$(readlink /proc/$w/cwd); else d=$(lsof -a -p $w -d cwd -Fn | sed -n 's/^n//p'); fi; printf '%s\\n' \"$d\" > \"{}\"",
        common::FIND_AGENT_WORKER_SH,
        worker_pid.display(),
        record.display()
    );
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: TEST\nagents:\n  probe:\n    runner: command\n    command: sh\n    args: ['-c', {}]\n",
            serde_json::to_string(&script).unwrap()
        ),
    )
    .unwrap();
    AutomationService::set(
        &fixtures.tasks_root,
        None,
        "automation:\n  rules:\n    - name: Probe new tickets\n      on:\n        created:\n          set:\n            assignee: \"@probe\"\n",
    )
    .expect("set automation");

    let out = Command::new(crate::common::lotar_bin())
        .current_dir(&launcher)
        .arg("--tasks-dir")
        .arg(&fixtures.tasks_root)
        .args(["add", "probe"])
        .env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_AGENT_QUEUE_DISABLE_WORKER")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", root.join("queue"))
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_TEST_SILENT", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));

    let worker_cwd = wait_for(&record, Duration::from_secs(30));
    let expected = fixtures.tasks_root.parent().unwrap().canonicalize().unwrap();
    assert_eq!(Path::new(&worker_cwd).canonicalize().unwrap(), expected, "the worker kept the folder of the command that started it");

    // The worker still finishes the job after the record lands; let it exit
    // before the fixture removes its folder (DEV-840).
    common::wait_for_worker_exit(&wait_for(&worker_pid, Duration::from_secs(1)), Duration::from_secs(30));
}
