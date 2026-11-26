//! DEV-605: a merge job of a repository with `agent.merge` merges without an
//! agent when the branch rebases cleanly and the checks pass, and starts the
//! merge agent, with a hand-off in its prompt, only for what needs judgment.
#![cfg(unix)]

use crate::common::TestFixtures;
use lotar::api_types::{AgentJobCreateRequest, TaskCreate, TaskUpdate};
use lotar::services::agent_job_service::{AgentJobService, AgentOrchestratorMode};
use lotar::services::task_service::TaskService;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const TICKET: &str = "MRG-1";
const BRANCH: &str = "agent/MRG-1";

/// A repository whose main checkout holds `.tasks`, with a bare `origin`,
/// the ticket's branch checked out in a fix worktree, and a merge (and fix)
/// agent that writes its prompt to `prompt` when it runs.
struct Repo {
    fixtures: TestFixtures,
    /// Holds the worktrees, the remote and the agent outside the checkout.
    outside: tempfile::TempDir,
    main: String,
}

impl Repo {
    fn new(merge_config: &str) -> Self {
        crate::common::require_git();
        AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
        let fixtures = TestFixtures::new();
        let outside = tempfile::tempdir().expect("tempdir");
        let root = fixtures.get_temp_path().to_path_buf();
        git(&root, &["init", "-q"]);
        for (key, value) in [("user.name", "Agent Tests"), ("user.email", "agent-tests@example.com"), ("commit.gpgsign", "false")] {
            git(&root, &["config", key, value]);
        }
        fs::write(root.join("README.md"), "test repo\n").expect("write");
        git(&root, &["add", "README.md"]);
        git(&root, &["commit", "-qm", "Initial commit"]);
        let main = git(&root, &["symbolic-ref", "--short", "HEAD"]);
        let remote = outside.path().join("origin.git");
        git(outside.path(), &["init", "-q", "--bare", &remote.to_string_lossy()]);
        git(&root, &["remote", "add", "origin", &remote.to_string_lossy()]);
        git(&root, &["push", "-q", "-u", "origin", &main]);

        let agent = outside.path().join("merge-agent.sh");
        fs::write(&agent, format!("#!/bin/sh\ncat \"$LOTAR_AGENT_PROMPT_FILE\" > \"{}\"\n", outside.path().join("prompt").display())).expect("write agent");
        fs::set_permissions(&agent, fs::Permissions::from_mode(0o755)).expect("chmod");
        fs::write(
            fixtures.tasks_root.join("config.yml"),
            format!(
                "default:\n  project: MRG\nissue:\n  states: [Todo, Merging, Done]\n  priorities: [Low]\n  types: [Feature]\nagent:\n  worktree:\n    enabled: true\n    dir: '{}'\n{merge_config}agents:\n  merge:\n    runner: command\n    command: '{agent}'\n  fix:\n    runner: command\n    command: '{agent}'\n",
                outside.path().join("worktrees").display(),
                agent = agent.display()
            ),
        )
        .expect("write config");
        let mut storage = fixtures.create_storage();
        let task = TaskService::create(
            &mut storage,
            TaskCreate { title: "Merge me".to_string(), project: Some("MRG".to_string()), reporter: Some("Agent Tests".to_string()), ..Default::default() },
        )
        .expect("create task");
        assert_eq!(task.id, TICKET);
        let repo = Repo { fixtures, outside, main };
        git(&repo.root(), &["branch", BRANCH]);
        git(&repo.root(), &["worktree", "add", "-q", &repo.fix_worktree().to_string_lossy(), BRANCH]);
        repo
    }

    fn root(&self) -> PathBuf {
        self.fixtures.get_temp_path().to_path_buf()
    }

    fn fix_worktree(&self) -> PathBuf {
        self.outside.path().join("fix")
    }

    fn remote(&self) -> PathBuf {
        self.outside.path().join("origin.git")
    }

    /// The fix's commit on the ticket's branch.
    fn commit_fix(&self, file: &str, content: &str) {
        fs::write(self.fix_worktree().join(file), content).expect("write fix");
        git(&self.fix_worktree(), &["add", file]);
        git(&self.fix_worktree(), &["commit", "-qm", "MRG-1 Merge me"]);
    }

    /// Main moves on while the ticket waits for its merge.
    fn commit_main(&self, file: &str, content: &str) {
        fs::write(self.root().join(file), content).expect("write main");
        git(&self.root(), &["add", file]);
        git(&self.root(), &["commit", "-qm", &format!("other work on {file}")]);
    }

    /// Run the ticket's merge job to its end; its status and the merge
    /// agent's prompt, when the agent ran.
    fn merge(&self) -> (String, Option<String>) {
        let (_, status, prompt) = self.merge_job();
        (status, prompt)
    }

    /// [`Self::merge`], with the job's progress lines first.
    fn merge_job(&self) -> (Vec<String>, String, Option<String>) {
        self.job_of("merge")
    }

    /// Run a job of `profile` on the ticket to its end; its progress lines,
    /// status and the agent's prompt, when the agent ran.
    fn job_of(&self, profile: &str) -> (Vec<String>, String, Option<String>) {
        let job = AgentJobService::start_job_with_tasks_dir(
            AgentJobCreateRequest {
                ticket_id: TICKET.to_string(),
                prompt: "Merge the ticket".to_string(),
                agent: Some(profile.to_string()),
                ..Default::default()
            },
            &self.fixtures.tasks_root,
        )
        .expect("start merge job");
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let status = AgentJobService::get_job(&job.id).map(|job| job.status).unwrap_or_default();
            if matches!(status.as_str(), "completed" | "failed" | "cancelled") {
                // The end rules and cleanup run after the status is set.
                std::thread::sleep(Duration::from_millis(200));
                let progress = AgentJobService::events_for(&job.id)
                    .into_iter()
                    .filter(|event| event.kind == "agent_job_progress")
                    .filter_map(|event| event.message)
                    .collect();
                return (progress, status, fs::read_to_string(self.outside.path().join("prompt")).ok());
            }
            assert!(Instant::now() < deadline, "the merge job didn't end: {status}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn comments(&self) -> Vec<lotar::types::TaskComment> {
        TaskService::get(&self.fixtures.create_storage(), TICKET, None).expect("ticket").comments
    }

    fn on_main(&self, file: &str) -> bool {
        Command::new("git").args(["cat-file", "-e", &format!("HEAD:{file}")]).current_dir(self.root()).status().expect("git").success()
    }

    fn remote_head(&self) -> String {
        git(&self.remote(), &["rev-parse", &self.main])
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git").args(args).current_dir(dir).env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").output().expect("git");
    assert!(output.status.success(), "git {args:?} in {}: {}", dir.display(), String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

const CHECKS: &str = "  merge:\n    checks: ['test -f feature.txt']\n    push: true\n";

#[test]
fn a_clean_branch_lands_and_pushes_without_an_agent() {
    let repo = Repo::new(CHECKS);
    repo.commit_fix("feature.txt", "feature\n");
    repo.commit_main("other.txt", "other\n");
    git(&repo.root(), &["push", "-q"]);

    let (status, prompt) = repo.merge();

    assert_eq!(status, "completed");
    assert_eq!(prompt, None, "no agent started");
    assert!(repo.on_main("feature.txt") && repo.on_main("other.txt"), "the fix is rebased onto main and landed");
    assert_eq!(repo.remote_head(), git(&repo.root(), &["rev-parse", "HEAD"]), "main is pushed");
    assert_eq!(git(&repo.root(), &["status", "--porcelain", "--", "README.md", "feature.txt"]), "");
    let comments = repo.comments();
    let merged = comments.iter().find(|comment| comment.text.starts_with("Merged without an agent")).expect("the outcome comment");
    let report = merged.report.as_ref().expect("a report");
    assert_eq!(report.checks.iter().map(|check| (check.command.as_str(), check.result.as_str())).collect::<Vec<_>>(), vec![("test -f feature.txt", "passed")]);
    assert!(report.commits.iter().any(|commit| commit.what.as_deref() == Some("MRG-1 Merge me")), "{report:?}");
    assert!(merged.text.contains("Pushed"), "{}", merged.text);
}

/// DEV-784: the checks run under the machine's check lock and tell the
/// repository's scripts in them that it is held, so they don't wait for it.
#[test]
fn the_checks_tell_their_scripts_that_the_check_lock_is_held() {
    let repo = Repo::new("  merge:\n    checks: ['test \"$LOTAR_CHECK_LOCK_HELD\" = 1']\n");
    repo.commit_fix("feature.txt", "feature\n");

    let (status, prompt) = repo.merge();

    assert_eq!(status, "completed");
    assert_eq!(prompt, None, "the check passed, so no agent started");
    assert!(repo.on_main("feature.txt"));
}

#[test]
fn a_check_that_passed_on_the_unchanged_commit_is_skipped() {
    let repo = Repo::new("  merge:\n    checks: ['exit 1']\n");
    repo.commit_fix("feature.txt", "feature\n");
    let head = git(&repo.fix_worktree(), &["rev-parse", "HEAD"]);
    let mut storage = repo.fixtures.create_storage();
    let report = lotar::types::CommentReport {
        commits: vec![lotar::types::ReportCommit { hash: head[..8].to_string(), ..Default::default() }],
        checks: vec![lotar::types::ReportCheck { command: "exit 1".to_string(), result: "passed".to_string(), ..Default::default() }],
        ..Default::default()
    };
    let context = lotar::services::task_service::CommentContext { report: Some(report), ..Default::default() };
    TaskService::add_comment_with_context(&mut storage, TICKET, "Fixed", context).expect("fix comment");

    let (status, prompt) = repo.merge();

    assert_eq!(status, "completed");
    assert_eq!(prompt, None);
    let comments = repo.comments();
    let check = &comments.last().and_then(|comment| comment.report.as_ref()).expect("report").checks[0];
    assert_eq!(check.result, "not_run");
    assert!(check.reason.as_deref().is_some_and(|reason| reason.starts_with("skipped")), "{check:?}");
}

#[test]
fn a_branch_landed_already_has_nothing_to_merge() {
    let repo = Repo::new(CHECKS);
    repo.commit_fix("feature.txt", "feature\n");
    git(&repo.root(), &["merge", "-q", "--ff-only", BRANCH]);

    let (status, prompt) = repo.merge();

    assert_eq!(status, "completed");
    assert_eq!(prompt, None);
    assert!(repo.comments().iter().any(|comment| comment.text.starts_with("Nothing to merge")));
}

/// DEV-680: the fix job committed nothing anywhere (DEV-657), so main holds
/// no commit named after the ticket. The merge fails, which sends the
/// ticket back to the fix step instead of Done.
#[test]
fn a_ticket_nothing_was_built_for_is_not_done() {
    let repo = Repo::new(CHECKS);

    let (status, prompt) = repo.merge();

    assert_eq!(status, "failed");
    assert_eq!(prompt, None, "no agent started");
    let summary = AgentJobService::list_jobs_in(&repo.fixtures.tasks_root).into_iter().find_map(|job| job.summary).unwrap_or_default();
    assert!(summary.contains("nothing was built"), "{summary}");
}

/// DEV-886: the merge failed so, and its `job_failed` retry assigned `@fix`
/// while the ticket stays in `Merging`. The fix job runs its agent instead
/// of lotar's merge, and doesn't fail with the merge's verdict.
#[test]
fn a_fix_job_on_a_merging_ticket_runs_its_agent() {
    let repo = Repo::new(CHECKS);
    let mut storage = repo.fixtures.create_storage();
    TaskService::update_with_context(
        &mut storage,
        TICKET,
        TaskUpdate { status: Some("Merging".to_string()), ..Default::default() },
        lotar::services::task_service::TaskUpdateContext::automation_disabled(),
    )
    .expect("set Merging");

    let (progress, status, prompt) = repo.job_of("fix");

    assert!(prompt.is_some(), "the fix agent ran");
    assert!(!progress.iter().any(|line| line.starts_with("Nothing to merge")), "{progress:?}");
    assert_eq!(status, "completed");
}

/// DEV-896: DEV-832's fix job, on a ticket a failed merge left in `Merging`,
/// ran as a merge job and landed the branch, and the fix's end rule then sent
/// work already on main to review. Now the fix job leaves the branch for the
/// merge, and the merge job that lands it ends the ticket Done, also when it
/// runs while the ticket is in another status.
#[test]
fn only_the_merge_job_lands_a_merging_tickets_branch_and_ends_it_done() {
    // No checks: they would wait for the machine's check lock.
    let repo = Repo::new("  merge:\n    checks: []\n");
    let config_path = repo.fixtures.tasks_root.join("config.yml");
    let config = fs::read_to_string(&config_path).expect("read config").replace("[Todo, Merging, Done]", "[Todo, InProgress, Reviewing, Merging, Done]");
    fs::write(&config_path, config).expect("write config");
    fs::write(
        repo.fixtures.tasks_root.join("automation.yml"),
        "automation:\n  rules:\n    - name: fix lifecycle\n      when: { assignee: '@fix' }\n      on:\n        job_started: { set: { status: InProgress } }\n        job_completed: { set: { status: Reviewing } }\n    - name: merge lifecycle\n      when: { assignee: '@merge' }\n      on:\n        job_completed: { set: { status: Done } }\n",
    )
    .expect("write rules");
    let assign = |assignee: &str, status: Option<&str>| {
        let update = TaskUpdate { status: status.map(str::to_string), assignee: Some(assignee.to_string()), ..Default::default() };
        TaskService::update_with_context(
            &mut repo.fixtures.create_storage(),
            TICKET,
            update,
            lotar::services::task_service::TaskUpdateContext::automation_disabled(),
        )
        .expect("assign");
    };
    let ticket = || TaskService::get(&repo.fixtures.create_storage(), TICKET, None).expect("ticket");
    repo.commit_fix("feature.txt", "feature\n");

    assign("@fix", Some("Merging"));
    let (_, status, prompt) = repo.job_of("fix");

    assert_eq!(status, "completed");
    assert!(prompt.is_some(), "the fix agent ran");
    assert!(!repo.on_main("feature.txt"), "the fix job left the branch for the merge");
    assert_eq!(ticket().status.as_str(), "Reviewing");

    assign("@merge", None);
    let (status, _) = repo.merge();

    assert_eq!(status, "completed");
    assert!(repo.on_main("feature.txt"));
    assert_eq!(ticket().status.as_str(), "Done", "the merge's end rule moved it on from Reviewing");
}

/// DEV-685: the same ticket, merged by an agent that exits 1. Its empty
/// branch is on main already, but that doesn't complete the job.
#[test]
fn a_failed_merge_agent_of_a_ticket_nothing_was_built_for_fails() {
    let repo = Repo::new("");
    let agent = repo.outside.path().join("merge-agent.sh");
    let script = fs::read_to_string(&agent).expect("read agent");
    fs::write(&agent, format!("{script}exit 1\n")).expect("write agent");

    let (status, prompt) = repo.merge();

    assert!(prompt.is_some(), "the merge agent ran");
    assert_eq!(status, "failed");
    let summary = AgentJobService::list_jobs_in(&repo.fixtures.tasks_root).into_iter().find_map(|job| job.summary).unwrap_or_default();
    assert!(summary.contains("nothing was built"), "{summary}");
}

/// DEV-677: lotar's branch is empty, the ticket's commits are on the same
/// branch of another repository its comment names. That repository's main
/// checkout.
fn work_in_another_repository(repo: &Repo) -> PathBuf {
    let other = repo.outside.path().join("other");
    fs::create_dir(&other).expect("mkdir");
    git(&other, &["init", "-q"]);
    for (key, value) in [("user.name", "Agent Tests"), ("user.email", "agent-tests@example.com"), ("commit.gpgsign", "false")] {
        git(&other, &["config", key, value]);
    }
    git(&other, &["commit", "-q", "--allow-empty", "-m", "Initial commit"]);
    git(&other, &["branch", BRANCH]);
    let worktree = repo.outside.path().join("other-fix");
    git(&other, &["worktree", "add", "-q", &worktree.to_string_lossy(), BRANCH]);
    git(&worktree, &["commit", "-q", "--allow-empty", "-m", "MRG-1 the work"]);
    let mut storage = repo.fixtures.create_storage();
    let text = format!("Reopened: the work is branch `{BRANCH}` in `{}`.", worktree.display());
    TaskService::add_comment_with_context(&mut storage, TICKET, &text, Default::default()).expect("comment");
    other
}

#[test]
fn a_ticket_whose_work_is_in_another_repository_is_not_done() {
    let repo = Repo::new(CHECKS);
    work_in_another_repository(&repo);

    let (status, prompt) = repo.merge();

    assert_eq!(status, "failed", "the merge agent left the other repository's branch unmerged");
    let prompt = prompt.expect("the merge agent ran");
    assert!(prompt.contains("the ticket's work is in another repository") && prompt.contains("holds 1 commit beyond"), "{prompt}");
    assert!(!repo.comments().iter().any(|comment| comment.text.starts_with("Nothing to merge")));
}

#[test]
fn a_merge_agent_that_lands_the_other_repository_completes() {
    let repo = Repo::new(CHECKS);
    let other = work_in_another_repository(&repo);
    let agent = repo.outside.path().join("merge-agent.sh");
    let script = fs::read_to_string(&agent).expect("read agent");
    fs::write(&agent, format!("{script}cd '{}' && git merge -q --ff-only {BRANCH}\n", other.display())).expect("write agent");

    let (status, prompt) = repo.merge();

    assert!(prompt.is_some(), "the merge agent ran");
    assert_eq!(status, "completed", "the other repository's main holds the work");
}

#[test]
fn a_failed_merge_agent_leaves_work_in_another_repository_unfinished() {
    let repo = Repo::new(CHECKS);
    work_in_another_repository(&repo);
    let agent = repo.outside.path().join("merge-agent.sh");
    let script = fs::read_to_string(&agent).expect("read agent");
    fs::write(&agent, format!("{script}exit 7\n")).expect("write agent");

    let (status, prompt) = repo.merge();

    assert!(prompt.is_some(), "the merge agent ran");
    assert_eq!(status, "failed", "lotar's branch on main doesn't complete the job: the work is in the other repository");
}

/// DEV-679: lotar's branch has commits too. The merge agent `lands` the
/// other repository's branch or not.
fn work_in_both_repositories(merge_config: &str, lands: bool) -> (Repo, String, Option<String>) {
    let repo = Repo::new(merge_config);
    repo.commit_fix("feature.txt", "feature\n");
    let other = work_in_another_repository(&repo);
    if lands {
        let agent = repo.outside.path().join("merge-agent.sh");
        let script = fs::read_to_string(&agent).expect("read agent");
        fs::write(&agent, format!("{script}cd '{}' && git merge -q --ff-only {BRANCH}\n", other.display())).expect("write agent");
    }
    let (status, prompt) = repo.merge();
    (repo, status, prompt)
}

#[test]
fn lotars_merge_hands_off_before_landing_when_the_work_is_in_both_repositories() {
    let (repo, status, prompt) = work_in_both_repositories(CHECKS, false);

    let prompt = prompt.expect("the merge agent ran");
    assert!(prompt.contains("isn't merged yet, but the ticket's work is in another repository") && prompt.contains("holds 1 commit beyond"), "{prompt}");
    assert!(!repo.on_main("feature.txt"), "lotar's branch waits for the other repository");
    assert_eq!(status, "failed", "the merge agent left the other repository's branch unmerged");
}

#[test]
fn a_merge_agent_that_leaves_the_other_repository_unmerged_fails() {
    let (repo, status, prompt) = work_in_both_repositories("", false);

    assert!(prompt.is_some(), "the merge agent ran");
    assert_eq!(status, "failed", "the other repository's main lacks the work");
    assert!(!repo.on_main("feature.txt"), "lotar doesn't fast-forward its own branch meanwhile");
}

#[test]
fn a_merge_agent_that_lands_the_other_repository_lands_lotars_branch_too() {
    let (repo, status, prompt) = work_in_both_repositories("", true);

    assert!(prompt.is_some(), "the merge agent ran");
    assert_eq!(status, "completed");
    assert!(repo.on_main("feature.txt"), "lotar fast-forwards its own branch");
}

#[test]
fn a_conflict_hands_off_to_the_merge_agent() {
    let repo = Repo::new(CHECKS);
    repo.commit_fix("README.md", "the fix's readme\n");
    repo.commit_main("README.md", "main's readme\n");
    let branch_before = git(&repo.root(), &["rev-parse", BRANCH]);

    let (_, prompt) = repo.merge();

    let prompt = prompt.expect("the merge agent ran");
    assert!(prompt.contains("conflicts in `README.md`"), "{prompt}");
    assert!(prompt.contains("Merge the ticket"), "the rule's prompt follows the hand-off: {prompt}");
    assert_eq!(git(&repo.root(), &["rev-parse", BRANCH]), branch_before, "the aborted rebase leaves the branch as it was");
    assert!(repo.comments().iter().any(|comment| comment.text.starts_with("lotar's merge needs the merge agent")));
}

#[test]
fn a_failed_check_hands_off_with_its_output() {
    let repo = Repo::new("  merge:\n    checks: ['echo the check says boom; exit 3']\n");
    repo.commit_fix("feature.txt", "feature\n");

    let (_, prompt) = repo.merge();

    let prompt = prompt.expect("the merge agent ran");
    assert!(prompt.contains("the check `echo the check says boom; exit 3` failed") && prompt.contains("the check says boom"), "{prompt}");
    let comments = repo.comments();
    let hand_off = comments.iter().find(|comment| comment.text.starts_with("lotar's merge needs the merge agent")).expect("the hand-off comment");
    let report = hand_off.report.as_ref().expect("a report");
    let runs: Vec<_> = report.checks.iter().map(|check| (check.result.as_str(), check.reason.as_deref())).collect();
    assert_eq!(runs, vec![("failed", Some("first run")), ("failed", Some("rerun"))], "the check ran twice");
    assert!(prompt.contains("and again when rerun on its own"), "{prompt}");
    assert!(report.commits.is_empty(), "lotar landed nothing: {report:?}");
}

/// DEV-783: most merge-agent jobs came from a check that fails under load
/// and passes alone. lotar reruns a failed check once and lands when that
/// run passes.
#[test]
fn a_check_that_fails_once_then_passes_lands_without_an_agent() {
    let marker = tempfile::tempdir().expect("tempdir");
    let ran = marker.path().join("ran");
    let check = format!("if [ -e '{0}' ]; then echo second run; else touch '{0}'; echo flaky boom; exit 1; fi", ran.display());
    let repo = Repo::new(&format!("  merge:\n    checks: [\"{check}\", 'test -f feature.txt']\n"));
    repo.commit_fix("feature.txt", "feature\n");

    let (progress, status, prompt) = repo.merge_job();

    assert_eq!(status, "completed");
    assert_eq!(prompt, None, "no agent started");
    assert!(repo.on_main("feature.txt"), "the fix landed");
    assert!(
        progress.iter().any(|line| line.starts_with("Rerunning the check `if [ -e") && line.ends_with("once on its own: its first run failed")),
        "{progress:?}"
    );
    let comments = repo.comments();
    let merged = comments.iter().find(|comment| comment.text.starts_with("Merged without an agent")).expect("the outcome comment");
    assert!(merged.text.contains("failed, then passed when rerun on its own") && merged.text.contains("flaky boom"), "{}", merged.text);
    let report = merged.report.as_ref().expect("a report");
    let runs: Vec<_> = report.checks.iter().map(|check| (check.result.as_str(), check.reason.as_deref())).collect();
    assert_eq!(runs, vec![("failed", Some("first run; the rerun passed")), ("passed", Some("rerun")), ("passed", None)]);
}

#[test]
fn unpushed_commits_of_others_hand_off_after_landing() {
    let repo = Repo::new(CHECKS);
    repo.commit_fix("feature.txt", "feature\n");
    repo.commit_main("other.txt", "someone else's unfinished work\n");
    let pushed = repo.remote_head();

    let (_, prompt) = repo.merge();

    let prompt = prompt.expect("the merge agent ran");
    assert!(prompt.contains("unpushed commits that aren't this merge's") && prompt.contains("other work on other.txt"), "{prompt}");
    assert!(prompt.contains("don't stop at \"nothing to merge\""), "{prompt}");
    assert!(repo.on_main("feature.txt"), "the merge landed before the push");
    assert_eq!(repo.remote_head(), pushed, "nothing was pushed");
}

#[test]
fn a_refused_push_hands_off() {
    let repo = Repo::new(CHECKS);
    repo.commit_fix("feature.txt", "feature\n");
    let hook = repo.remote().join("hooks/pre-receive");
    fs::write(&hook, "#!/bin/sh\necho 'the remote says no' >&2\nexit 1\n").expect("write hook");
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).expect("chmod");

    let (_, prompt) = repo.merge();

    let prompt = prompt.expect("the merge agent ran");
    assert!(prompt.contains("`git push` was refused") && prompt.contains("the remote says no"), "{prompt}");
    assert!(repo.on_main("feature.txt"));
}

#[test]
fn without_agent_merge_the_merge_agent_merges() {
    let repo = Repo::new("");
    repo.commit_fix("feature.txt", "feature\n");

    let (_, prompt) = repo.merge();

    let prompt = prompt.expect("the merge agent ran");
    assert!(!prompt.contains("LoTaR started this merge itself"), "{prompt}");
}

#[test]
fn a_projects_agent_merge_replaces_the_repositorys() {
    let fixtures = TestFixtures::new();
    fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  project: MRG\nagent:\n  merge:\n    checks: [npm test]\n    install: npm ci\n    push: true\n",
    )
    .expect("write");
    fs::create_dir_all(fixtures.tasks_root.join("MRG")).expect("mkdir");
    fs::write(fixtures.tasks_root.join("MRG/config.yml"), "agent.merge:\n  checks: [cargo test]\n").expect("write");
    fs::create_dir_all(fixtures.tasks_root.join("OTH")).expect("mkdir");
    fs::write(fixtures.tasks_root.join("OTH/config.yml"), "project:\n  name: Other\n").expect("write");

    let project = lotar::config::resolution::config_for_project(&fixtures.tasks_root, Some("MRG")).expect("config");
    let merge = project.agent_merge.expect("agent.merge");
    assert_eq!((merge.checks, merge.install, merge.push), (vec!["cargo test".to_string()], None, false), "the project's section replaces it whole");
    let other = lotar::config::resolution::config_for_project(&fixtures.tasks_root, Some("OTH")).expect("config").agent_merge.expect("agent.merge");
    assert_eq!((other.checks, other.install.as_deref(), other.push), (vec!["npm test".to_string()], Some("npm ci"), true));

    let global = lotar::config::normalization::parse_global_from_yaml_str("agent:\n  merge:\n    checks: [npm test]\n").expect("parse");
    let written = lotar::config::normalization::to_canonical_global_yaml(&global);
    assert!(written.contains("merge:\n    checks:\n    - npm test"), "{written}");
}

#[test]
fn a_check_that_leaves_a_process_running_doesnt_hold_up_the_merge() {
    // The background process outlives `merge()`'s 60 s deadline: a merge that
    // waited for it fails there, however slow the machine. One that doesn't
    // stops it once the check's output has had its few seconds to drain.
    let ran = tempfile::tempdir().expect("tempdir");
    let pid_file = ran.path().join("pid");
    let repo = Repo::new(&format!("  merge:\n    checks: ['sleep 600 & echo $! > \"{}\"; exit 0']\n", pid_file.display()));
    repo.commit_fix("feature.txt", "feature\n");

    let (status, prompt) = repo.merge();

    let pid = fs::read_to_string(&pid_file).expect("the check ran").trim().to_string();
    let running = || Command::new("kill").args(["-0", &pid]).stderr(std::process::Stdio::null()).status().is_ok_and(|status| status.success());
    let deadline = Instant::now() + Duration::from_secs(5);
    while running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let left_running = running();
    if left_running {
        let _ = Command::new("kill").arg(&pid).status();
    }
    assert!(!left_running, "the merge left the background process running");
    assert_eq!(status, "completed");
    assert_eq!(prompt, None, "no agent started");
    assert!(repo.on_main("feature.txt"));
}

#[test]
fn an_untrusted_repository_leaves_the_merge_to_the_agent() {
    use crate::common::env_mutex::EnvVarGuard;
    let ran = tempfile::tempdir().expect("tempdir");
    let marker = ran.path().join("check-ran");
    let repo = Repo::new(&format!("  merge:\n    checks: ['touch \"{}\"']\n", marker.display()));
    repo.commit_fix("feature.txt", "feature\n");
    // The merge profile comes from the home config, so the untrusted
    // repository's own `agent.merge` is all that trust holds back.
    let config_path = repo.fixtures.tasks_root.join("config.yml");
    let config = fs::read_to_string(&config_path).expect("read config");
    let (repo_config, profiles) = config.split_once("agents:\n").expect("agents block");
    fs::write(&config_path, repo_config).expect("write config");
    let home = tempfile::tempdir().expect("tempdir");
    fs::write(home.path().join(".lotar"), format!("agents:\n{profiles}")).expect("write home config");
    let trust_file = home.path().join("trust.yml");
    let _env = [
        EnvVarGuard::clear("LOTAR_TRUST_ALL"),
        EnvVarGuard::set("LOTAR_TRUST_FILE", &trust_file.to_string_lossy()),
        EnvVarGuard::set("HOME", &home.path().to_string_lossy()),
        EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "0"),
        EnvVarGuard::clear("LOTAR_TEST_MODE"),
        EnvVarGuard::clear("RUST_TEST_THREADS"),
    ];
    assert!(!lotar::config::trust::TrustService::allows_commands(&repo.fixtures.tasks_root), "the repository is untrusted");

    let (_, prompt) = repo.merge();

    let prompt = prompt.expect("the merge agent ran");
    assert!(!prompt.contains("LoTaR started this merge itself"), "{prompt}");
    assert!(!marker.exists(), "the untrusted check didn't run");
}

/// DEV-863: the ticket's finding keys, as `lotar add --finding ... --bundle`
/// leaves them.
fn set_findings(repo: &Repo, keys: &[&str]) {
    let mut storage = repo.fixtures.create_storage();
    let fields =
        [(lotar::utils::custom_fields::FINDING_FIELD.to_string(), lotar::types::custom_value_from_json(&serde_json::json!(keys)))].into_iter().collect();
    TaskService::update(&mut storage, TICKET, lotar::api_types::TaskUpdate { custom_fields: Some(fields), ..Default::default() }).expect("set findings");
}

/// A merge config whose `finding_check` appends `$LOTAR_FINDING` to `ran`
/// and prints `listed`, as a health check prints what is still wrong.
fn finding_check_config(ran: &Path, listed: &str) -> String {
    format!("  merge:\n    checks: ['test -f feature.txt']\n    finding_check: 'echo \"$LOTAR_FINDING\" >> \"{}\"; echo \"{listed}\"'\n", ran.display())
}

fn finding_checks(report: &lotar::types::CommentReport) -> Vec<(&str, Option<&str>)> {
    report.checks.iter().filter(|check| check.command.starts_with("echo")).map(|check| (check.result.as_str(), check.reason.as_deref())).collect()
}

/// DEV-863: a bundle's finding check runs once per key; none still listed
/// lands without an agent.
#[test]
fn a_finding_check_that_lists_no_key_of_the_bundle_completes() {
    let marker = tempfile::tempdir().expect("tempdir");
    let ran = marker.path().join("ran");
    let repo = Repo::new(&finding_check_config(&ran, "[c:z] another finding"));
    set_findings(&repo, &["a:x", "b:y"]);
    repo.commit_fix("feature.txt", "feature\n");

    let (status, prompt) = repo.merge();

    assert_eq!(status, "completed");
    assert_eq!(prompt, None, "no agent started");
    assert_eq!(fs::read_to_string(&ran).expect("the finding check ran"), "a:x\nb:y\n", "once per key, in order");
    let comments = repo.comments();
    let merged = comments.iter().find(|comment| comment.text.starts_with("Merged without an agent")).expect("the outcome comment");
    assert!(merged.text.contains("no longer lists `[a:x]`") && merged.text.contains("no longer lists `[b:y]`"), "{}", merged.text);
    assert_eq!(finding_checks(merged.report.as_ref().expect("a report")), vec![("passed", None), ("passed", None)]);
}

/// DEV-863: the check still lists the bundle's second key: the merge, landed
/// already, hands off naming that key.
#[test]
fn a_finding_check_that_still_lists_the_second_key_hands_off_naming_it() {
    let marker = tempfile::tempdir().expect("tempdir");
    let ran = marker.path().join("ran");
    let repo = Repo::new(&finding_check_config(&ran, "[b:y] still broken"));
    set_findings(&repo, &["a:x", "b:y"]);
    repo.commit_fix("feature.txt", "feature\n");

    let (_, prompt) = repo.merge();

    let prompt = prompt.expect("the merge agent ran");
    assert!(prompt.contains("still lists `[b:y]`: [b:y] still broken"), "{prompt}");
    assert!(!prompt.contains("still lists `[a:x]`"), "{prompt}");
    assert!(repo.on_main("feature.txt"), "the check runs after landing");
    assert_eq!(fs::read_to_string(&ran).expect("the finding check ran"), "a:x\nb:y\n");
    let comments = repo.comments();
    let hand_off = comments.iter().find(|comment| comment.text.starts_with("lotar's merge needs the merge agent")).expect("the hand-off comment");
    assert_eq!(finding_checks(hand_off.report.as_ref().expect("a report")), vec![("passed", None), ("failed", Some("still lists [b:y]"))]);
}

/// DEV-863: a ticket without a finding key skips the finding check.
#[test]
fn a_ticket_without_a_finding_key_skips_the_finding_check() {
    let marker = tempfile::tempdir().expect("tempdir");
    let ran = marker.path().join("ran");
    let repo = Repo::new(&finding_check_config(&ran, "[a:x] still broken"));
    repo.commit_fix("feature.txt", "feature\n");

    let (status, prompt) = repo.merge();

    assert_eq!(status, "completed");
    assert_eq!(prompt, None, "no agent started");
    assert!(!ran.exists(), "the finding check didn't run");
}

/// DEV-863: without `finding_check` the keys aren't checked.
#[test]
fn without_a_finding_check_a_finding_ticket_lands() {
    let repo = Repo::new("  merge:\n    checks: ['test -f feature.txt']\n");
    set_findings(&repo, &["a:x", "b:y"]);
    repo.commit_fix("feature.txt", "feature\n");

    let (status, prompt) = repo.merge();

    assert_eq!(status, "completed");
    assert_eq!(prompt, None, "no agent started");
    let comments = repo.comments();
    let merged = comments.iter().find(|comment| comment.text.starts_with("Merged without an agent")).expect("the outcome comment");
    assert!(!merged.text.contains("finding check"), "{}", merged.text);
    let checks: Vec<_> = merged.report.as_ref().expect("a report").checks.iter().map(|check| check.command.as_str()).collect();
    assert_eq!(checks, vec!["test -f feature.txt"]);
}
