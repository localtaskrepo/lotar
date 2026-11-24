//! DEV-604: a job that fails on commit signing or a full disk pauses the
//! queue instead of failing. The jobs the pause holds wait, one tries again
//! on a schedule, and the first that completes resumes the queue. A paused
//! commit is retried alone: the agent doesn't run again.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

struct Workspace {
    _fixtures: TestFixtures,
    root: PathBuf,
    /// One line per agent run: the ticket it ran on.
    runs: PathBuf,
    /// While it exists, the fake signer or the fake disk fails.
    broken: PathBuf,
    /// `LOTAR_AGENT_PAUSE_RETRY_SECONDS`.
    retry_seconds: &'static str,
}

impl Drop for Workspace {
    /// Let the worker finish before the fixture goes.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.broken);
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            let out = lotar(self).args(["--format", "json", "agent", "queue"]).output().ok();
            let json: serde_json::Value = out.and_then(|out| serde_json::from_slice(&out.stdout).ok()).unwrap_or_default();
            if json["worker_state"] != "active" && json["running_jobs"] == 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn git(root: &Path, args: &[&str]) -> String {
    let out =
        Command::new("git").args(args).current_dir(root).env("GIT_CONFIG_GLOBAL", root.join("gitconfig")).env("GIT_CONFIG_NOSYSTEM", "1").output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Two tickets and agent `writer`, whose `runner.sh` body is `runner`.
/// `worktree`: jobs run in agent worktrees of a git repository whose
/// commits a fake signer signs, failing while `broken` exists.
fn workspace(runner: &str, worktree: bool) -> Workspace {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let runs = root.join("runs");
    let broken = root.join("broken");
    std::fs::write(root.join("gitconfig"), "").unwrap();
    script(&root.join("runner.sh"), &format!("echo \"$LOTAR_TICKET_ID\" >> '{}'\n{runner}", runs.display()));
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: TEST\nagent:\n  worktree:\n    enabled: {worktree}\n    dir: '{}'\nagents:\n  writer:\n    runner: command\n    command: '{}'\n",
            root.join("worktrees").display(),
            root.join("runner.sh").display(),
        ),
    )
    .unwrap();
    let ws = Workspace { _fixtures: fixtures, root: root.clone(), runs, broken, retry_seconds: "2" };
    for title in ["first", "second"] {
        assert!(lotar(&ws).args(["add", title]).status().unwrap().success());
    }
    if worktree {
        // gpg's protocol as git reads it: a SIG_CREATED status line on the
        // status fd (2) and the signature on stdout.
        script(
            &root.join("signer.sh"),
            &format!(
                "cat >/dev/null\nif [ -f '{}' ]; then echo \"error: Couldn't find key in agent\" >&2; exit 2; fi\nprintf '\\n[GNUPG:] SIG_CREATED D 1 8 00 0 0\\n' >&2\nprintf -- '-----BEGIN PGP SIGNATURE-----\\nfake\\n-----END PGP SIGNATURE-----\\n'",
                ws.broken.display()
            ),
        );
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.name", "Pause Tests"]);
        git(&root, &["config", "user.email", "pause@example.invalid"]);
        std::fs::write(root.join(".gitignore"), "runs\nbroken\nworktrees/\ngitconfig\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "Test fixture"]);
        git(&root, &["config", "commit.gpgsign", "true"]);
        git(&root, &["config", "gpg.format", "openpgp"]);
        git(&root, &["config", "gpg.program", root.join("signer.sh").to_str().unwrap()]);
    }
    ws
}

fn lotar(ws: &Workspace) -> Command {
    let mut cmd = Command::new(crate::common::lotar_bin());
    cmd.current_dir(&ws.root)
        .env_remove("RUST_TEST_THREADS")
        .env_remove("LOTAR_AGENT_QUEUE_DISABLE_WORKER")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_AGENT_QUEUE_DIR", ws.root.join("queue"))
        .env("LOTAR_AGENT_PAUSE_RETRY_SECONDS", ws.retry_seconds)
        .env("GIT_CONFIG_GLOBAL", ws.root.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_TEST_SILENT", "1");
    cmd
}

fn json(ws: &Workspace, args: &[&str]) -> serde_json::Value {
    let out = lotar(ws).arg("--format").arg("json").args(args).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

fn run(ws: &Workspace, ticket: &str) {
    let out = lotar(ws).args(["agent", "run", ticket, "go", "--agent", "writer"]).output().unwrap();
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

fn wait_for(ws: &Workspace, what: &str, done: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let queue = json(ws, &["agent", "queue"]);
        if done(&queue) {
            return queue;
        }
        assert!(Instant::now() < deadline, "{what}: {queue}\n{}", job_logs(ws));
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Every job's log, for a failure message: what a job waited on (DEV-742).
fn job_logs(ws: &Workspace) -> String {
    let list = json(ws, &["agent", "list-jobs"]);
    let mut text = String::new();
    for job in list["jobs"].as_array().into_iter().flatten() {
        let id = job["id"].as_str().unwrap_or_default();
        let out = lotar(ws).args(["agent", "logs", id]).output().unwrap();
        text += &format!("--- {job}\n{}{}\n", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }
    text
}

fn runs(ws: &Workspace) -> Vec<String> {
    std::fs::read_to_string(&ws.runs).unwrap_or_default().lines().map(str::to_string).collect()
}

fn queued(queue: &serde_json::Value, ticket: &str) -> bool {
    queue["pending"].as_array().is_some_and(|pending| pending.iter().any(|entry| entry["ticket_id"] == ticket))
}

fn jobs(ws: &Workspace, ticket: &str, status: &str) -> Vec<serde_json::Value> {
    let list = json(ws, &["agent", "list-jobs"]);
    list["jobs"].as_array().unwrap().iter().filter(|job| job["ticket_id"] == ticket && job["status"] == status).cloned().collect()
}

#[test]
fn a_signing_failure_pauses_the_queue_and_its_retry_makes_only_the_commit() {
    common::require_git();
    // The agent also hands its ticket on, as a job's outcome (DEV-320).
    let mut ws =
        workspace("echo work > work.txt\n\"$LOTAR_BIN\" status \"$LOTAR_TICKET_ID\" InProgress\n\"$LOTAR_BIN\" assignee \"$LOTAR_TICKET_ID\" mallox", true);
    // Each try is due as soon as the one before ended: a try that starts
    // while another runs shows without load (DEV-742).
    ws.retry_seconds = "0";
    std::fs::write(&ws.broken, "").unwrap();

    run(&ws, "TEST-1");
    let queue = wait_for(&ws, "no signing pause", |queue| queue["pauses"][0]["reason"] == "signing");
    assert!(queue["pauses"][0]["message"].as_str().is_some_and(|line| line.contains("Couldn't find key in agent")), "{queue}");
    assert_eq!(queue["pauses"][0]["ticket_id"], "TEST-1", "{queue}");
    // The queue pauses just before the job's log gets its end.
    wait_for(&ws, "the job doesn't read paused", |_| !jobs(&ws, "TEST-1", "paused").is_empty());
    assert!(jobs(&ws, "TEST-1", "failed").is_empty(), "no job failed");
    let text = lotar(&ws).args(["agent", "list-jobs"]).output().unwrap();
    let text = String::from_utf8_lossy(&text.stdout).to_string() + &String::from_utf8_lossy(&text.stderr);
    assert!(text.contains("commit signing fails"), "list-jobs shows the pause: {text}");

    // A job that commits waits while the signer fails; the retry keeps
    // trying the commit, without running the agent again.
    run(&ws, "TEST-2");
    std::thread::sleep(Duration::from_secs(5));
    assert_eq!(runs(&ws), ["TEST-1"], "the held job didn't run, the paused one didn't run again");
    let queue = json(&ws, &["agent", "queue"]);
    assert!(queued(&queue, "TEST-2"), "{queue}\n{}", job_logs(&ws));
    assert_eq!(queue["pauses"][0]["reason"], "signing", "{queue}");
    let show = json(&ws, &["show", "TEST-1"]);
    assert_eq!(show["status"], "Todo", "the ticket keeps its status: {show}");
    assert!(show["assignee"].is_null(), "and its assignee during the pause: {show}");

    std::fs::remove_file(&ws.broken).unwrap();
    wait_for(&ws, "the queue didn't resume", |queue| {
        queue["pauses"].as_array().is_some_and(Vec::is_empty) && !queued(queue, "TEST-2") && queue["running_jobs"] == 0
    });
    wait_for(&ws, "TEST-2 didn't end", |_| !jobs(&ws, "TEST-2", "completed").is_empty());
    assert_eq!(runs(&ws), ["TEST-1", "TEST-2"], "TEST-1's agent ran once");
    let log = git(&ws.root, &["log", "--format=%s", "agent/TEST-1"]);
    assert!(log.lines().next().is_some_and(|subject| subject.starts_with("TEST-1")), "the retry committed TEST-1's work: {log}");
    assert_eq!(git(&ws.root, &["show", "agent/TEST-1:work.txt"]), "work");
    assert!(!jobs(&ws, "TEST-1", "completed").is_empty(), "the retry completed");
    let show = json(&ws, &["show", "TEST-1"]);
    assert_eq!((&show["status"], &show["assignee"]), (&"InProgress".into(), &"mallox".into()), "the retry applied the agent's change: {show}");
}

#[test]
fn a_full_disk_pauses_every_job_until_one_completes() {
    common::require_git();
    let ws = workspace("", false);
    script(
        &ws.root.join("runner.sh"),
        &format!(
            "echo \"$LOTAR_TICKET_ID\" >> '{}'\nif [ -f '{}' ]; then \"$LOTAR_BIN\" status \"$LOTAR_TICKET_ID\" InProgress; \"$LOTAR_BIN\" assignee \"$LOTAR_TICKET_ID\" mallox; echo 'write /tmp/x: No space left on device' >&2; exit 1; fi",
            ws.runs.display(),
            ws.broken.display()
        ),
    );
    // The paused job's end waits in a `job_paused` rule until the owner's
    // change is in: the queue reads paused while the job still ends, the
    // window full-suite load opened (DEV-752).
    let (held, release) = (ws.root.join("held"), ws.root.join("release"));
    std::fs::write(
        ws._fixtures.tasks_root.join("automation.yml"),
        format!(
            "automation:\n  rules:\n    - name: Slow pause\n      on:\n        job_paused:\n          run:\n            command: sh\n            args: ['-c', 'touch \"{}\"; i=0; while [ ! -f \"{}\" ] && [ $i -lt 300 ]; do sleep 0.1; i=$((i+1)); done']\n            wait: true\n",
            held.display(),
            release.display()
        ),
    )
    .unwrap();
    std::fs::write(&ws.broken, "").unwrap();

    run(&ws, "TEST-1");
    let queue = wait_for(&ws, "no disk pause", |queue| queue["pauses"][0]["reason"] == "disk");
    assert!(queue["pauses"][0]["message"].as_str().is_some_and(|line| line.contains("No space left on device")), "{queue}");
    // Jobs without a worktree commit nothing, yet a full disk holds them.
    run(&ws, "TEST-2");
    std::thread::sleep(Duration::from_secs(1));
    assert!(!runs(&ws).contains(&"TEST-2".to_string()), "the held job didn't run: {:?}", runs(&ws));
    assert!(jobs(&ws, "TEST-1", "failed").is_empty(), "no job failed");
    // The owner reassigns the ticket during the pause: that stays.
    wait_for(&ws, "the job_paused rule didn't run", |_| held.exists());
    assert!(lotar(&ws).args(["assignee", "TEST-1", "sam"]).status().unwrap().success());
    std::fs::write(&release, "").unwrap();

    std::fs::remove_file(&ws.broken).unwrap();
    wait_for(&ws, "the queue didn't resume", |queue| {
        queue["pauses"].as_array().is_some_and(Vec::is_empty) && !queued(queue, "TEST-2") && queue["running_jobs"] == 0
    });
    wait_for(&ws, "TEST-2 didn't end", |_| !jobs(&ws, "TEST-2", "completed").is_empty());
    assert!(!jobs(&ws, "TEST-1", "completed").is_empty(), "the paused job ran again and completed");
    let show = json(&ws, &["show", "TEST-1"]);
    assert_eq!(
        (&show["status"], &show["assignee"]),
        (&"InProgress".into(), &"sam".into()),
        "the paused job's status applies, its assignee gave way to the owner's: {show}"
    );
}

/// `agent run --wait` returns once its job pauses, and fails: the job didn't complete.
#[test]
fn waiting_for_a_job_ends_when_it_pauses() {
    common::require_git();
    let ws = workspace("echo 'No space left on device' >&2\nexit 1", false);
    let started = Instant::now();
    let out = lotar(&ws).args(["agent", "run", "TEST-1", "go", "--agent", "writer", "--wait", "--timeout-seconds", "60"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("ended with status paused"), "{text}");
    assert!(started.elapsed() < Duration::from_secs(30), "it returned when the job paused, not at the timeout: {text}");
    // Let the retry complete, so the worker ends.
    script(&ws.root.join("runner.sh"), "exit 0");
    wait_for(&ws, "the queue didn't resume", |queue| queue["pauses"].as_array().is_some_and(Vec::is_empty) && queue["running_jobs"] == 0);
}

/// A job held in `lotar serve` takes the pause's due try when no queue
/// entry in its scope waits for it: here the paused job's retry left the
/// queue, so nothing else would ever end the pause.
#[test]
fn a_job_held_in_serve_takes_the_due_try_when_no_queue_entry_waits() {
    common::require_git();
    let ws = workspace("", false);
    script(
        &ws.root.join("runner.sh"),
        &format!(
            "echo \"$LOTAR_TICKET_ID\" >> '{}'\nif [ -f '{}' ]; then echo 'write /tmp/x: No space left on device' >&2; exit 1; fi",
            ws.runs.display(),
            ws.broken.display()
        ),
    );
    std::fs::write(&ws.broken, "").unwrap();
    // This process and the worker share the queue file, which is keyed by
    // the tasks directory's path as the CLI resolves it.
    let tasks_dir = std::fs::canonicalize(&ws._fixtures.tasks_root).unwrap();
    let queue_dir = std::env::var("LOTAR_AGENT_QUEUE_DIR").unwrap();
    let cli = |args: &[&str]| {
        let out = lotar(&ws).env("LOTAR_AGENT_QUEUE_DIR", &queue_dir).args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap_or_default()
    };

    cli(&["agent", "run", "TEST-1", "go", "--agent", "writer"]);
    let deadline = Instant::now() + Duration::from_secs(30);
    while cli(&["--format", "json", "agent", "queue"])["pauses"][0]["reason"] != "disk" {
        assert!(Instant::now() < deadline, "no disk pause");
        std::thread::sleep(Duration::from_millis(50));
    }
    // The retry leaves the queue before its try; the pause stays.
    cli(&["agent", "queue", "flush"]);
    std::fs::remove_file(&ws.broken).unwrap();

    let request =
        lotar::api_types::AgentJobCreateRequest { ticket_id: "TEST-2".into(), prompt: "go".into(), agent: Some("writer".into()), ..Default::default() };
    let job = lotar::services::agent_job_service::AgentJobService::start_job_with_tasks_dir(request, &tasks_dir).unwrap().id;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = lotar::services::agent_job_service::AgentJobService::get_job(&job).unwrap().status;
        if status == "completed" {
            break;
        }
        assert!(Instant::now() < deadline, "the job held in serve never started: {status}, runs {:?}", runs(&ws));
        std::thread::sleep(Duration::from_millis(100));
    }
    let queue = cli(&["--format", "json", "agent", "queue"]);
    assert_eq!(queue["pauses"], serde_json::json!([]), "its completion ended the pause: {queue}");
}
