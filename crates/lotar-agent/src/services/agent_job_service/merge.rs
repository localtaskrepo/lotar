//! lotar's own merge (DEV-605). A merge job of a repository with
//! `agent.merge` rebases the ticket's branch onto the main checkout's
//! branch, runs the repository's checks, fast-forwards, pushes and reruns a
//! finding's check, the steps the merge agent's prompt names, before any
//! agent starts. Only what needs judgment starts the merge agent: a
//! conflict, a failed check, others' unpushed commits, a refused push.

use super::{
    JOB_REGISTRY, OUTPUT_DRAIN_LIMIT, git_command, linked_worktrees, merged_landed, prepare_worktree, record_and_emit, resolve_repo_root, rev_parse,
    should_stop_job, terminate_child, update_job,
};
use crate::config::trust::{TrustService, TrustState};
use crate::config::types::{AgentMergeConfig, ResolvedConfig};
use crate::types::{CommentReport, ReportCheck, ReportCommit, TaskComment};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Rounds of rebase and checks when main moves on meanwhile.
const MAX_ROUNDS: usize = 3;
/// Lines of a failed command's output that a hand-off quotes.
const OUTPUT_TAIL_LINES: usize = 30;
/// Bytes of a command's output kept for its tail.
const OUTPUT_TAIL_BYTES: usize = 16 * 1024;
/// Files whose change makes the worktree's dependencies stale (lotar DEV-556).
const LOCKFILES: &[&str] = &["package-lock.json", "pnpm-lock.yaml", "yarn.lock"];

/// How lotar's merge ended.
pub(super) enum Outcome {
    /// Landed, or nothing to merge: the job completes.
    Completed(Summary),
    /// A step needs judgment: the merge agent takes over.
    HandOff(Summary),
    /// The job was cancelled.
    Cancelled,
}

/// The outcome comment: its text and report.
pub(super) struct Summary {
    pub text: String,
    pub report: CommentReport,
}

/// Why the merge stopped before its end.
enum Stop {
    Nothing(String),
    HandOff(String),
    Cancelled,
}

fn hand_off(why: impl Into<String>) -> Stop {
    Stop::HandOff(why.into())
}

/// Merge `ticket_id`'s branch as the merge agent would, for a merge job of
/// a repository with `agent.merge`.
pub(super) fn run(job_id: &str, tasks_dir: &Path, ticket_id: &str, config: &ResolvedConfig, settings: &AgentMergeConfig) -> Outcome {
    let branch = super::build_worktree_branch(&config.agent_worktree.branch_prefix, &super::sanitize_worktree_token(ticket_id));
    let Some(repo_root) = resolve_repo_root(tasks_dir) else {
        return Outcome::HandOff(Summary {
            text: "lotar's merge needs the merge agent: no git repository found.".to_string(),
            report: CommentReport::default(),
        });
    };
    let mut merge = Merge {
        job_id,
        tasks_dir,
        ticket_id,
        config,
        settings,
        repo_root,
        branch,
        main: String::new(),
        done: Vec::new(),
        checks: Vec::new(),
        landed: Vec::new(),
    };
    let result = merge.land();
    let steps = merge.done.iter().map(|step| format!("\n- {step}")).collect::<String>();
    let report = CommentReport { commits: merge.landed.clone(), checks: merge.checks.clone(), ..CommentReport::default() };
    match result {
        Ok(()) => Outcome::Completed(Summary { text: format!("Merged without an agent: `{}` is in `{}`.\n{steps}", merge.branch, merge.main), report }),
        Err(Stop::Nothing(why)) => Outcome::Completed(Summary { text: format!("Nothing to merge: {why}.\n{steps}"), report }),
        Err(Stop::HandOff(why)) => Outcome::HandOff(Summary {
            text: format!("lotar's merge needs the merge agent: {why}.\n{steps}"),
            report: CommentReport { next: Some("The merge agent takes over in a new session".to_string()), ..report },
        }),
        Err(Stop::Cancelled) => Outcome::Cancelled,
    }
}

/// What the merge agent reads before its prompt when lotar's merge hands
/// off: what lotar did and where it stopped.
pub(super) fn hand_off_prompt(summary: &Summary) -> String {
    let landed = if summary.report.commits.is_empty() {
        String::new()
    } else {
        "\nThe work is in the main checkout's branch already: don't stop at \"nothing to merge\", do the steps after the fast-forward that are left."
            .to_string()
    };
    format!(
        "LoTaR started this merge itself and stopped where it needs your judgment. Its comment on the ticket:\n\n{}\n{landed}\nContinue from there with the steps below.\n\n",
        summary.text.trim()
    )
}

struct Merge<'a> {
    job_id: &'a str,
    tasks_dir: &'a Path,
    ticket_id: &'a str,
    config: &'a ResolvedConfig,
    settings: &'a AgentMergeConfig,
    repo_root: PathBuf,
    branch: String,
    /// The main checkout's current branch.
    main: String,
    /// What lotar did, for the comment.
    done: Vec<String>,
    checks: Vec<ReportCheck>,
    /// The commits this merge brought to main.
    landed: Vec<ReportCommit>,
}

impl Merge<'_> {
    fn land(&mut self) -> Result<(), Stop> {
        let root = self.repo_root.clone();
        self.main = git(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"]).map_err(|_| hand_off("the main checkout is on no branch"))?;
        let branch = self.branch.clone();
        if rev_parse(&root, &format!("refs/heads/{branch}")).is_none() {
            return Err(hand_off(format!("`{branch}` doesn't exist")));
        }
        let landed = merged_landed(&root, self.tasks_dir, &branch).unwrap_or(false);
        // Work in another repository is merged there first (DEV-677), also
        // when this branch has commits of its own (DEV-679).
        if let Some(elsewhere) = work_elsewhere(self.tasks_dir, self.ticket_id, &root, &branch) {
            let here = if landed { format!("`{branch}` is in `{}` already", self.main) } else { format!("`{branch}` isn't merged yet") };
            return Err(hand_off(format!(
                "{here}, but the ticket's work is in another repository: {elsewhere}. Review and merge it there, and `{branch}` here too"
            )));
        }
        if landed {
            return Err(Stop::Nothing(format!("`{branch}` is in `{}` already", self.main)));
        }
        let trusted_before = TrustService::state(self.tasks_dir) == TrustState::Trusted;
        let (worktree, fresh) = self.worktree()?;
        let main_before = rev_parse(&root, "HEAD").ok_or_else(|| hand_off("the main checkout has no commit"))?;

        for round in 1.. {
            let target = rev_parse(&root, "HEAD").ok_or_else(|| hand_off("the main checkout has no commit"))?;
            let before = rev_parse(&worktree, "HEAD").ok_or_else(|| hand_off(format!("`{}` has no commit", worktree.display())))?;
            self.rebase(&worktree, &target)?;
            let after = rev_parse(&worktree, "HEAD").ok_or_else(|| hand_off(format!("`{}` has no commit", worktree.display())))?;
            if before == after {
                self.done.push(format!("Rebase onto `{}` changed nothing (`{}`)", self.main, short(&after)));
            } else {
                self.done.push(format!("Rebased `{branch}` onto `{}` (`{}`): `{}`", self.main, short(&target), short(&after)));
            }
            if let Some(install) = self.settings.install.as_deref().filter(|install| !install.trim().is_empty())
                && ((fresh && round == 1) || lockfile_changed(&worktree, &before, &after))
            {
                self.command("install", install, &worktree, false)?;
            }
            let passed_on_head = if before == after { passed_checks(self.tasks_dir, self.ticket_id, &after) } else { Vec::new() };
            self.checks.clear();
            for check in self.settings.checks.iter().map(|check| check.trim()).filter(|check| !check.is_empty()) {
                if passed_on_head.iter().any(|passed| passed == check) {
                    self.checks.push(ReportCheck {
                        command: check.to_string(),
                        result: "not_run".to_string(),
                        reason: Some(format!("skipped: passed on `{}`, which the rebase left unchanged", short(&after))),
                        ..ReportCheck::default()
                    });
                    continue;
                }
                self.command("check", check, &worktree, true)?;
            }
            let changes = git(&worktree, &["status", "--porcelain"]).map_err(hand_off)?;
            if !changes.is_empty() {
                return Err(hand_off(format!("the checks left changes in `{}`:\n```\n{}\n```", worktree.display(), first_lines(&changes, 10))));
            }
            match git(&root, &["merge", "--ff-only", "--quiet", &branch]) {
                Ok(_) => break,
                Err(_) if round < MAX_ROUNDS && rev_parse(&root, "HEAD").as_deref() != Some(target.as_str()) => {
                    self.done.push(format!("`{}` moved on during the checks: rebasing again", self.main));
                }
                Err(err) => return Err(hand_off(format!("`git merge --ff-only {branch}` in the main checkout refused: {err}"))),
            }
        }
        let merged = rev_parse(&root, "HEAD").unwrap_or_default();
        self.done.push(format!("Fast-forwarded `{}` to `{}`", self.main, short(&merged)));
        self.landed = landed_commits(&root, &main_before);

        self.amend_tasks(&merged)?;
        self.trust(trusted_before)?;
        if self.settings.push {
            self.push(&main_before)?;
        }
        self.finding_check()?;
        // The amend rewrote the tip: the report names the commits on main.
        self.landed = landed_commits(&root, &main_before);
        Ok(())
    }

    /// The worktree that has the ticket's branch checked out, else a new
    /// one of LoTaR's; `true` for a new one.
    fn worktree(&mut self) -> Result<(PathBuf, bool), Stop> {
        let worktrees = linked_worktrees(&self.repo_root).map_err(|err| hand_off(err.to_string()))?;
        if let Some((path, _)) = worktrees.into_iter().find(|(path, on)| on.as_deref() == Some(self.branch.as_str()) && path.is_dir()) {
            let changes = git(&path, &["status", "--porcelain"]).map_err(hand_off)?;
            if !changes.is_empty() {
                return Err(hand_off(format!("`{}` has uncommitted changes, which lotar leaves alone", path.display())));
            }
            return Ok((path, false));
        }
        let context =
            prepare_worktree(self.tasks_dir, self.ticket_id, self.config).map_err(|err| hand_off(format!("no worktree for `{}`: {err}", self.branch)))?;
        self.done.push(format!("Checked `{}` out in the new worktree `{}`", self.branch, context.working_dir.display()));
        self.done.extend(context.note);
        Ok((context.working_dir, true))
    }

    fn rebase(&mut self, worktree: &Path, target: &str) -> Result<(), Stop> {
        let Err(err) = git(worktree, &["rebase", "--quiet", target]) else {
            return Ok(());
        };
        let conflicts = git(worktree, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
        let aborted = git(worktree, &["rebase", "--abort"]).is_ok();
        let state = if aborted { format!("lotar aborted the rebase, `{}` is unchanged", self.branch) } else { "`git rebase --abort` failed too".to_string() };
        if conflicts.is_empty() {
            return Err(hand_off(format!("rebasing `{}` onto `{}` failed ({state}): {err}", self.branch, self.main)));
        }
        let files = conflicts.lines().map(|file| format!("`{file}`")).collect::<Vec<_>>().join(", ");
        Err(hand_off(format!("rebasing `{}` onto `{}` conflicts in {files}; {state}", self.branch, self.main)))
    }

    /// Run the repository's `install` or a check in `dir`; a failure hands
    /// off with its output's tail.
    fn command(&mut self, what: &str, command: &str, dir: &Path, report: bool) -> Result<(), Stop> {
        self.progress(&format!("Running the {what} `{command}` in {}", dir.display()));
        let ran = run_shell(self.job_id, command, dir, &[])?;
        let passed = ran.as_ref().is_ok_and(|(success, _)| *success);
        if report {
            self.checks.push(ReportCheck {
                command: command.to_string(),
                result: if passed { "passed" } else { "failed" }.to_string(),
                ..ReportCheck::default()
            });
        }
        match ran {
            Ok((true, _)) => {
                self.done.push(format!("The {what} `{command}` passed"));
                Ok(())
            }
            Ok((false, output)) => Err(hand_off(format!("the {what} `{command}` failed:\n```\n{}\n```", last_lines(&output, OUTPUT_TAIL_LINES)))),
            Err(err) => Err(hand_off(format!("the {what} `{command}` could not run: {err}"))),
        }
    }

    /// Amend the main checkout's pending ticket changes into the commit
    /// just merged while it is still main's tip: they belong to no ticket
    /// (owner 2026-11-12, the merge prompt's step 4).
    fn amend_tasks(&mut self, merged: &str) -> Result<(), Stop> {
        let Some(tasks) = std::path::absolute(self.tasks_dir).ok().and_then(|tasks| tasks.strip_prefix(&self.repo_root).ok().map(Path::to_path_buf)) else {
            return Ok(());
        };
        let tasks = tasks.to_string_lossy().to_string();
        let root = self.repo_root.clone();
        if git(&root, &["status", "--porcelain", "--", &tasks]).map_err(hand_off)?.is_empty() || rev_parse(&root, "HEAD").as_deref() != Some(merged) {
            return Ok(());
        }
        git(&root, &["add", "--all", "--", &tasks]).map_err(|err| hand_off(format!("`git add {tasks}` in the main checkout failed: {err}")))?;
        git(&root, &["commit", "--quiet", "--amend", "--no-edit", "--", &tasks])
            .map_err(|err| hand_off(format!("amending the pending `{tasks}` changes into `{}` failed: {err}", short(merged))))?;
        self.done.push(format!("Amended the pending `{tasks}` changes into the merged commit: `{}`", short(&rev_parse(&root, "HEAD").unwrap_or_default())));
        Ok(())
    }

    /// Trust the merged rules and agent profiles again, as the merge
    /// agent's `lotar trust` does (owner 2026-11-11), when the repository
    /// was trusted before the merge changed them.
    fn trust(&mut self, trusted_before: bool) -> Result<(), Stop> {
        if TrustService::state(self.tasks_dir).allows_commands() {
            return Ok(());
        }
        if !trusted_before {
            self.done.push(
                "The merge changed rules, agent profiles or `agent.merge` of a repository that wasn't trusted: lotar didn't run `lotar trust`".to_string(),
            );
            return Ok(());
        }
        TrustService::trust(self.tasks_dir)
            .map_err(|err| hand_off(format!("`lotar trust` failed after the merge changed rules, agent profiles or `agent.merge`: {err}")))?;
        self.done.push("The merge changed rules, agent profiles or `agent.merge`: ran `lotar trust`".to_string());
        Ok(())
    }

    /// Push only finished work (owner 2026-11-12): a clean main checkout
    /// whose unpushed commits are all this merge's.
    fn push(&mut self, main_before: &str) -> Result<(), Stop> {
        let root = self.repo_root.clone();
        let changes = git(&root, &["status", "--porcelain"]).map_err(hand_off)?;
        if !changes.is_empty() {
            return Err(hand_off(format!("the main checkout has uncommitted changes, so lotar didn't push:\n```\n{}\n```", first_lines(&changes, 10))));
        }
        let upstream = git(&root, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
            .map_err(|_| hand_off(format!("`{}` has no upstream, so lotar didn't push", self.main)))?;
        let ours = git(&root, &["rev-list", &format!("{main_before}..HEAD")]).map_err(hand_off)?;
        let others = git(&root, &["log", "--format=%h %s", &format!("@{{u}}..{main_before}")]).map_err(hand_off)?;
        if !others.is_empty() {
            return Err(hand_off(format!(
                "`{}` holds unpushed commits that aren't this merge's, so lotar didn't push:\n```\n{}\n```",
                self.main,
                first_lines(&others, 10)
            )));
        }
        let mut push = git_command(&root);
        push.args(["push", "--quiet"]).env("GIT_TERMINAL_PROMPT", "0");
        match run_process(self.job_id, push)? {
            Ok((true, _)) => {
                let count = ours.lines().count();
                self.done.push(format!("Pushed {count} commit{} to `{upstream}`", if count == 1 { "" } else { "s" }));
                Ok(())
            }
            Ok((false, output)) => Err(hand_off(format!("`git push` was refused:\n```\n{}\n```", last_lines(&output, OUTPUT_TAIL_LINES)))),
            Err(err) => Err(hand_off(format!("`git push` could not run: {err}"))),
        }
    }

    /// For a ticket with a finding key: the finding check must no longer
    /// list it.
    fn finding_check(&mut self) -> Result<(), Stop> {
        let Some(command) = self.settings.finding_check.as_deref().map(str::trim).filter(|command| !command.is_empty()) else {
            return Ok(());
        };
        let storage = crate::storage::manager::Storage::new(self.tasks_dir);
        let key = crate::services::task_service::TaskService::get(&storage, self.ticket_id, None)
            .ok()
            .and_then(|task| crate::utils::custom_fields::extract_value_strings(&task.custom_fields, crate::utils::custom_fields::FINDING_FIELD))
            .and_then(|keys| keys.into_iter().next());
        let Some(key) = key else {
            return Ok(());
        };
        self.progress(&format!("Running the finding check `{command}` for `[{key}]`"));
        let root = self.repo_root.clone();
        let ran = run_shell(self.job_id, command, &root, &[("LOTAR_FINDING", key.as_str())])?;
        let listed = format!("[{key}]");
        match ran {
            Ok((_, output)) if !output.contains(&listed) => {
                self.checks.push(ReportCheck { command: command.to_string(), result: "passed".to_string(), ..ReportCheck::default() });
                self.done.push(format!("The finding check `{command}` no longer lists `{listed}`"));
                Ok(())
            }
            Ok((_, output)) => {
                self.checks.push(ReportCheck {
                    command: command.to_string(),
                    result: "failed".to_string(),
                    reason: Some(format!("still lists {listed}")),
                    ..ReportCheck::default()
                });
                let line = output.lines().find(|line| line.contains(&listed)).unwrap_or_default().trim();
                Err(hand_off(format!("the finding check `{command}` still lists `{listed}`: {line}")))
            }
            Err(err) => Err(hand_off(format!("the finding check `{command}` could not run: {err}"))),
        }
    }

    fn progress(&self, message: &str) {
        update_job(self.job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(message.to_string()));
        });
    }
}

/// `git` in `dir`: its trimmed output, or its error output.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = git_command(dir).args(args).env("GIT_EDITOR", "true").output().map_err(|err| format!("git {}: {err}", args.join(" ")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if err.is_empty() { String::from_utf8_lossy(&output.stdout).trim().to_string() } else { err })
    }
}

fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

fn first_lines(text: &str, count: usize) -> String {
    text.lines().take(count).collect::<Vec<_>>().join("\n")
}

fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<_> = text.trim_end().lines().collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

/// Whether `before..after` changes a lockfile anywhere in the tree.
fn lockfile_changed(dir: &Path, before: &str, after: &str) -> bool {
    before != after
        && git(dir, &["diff", "--name-only", before, after])
            .is_ok_and(|files| files.lines().any(|file| LOCKFILES.contains(&Path::new(file).file_name().and_then(|name| name.to_str()).unwrap_or_default())))
}

/// `main_before..HEAD` in the main checkout, oldest first.
fn landed_commits(root: &Path, main_before: &str) -> Vec<ReportCommit> {
    git(root, &["log", "--reverse", "--format=%h %s", &format!("{main_before}..HEAD")])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (hash, subject) = line.split_once(' ').unwrap_or((line, ""));
            (!hash.is_empty()).then(|| ReportCommit { hash: hash.to_string(), what: Some(subject.to_string()), ..ReportCommit::default() })
        })
        .collect()
}

/// The checks the ticket's latest report on `head` names as passed: the
/// fix or rework comment's, when the rebase left its commit unchanged.
fn passed_checks(tasks_dir: &Path, ticket_id: &str, head: &str) -> Vec<String> {
    let storage = crate::storage::manager::Storage::new(tasks_dir);
    let Ok(task) = crate::services::task_service::TaskService::get(&storage, ticket_id, None) else {
        return Vec::new();
    };
    let on_head = |comment: &&TaskComment| {
        comment.report.as_ref().is_some_and(|report| report.commits.iter().any(|commit| commit.hash.len() >= 7 && head.starts_with(commit.hash.as_str())))
    };
    task.comments
        .iter()
        .rev()
        .find(on_head)
        .and_then(|comment| comment.report.as_ref())
        .map(|report| report.checks.iter().filter(|check| check.result == "passed").map(|check| check.command.trim().to_string()).collect())
        .unwrap_or_default()
}

/// Another repository the ticket names, by a path in its description or
/// comments, whose `branch` holds commits beyond that repository's main
/// checkout branch: lotar's own branch being on main then says nothing
/// about the ticket's work (lotar DEV-677).
pub(super) fn work_elsewhere(tasks_dir: &Path, ticket_id: &str, root: &Path, branch: &str) -> Option<String> {
    named_checkouts(tasks_dir, ticket_id, root)?.into_iter().find_map(|(checkout, main)| {
        let ahead =
            git(&checkout, &["cherry", &main, &format!("refs/heads/{branch}")]).map(|cherry| cherry.lines().filter(|line| line.starts_with('+')).count());
        match ahead {
            Ok(ahead @ 1..) => {
                Some(format!("`{branch}` in `{}` holds {ahead} commit{} beyond its `{main}`", checkout.display(), if ahead == 1 { "" } else { "s" }))
            }
            _ => None,
        }
    })
}

/// Whether a commit named after the ticket is on the main checkout's branch
/// of `root` or of another repository the ticket names. A fix job that
/// wrote nothing leaves none (lotar DEV-680). `true` when lotar can't tell.
pub(super) fn built_anywhere(tasks_dir: &Path, ticket_id: &str, root: &Path) -> bool {
    let Some(others) = named_checkouts(tasks_dir, ticket_id, root) else {
        return true;
    };
    // The subject starts with the ID: `DEV-1 Title`, `DEV-1: ...`, `DEV-1-fix`, not `DEV-10`.
    let starts_with_id = |subject: &str| {
        subject.get(..ticket_id.len()).is_some_and(|start| start.eq_ignore_ascii_case(ticket_id))
            && !subject[ticket_id.len()..].starts_with(|ch: char| ch.is_ascii_alphanumeric())
    };
    let names_ticket = |dir: &Path, main: &str| {
        git(dir, &["log", "--format=%s", "--regexp-ignore-case", "--fixed-strings", &format!("--grep={ticket_id}"), main])
            .map(|subjects| subjects.lines().any(starts_with_id))
    };
    if names_ticket(root, "HEAD").unwrap_or(true) {
        return true;
    }
    others.iter().any(|(checkout, main)| names_ticket(checkout, main).unwrap_or(true))
}

/// The main checkouts of the other repositories the ticket names, by a path
/// in its description or comments, with their current branch.
fn named_checkouts(tasks_dir: &Path, ticket_id: &str, root: &Path) -> Option<Vec<(PathBuf, String)>> {
    let storage = crate::storage::manager::Storage::new(tasks_dir);
    let task = crate::services::task_service::TaskService::get(&storage, ticket_id, None).ok()?;
    let ours = common_dir(root)?;
    let texts = std::iter::once(task.description.unwrap_or_default())
        .chain(task.comments.into_iter().flat_map(|comment| std::iter::once(comment.text).chain(comment.report.and_then(|report| report.next))));
    let mut seen = vec![ours];
    let mut checkouts = Vec::new();
    for path in texts.flat_map(|text| named_paths(&text)) {
        let Some(common) = path.ancestors().find(|dir| dir.exists()).and_then(crate::utils::git::find_repo_root).and_then(|repo| common_dir(&repo)) else {
            continue;
        };
        if seen.contains(&common) {
            continue;
        }
        seen.push(common.clone());
        let Some(checkout) = common.parent().filter(|_| common.file_name().is_some_and(|name| name == ".git")) else {
            continue;
        };
        let Ok(main) = git(checkout, &["symbolic-ref", "--quiet", "--short", "HEAD"]) else {
            continue;
        };
        checkouts.push((checkout.to_path_buf(), main));
    }
    Some(checkouts)
}

/// The repository's shared git directory, which its worktrees share too.
fn common_dir(dir: &Path) -> Option<PathBuf> {
    git(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).ok().and_then(|common| std::fs::canonicalize(common).ok())
}

/// The absolute and `~/` paths in `text`.
fn named_paths(text: &str) -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    text.split(|ch: char| ch.is_whitespace() || "`'\"()[]<>,;".contains(ch))
        .map(|token| token.trim_end_matches(['.', ':']))
        .filter_map(|token| match token.strip_prefix("~/") {
            Some(rest) => home.as_ref().map(|home| home.join(rest)),
            None => token.starts_with('/').then(|| PathBuf::from(token)),
        })
        .collect()
}

/// Run `command` through the shell in `dir`, without LoTaR's own
/// environment: a check that runs lotar must not change this workspace.
fn run_shell(job_id: &str, command: &str, dir: &Path, env: &[(&str, &str)]) -> Result<Result<(bool, String), String>, Stop> {
    let mut cmd = if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);
        cmd
    };
    cmd.current_dir(dir);
    crate::utils::git::clear_repository_env(&mut cmd);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("LOTAR_") {
            cmd.env_remove(name);
        }
    }
    cmd.envs(env.iter().copied());
    run_process(job_id, cmd)
}

/// Run `cmd` until it exits or the job is cancelled: `Ok((success, output))`
/// with its stdout and stderr's tail, or why it couldn't start.
fn run_process(job_id: &str, mut cmd: Command) -> Result<Result<(bool, String), String>, Stop> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => return Ok(Err(err.to_string())),
    };
    let tail = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let drains: Vec<_> =
        [child.stdout.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>), child.stderr.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>)]
            .into_iter()
            .flatten()
            .map(|mut pipe| {
                let tail = std::sync::Arc::clone(&tail);
                std::thread::spawn(move || {
                    let mut buf = [0u8; 4096];
                    while let Ok(n) = pipe.read(&mut buf) {
                        if n == 0 {
                            break;
                        }
                        if let Ok(mut tail) = tail.lock() {
                            tail.extend_from_slice(&buf[..n]);
                            let excess = tail.len().saturating_sub(OUTPUT_TAIL_BYTES);
                            tail.drain(..excess);
                        }
                    }
                })
            })
            .collect();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(err) => return Ok(Err(err.to_string())),
        }
        if should_stop_job(job_id) {
            terminate_child(&mut child);
            let _ = child.wait();
            return Err(Stop::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    // Processes the command left running keep its output open: wait for
    // them as long as a runner's (`OUTPUT_DRAIN_LIMIT`), then stop them,
    // or one dev server a check starts holds up every later merge.
    let deadline = std::time::Instant::now() + OUTPUT_DRAIN_LIMIT;
    while !drains.iter().all(std::thread::JoinHandle::is_finished) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    if !drains.iter().all(std::thread::JoinHandle::is_finished) {
        terminate_child(&mut child);
    }
    let output = tail.lock().map(|tail| String::from_utf8_lossy(&tail).to_string()).unwrap_or_default();
    Ok(Ok((status.success(), output)))
}

/// Whether `job_id` is a merge job.
pub(super) fn is_merge_job(job_id: &str) -> bool {
    JOB_REGISTRY.lock().ok().is_some_and(|registry| registry.jobs.get(job_id).is_some_and(|state| state.record.is_merge_job))
}
