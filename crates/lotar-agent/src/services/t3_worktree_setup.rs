//! A t3 job's Claude settings (DEV-618). T3 Code makes the job's worktree
//! inside `t3 thread start` and its CLI takes no Claude settings, so the job
//! leaves a record in the repository's git directory before the thread
//! starts, and the T3 project's worktree script (`lotar agent worktree
//! setup`) writes the profile's `tools.disallowed` and `env` into the new
//! worktree's `.claude/settings.local.json` before the agent starts.
//!
//! A record for a new ticket branch matches the worktree on that branch. A
//! thread from the existing ticket branch gets a branch T3 Code names, so its
//! record matches a worktree at its commit only when T3 Code's thread in that
//! worktree has the job's thread title (DEV-674).

use crate::utils::git::{git_command, verified_common_dir};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const SETTINGS_FILE: &str = ".claude/settings.local.json";
/// T3 Code runs the worktree script right after `t3 thread start`; an older
/// record belongs to a job whose thread never got that far.
const PENDING_SECS: u64 = 600;
/// A result outlives any session: an older one belongs to a job that ended
/// without [`finish`] (lotar killed, the Mac restarted) (DEV-678).
const RESULT_SECS: u64 = 2 * 24 * 60 * 60;

/// What a starting job's new worktree gets.
#[derive(Debug, Serialize, Deserialize)]
struct Pending {
    job_id: String,
    ticket_id: String,
    /// The branch T3 Code makes for a new ticket branch; none when the
    /// thread starts from the existing ticket branch (T3 names its own).
    branch: Option<String>,
    /// The commit the worktree starts at.
    commit: String,
    /// The thread's title, which a worktree at `commit` must belong to.
    #[serde(default)]
    title: Option<String>,
    created_at: u64,
    deny: Vec<String>,
    env: BTreeMap<String, String>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or_default()
}

fn pending_dir(common_dir: &Path) -> PathBuf {
    common_dir.join("lotar").join("t3-worktree-setup")
}

fn git_line(dir: &Path, args: &[&str]) -> Option<String> {
    let output = git_command(dir).args(args).output().ok()?;
    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !line.is_empty()).then_some(line)
}

/// The thread a starting job asks T3 Code for: a worktree at `base`, on
/// `branch` when T3 Code makes that branch, titled `title`.
pub struct NewThread<'a> {
    pub base: &'a str,
    pub branch: Option<&'a str>,
    pub title: Option<&'a str>,
}

/// A job's record until its session ends. Dropped without
/// [`Recorded::finish`] (the session never started), it is removed.
pub struct Recorded {
    repo_root: PathBuf,
    job_id: String,
    finished: bool,
}

impl Recorded {
    /// Remove the record and say what became of it: the worktree script's
    /// result, or that the script never claimed it.
    pub fn finish(mut self) -> String {
        self.finished = true;
        finish(&self.repo_root, &self.job_id)
    }
}

impl Drop for Recorded {
    fn drop(&mut self) {
        if !self.finished {
            let _ = finish(&self.repo_root, &self.job_id);
        }
    }
}

/// Record what job `job_id`'s new thread worktree gets. `None` when there is
/// nothing to write.
pub fn record(
    repo_root: &Path,
    job_id: &str,
    ticket_id: &str,
    thread: &NewThread<'_>,
    deny: &[String],
    env: BTreeMap<String, String>,
) -> Result<Option<Recorded>, String> {
    if deny.is_empty() && env.is_empty() {
        return Ok(None);
    }
    let common = verified_common_dir(repo_root).map_err(|err| format!("no git directory for {}: {err}", repo_root.display()))?;
    let base = thread.base;
    let commit = git_line(repo_root, &["rev-parse", "--verify", &format!("{base}^{{commit}}")]).ok_or_else(|| format!("{base} names no commit"))?;
    let pending = Pending {
        job_id: job_id.to_string(),
        ticket_id: ticket_id.to_string(),
        branch: thread.branch.map(str::to_string),
        commit,
        title: thread.title.map(str::to_string),
        created_at: now(),
        deny: deny.to_vec(),
        env,
    };
    let dir = pending_dir(&common);
    fs::create_dir_all(&dir).map_err(|err| format!("creating {}: {err}", dir.display()))?;
    prune(&dir);
    let path = dir.join(format!("{job_id}.json"));
    let text = serde_json::to_string_pretty(&pending).map_err(|err| err.to_string())?;
    fs::write(&path, text).map_err(|err| format!("writing {}: {err}", path.display()))?;
    Ok(Some(Recorded { repo_root: repo_root.to_path_buf(), job_id: job_id.to_string(), finished: false }))
}

const NOT_CLAIMED: &str = "The thread started without the profile's tools.disallowed and env: T3 Code's worktree script for this project \
doesn't run `lotar agent worktree setup` (docs/help/agent.md#t3-code-threads).";

/// When `path` was last written, in seconds since the epoch; now when unknown.
fn modified_at(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map_or(now(), |since| since.as_secs())
}

/// Delete the results older than [`RESULT_SECS`] by their file's age, and the
/// records no worktree took in time; their jobs learn it from the result left
/// in their place, unless `setup` already left one (T3 Code couldn't be
/// asked). A record that can't be read goes by its file's age.
fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let (results, records): (Vec<PathBuf>, Vec<PathBuf>) = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json" || ext == "result"))
        .partition(|path| path.extension().is_some_and(|ext| ext == "result"));
    for path in results.iter().filter(|path| now().saturating_sub(modified_at(path)) > RESULT_SECS) {
        let _ = fs::remove_file(path);
    }
    for path in records {
        let created = match fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str::<Pending>(&text).ok()) {
            Some(pending) => pending.created_at,
            None => modified_at(&path),
        };
        if now().saturating_sub(created) > PENDING_SECS && fs::remove_file(&path).is_ok() {
            let result = fs::OpenOptions::new().write(true).create_new(true).open(path.with_extension("result"));
            let _ = result.and_then(|mut file| file.write_all(NOT_CLAIMED.as_bytes()));
        }
    }
}

/// Remove job `job_id`'s record and say what became of it: the worktree
/// script's result, or that the script never claimed it.
fn finish(repo_root: &Path, job_id: &str) -> String {
    let Ok(common) = verified_common_dir(repo_root) else {
        return format!("The thread's {SETTINGS_FILE} is unknown: the repository's git directory is gone.");
    };
    let dir = pending_dir(&common);
    let result = fs::read_to_string(dir.join(format!("{job_id}.result"))).ok();
    let _ = fs::remove_file(dir.join(format!("{job_id}.result")));
    let unclaimed = fs::remove_file(dir.join(format!("{job_id}.json"))).is_ok();
    match result {
        Some(result) => result.trim().to_string(),
        None if unclaimed => NOT_CLAIMED.to_string(),
        None => format!("The thread's {SETTINGS_FILE} is unknown: the worktree script claimed it but left no result."),
    }
}

/// A record's file and what it holds.
type Claimable = (PathBuf, Pending);

/// `lotar agent worktree setup`, run by T3 Code in the new worktree
/// `worktree`: claim the record of the job starting a thread there and write
/// its settings. `Ok(None)` when no job is starting one (the owner's own
/// threads, other repositories).
pub fn setup(worktree: &Path) -> Result<Option<String>, String> {
    setup_with(worktree, |titles| t3_thread_titles(worktree, titles))
}

/// [`setup`], with `thread_titles` saying which of the given titles T3 Code's
/// threads in `worktree` have.
fn setup_with(worktree: &Path, thread_titles: impl FnOnce(&[String]) -> Result<Vec<String>, String>) -> Result<Option<String>, String> {
    let Ok(common) = verified_common_dir(worktree) else {
        return Ok(None);
    };
    let dir = pending_dir(&common);
    prune(&dir);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(None);
    };
    let branch = git_line(worktree, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let head = git_line(worktree, &["rev-parse", "HEAD"]);
    let records = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| Some((path.clone(), serde_json::from_str::<Pending>(&fs::read_to_string(&path).ok()?).ok()?)))
        .filter(|(_, pending)| now().saturating_sub(pending.created_at) <= PENDING_SECS);
    let (mut candidates, at_commit): (Vec<Claimable>, Vec<Claimable>) = records
        .filter(|(_, pending)| match &pending.branch {
            Some(wanted) => branch.as_deref() == Some(wanted.as_str()),
            None => head.as_deref() == Some(pending.commit.as_str()) && pending.title.is_some(),
        })
        .partition(|(_, pending)| pending.branch.is_some());
    if !at_commit.is_empty() {
        let titles: Vec<String> = at_commit.iter().filter_map(|(_, pending)| pending.title.clone()).collect();
        match thread_titles(&titles) {
            Ok(found) => candidates.extend(at_commit.into_iter().filter(|(_, pending)| pending.title.as_ref().is_some_and(|title| found.contains(title)))),
            Err(err) if candidates.is_empty() => {
                let reason = format!("T3 Code couldn't say which thread {} is for: {err}", worktree.display());
                for (_, pending) in &at_commit {
                    let _ = fs::write(
                        dir.join(format!("{}.result", pending.job_id)),
                        format!("The thread started without the profile's tools.disallowed and env: {reason}"),
                    );
                }
                return Err(reason);
            }
            Err(_) => {}
        }
    }
    // The newest first: an older record at the same commit is a leftover.
    candidates.sort_by_key(|(_, pending)| std::cmp::Reverse(pending.created_at));
    // Removing the record claims it, so two worktrees made at once don't both take it.
    let Some((_, pending)) = candidates.into_iter().find(|(path, _)| fs::remove_file(path).is_ok()) else {
        return Ok(None);
    };
    let outcome = write_settings(worktree, &common, &pending);
    let result = match &outcome {
        Ok(()) => format!(
            "Wrote {SETTINGS_FILE} in {} for the thread ({} deny rules, {} env keys), before its agent started.",
            worktree.display(),
            pending.deny.len(),
            pending.env.len()
        ),
        Err(err) => format!("The thread started without the profile's tools.disallowed and env: {err}"),
    };
    let _ = fs::write(dir.join(format!("{}.result", pending.job_id)), &result);
    outcome.map(|()| Some(format!("{} {}", pending.ticket_id, result)))
}

/// Which of `titles` T3 Code's threads in `worktree` have: `t3 thread list`
/// finds the threads with those titles, `t3 thread show` their worktrees.
/// T3 Code records the worktree on the thread before it runs the script.
fn t3_thread_titles(worktree: &Path, titles: &[String]) -> Result<Vec<String>, String> {
    let table = crate::services::agent_runner::agent_table().map_err(|err| err.to_string())?;
    let spec = table.get("t3").map_err(|err| err.to_string())?;
    let (program, _) = crate::services::agent_runner::block_on(spec.launch_command()).map_err(|err| err.to_string())?;
    let project = std::env::var_os("T3CODE_PROJECT_ROOT")
        .map(PathBuf::from)
        .or_else(|| verified_common_dir(worktree).ok().and_then(|common| common.parent().map(Path::to_path_buf)))
        .unwrap_or_else(|| worktree.to_path_buf());
    let t3 = |args: &[&str]| -> Result<serde_json::Value, String> {
        let output = Command::new(&program)
            .args(args)
            .envs(spec.env.iter())
            .current_dir(&project)
            .stdin(Stdio::null())
            .output()
            .map_err(|err| format!("could not run `t3 {}`: {err}", args[..2].join(" ")))?;
        if !output.status.success() {
            return Err(format!("`t3 {}` failed: {}", args[..2].join(" "), String::from_utf8_lossy(&output.stderr).trim()));
        }
        serde_json::from_slice(&output.stdout).map_err(|err| format!("`t3 {}` printed no JSON: {err}", args[..2].join(" ")))
    };
    let listed = t3(&["thread", "list", "--json"])?;
    let wanted = fs::canonicalize(worktree).unwrap_or_else(|_| worktree.to_path_buf());
    let mut found = Vec::new();
    for thread in listed.as_array().map(Vec::as_slice).unwrap_or_default() {
        let field = |name: &str| thread.get(name).and_then(serde_json::Value::as_str).unwrap_or_default();
        let (id, title) = (field("threadId"), field("title"));
        if id.is_empty() || !titles.iter().any(|wanted| wanted == title) || found.iter().any(|seen| seen == title) {
            continue;
        }
        let shown = t3(&["thread", "show", id, "--json", "--messages", "0"])?;
        let path = shown.pointer("/thread/worktreePath").and_then(serde_json::Value::as_str).unwrap_or_default();
        if !path.is_empty() && fs::canonicalize(path).is_ok_and(|path| path == wanted) {
            found.push(title.to_string());
        }
    }
    Ok(found)
}

fn write_settings(worktree: &Path, common_dir: &Path, pending: &Pending) -> Result<(), String> {
    if git_command(worktree).args(["ls-files", "--error-unmatch", SETTINGS_FILE]).output().is_ok_and(|output| output.status.success()) {
        return Err(format!("{SETTINGS_FILE} is tracked in git, so it isn't changed"));
    }
    keep_out_of_commits(worktree, common_dir)?;
    let path = worktree.join(SETTINGS_FILE);
    let mut settings = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<serde_json::Value>(&text).map_err(|err| format!("{} isn't valid JSON: {err}", path.display()))?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(err) => return Err(format!("reading {}: {err}", path.display())),
    };
    merge(&mut settings, pending).map_err(|err| format!("{}: {err}", path.display()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("creating {}: {err}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&settings).map_err(|err| err.to_string())?;
    fs::write(&path, format!("{text}\n")).map_err(|err| format!("writing {}: {err}", path.display()))
}

/// Add the deny rules the file lacks; the profile's env keys win.
fn merge(settings: &mut serde_json::Value, pending: &Pending) -> Result<(), String> {
    let root = settings.as_object_mut().ok_or("the file holds no JSON object")?;
    if !pending.deny.is_empty() {
        let permissions = root.entry("permissions").or_insert_with(|| serde_json::json!({})).as_object_mut().ok_or("permissions isn't an object")?;
        let deny = permissions.entry("deny").or_insert_with(|| serde_json::json!([])).as_array_mut().ok_or("permissions.deny isn't a list")?;
        for rule in &pending.deny {
            if !deny.iter().any(|existing| existing.as_str() == Some(rule)) {
                deny.push(serde_json::Value::String(rule.clone()));
            }
        }
    }
    if !pending.env.is_empty() {
        let env = root.entry("env").or_insert_with(|| serde_json::json!({})).as_object_mut().ok_or("env isn't an object")?;
        for (key, value) in &pending.env {
            env.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
    }
    Ok(())
}

/// Git must ignore the file, so neither the agent's commit nor LoTaR's commit
/// of leftovers takes it: when no ignore rule covers it yet, the repository's
/// `info/exclude` gets one (the worktree's .gitignore stays unchanged).
fn keep_out_of_commits(worktree: &Path, common_dir: &Path) -> Result<(), String> {
    if git_command(worktree).args(["check-ignore", "-q", SETTINGS_FILE]).output().is_ok_and(|output| output.status.success()) {
        return Ok(());
    }
    let exclude = common_dir.join("info").join("exclude");
    let mut text = fs::read_to_string(&exclude).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("/{SETTINGS_FILE}\n"));
    if let Some(parent) = exclude.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("creating {}: {err}", parent.display()))?;
    }
    fs::write(&exclude, text).map_err(|err| format!("writing {}: {err}", exclude.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git").args(args).current_dir(dir).output().expect("git runs");
        assert!(status.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&status.stderr));
    }

    /// A repository with one commit and a linked worktree on `branch`.
    fn repo_with_worktree(branch: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).expect("repo dir");
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "init"]);
        let worktree = temp.path().join("wt");
        git(&repo, &["worktree", "add", "-q", "-b", branch, worktree.to_str().expect("utf-8 path")]);
        let repo = fs::canonicalize(repo).expect("canonical repo");
        let worktree = fs::canonicalize(worktree).expect("canonical worktree");
        (temp, repo, worktree)
    }

    fn deny() -> Vec<String> {
        vec!["Artifact".to_string(), "Workflow".to_string()]
    }

    /// A new ticket branch's thread.
    fn on_branch(branch: &str) -> NewThread<'_> {
        NewThread { base: "main", branch: Some(branch), title: Some("DEV-1 fix: one") }
    }

    /// A thread from the existing ticket branch, at main's commit.
    fn at_main(title: &str) -> NewThread<'_> {
        NewThread { base: "main", branch: None, title: Some(title) }
    }

    /// T3 Code, whose thread in the worktree is titled `title`.
    fn titled(title: &str) -> impl FnOnce(&[String]) -> Result<Vec<String>, String> + '_ {
        move |titles| Ok(titles.iter().filter(|wanted| *wanted == title).cloned().collect())
    }

    fn record_path(repo: &Path, job_id: &str) -> PathBuf {
        repo.join(".git/lotar/t3-worktree-setup").join(format!("{job_id}.json"))
    }

    #[test]
    fn the_job_worktree_gets_the_profile_settings_and_others_keep_theirs() {
        let (_temp, repo, worktree) = repo_with_worktree("agent/DEV-1");
        fs::create_dir_all(worktree.join(".claude")).expect("claude dir");
        fs::write(worktree.join(SETTINGS_FILE), r#"{"model":"x","permissions":{"deny":["Workflow"],"allow":["Bash"]},"env":{"A":"old","B":"keep"}}"#)
            .expect("existing settings");
        let env = BTreeMap::from([("A".to_string(), "new".to_string())]);
        let recorded = record(&repo, "job-1", "DEV-1", &on_branch("agent/DEV-1"), &deny(), env).expect("recorded").expect("a record");

        let said = setup_with(&worktree, |_| panic!("a branch record asks T3 Code nothing")).expect("setup").expect("claimed");
        assert!(said.starts_with("DEV-1 Wrote"), "{said}");
        let settings: serde_json::Value = serde_json::from_str(&fs::read_to_string(worktree.join(SETTINGS_FILE)).expect("settings")).expect("json");
        assert_eq!(settings["model"], "x");
        assert_eq!(settings["permissions"]["deny"], serde_json::json!(["Workflow", "Artifact"]));
        assert_eq!(settings["permissions"]["allow"], serde_json::json!(["Bash"]));
        assert_eq!(settings["env"], serde_json::json!({"A": "new", "B": "keep"}));

        // Ignored without a .gitignore change, so `git add -A` leaves it out.
        git(&worktree, &["add", "-A"]);
        let staged = Command::new("git").args(["diff", "--cached", "--name-only"]).current_dir(&worktree).output().expect("git diff");
        assert!(String::from_utf8_lossy(&staged.stdout).trim().is_empty());
        assert!(!worktree.join(".gitignore").exists());

        assert!(recorded.finish().starts_with("Wrote"));
        assert!(setup_with(&worktree, titled("DEV-1 fix: one")).expect("setup").is_none(), "claimed once");
    }

    #[test]
    fn a_thread_from_an_existing_branch_matches_by_commit_and_its_title() {
        let (_temp, repo, worktree) = repo_with_worktree("t3/topic");
        let _recorded = record(&repo, "job-2", "DEV-2", &at_main("DEV-2 fix: two"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        // The owner's thread, or another ticket's, at the same commit.
        assert!(setup_with(&worktree, titled("Mine")).expect("setup").is_none());
        assert!(setup_with(&worktree, titled("DEV-3 fix: three")).expect("setup").is_none());
        assert!(!worktree.join(SETTINGS_FILE).exists());
        assert!(setup_with(&worktree, titled("DEV-2 fix: two")).expect("setup").is_some());
        assert!(worktree.join(SETTINGS_FILE).exists());
    }

    #[test]
    fn a_commit_record_is_left_when_t3_code_cant_say_whose_worktree_it_is() {
        let (_temp, repo, worktree) = repo_with_worktree("t3/topic");
        let recorded = record(&repo, "job-6", "DEV-6", &at_main("DEV-6 fix: six"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        let err = setup_with(&worktree, |_| Err("`t3 thread list` failed: Could not reach the T3 Code server".to_string())).expect_err("unknown");
        assert!(err.contains("Could not reach the T3 Code server"), "{err}");
        assert!(!worktree.join(SETTINGS_FILE).exists());
        assert!(record_path(&repo, "job-6").exists(), "still claimable");
        let said = recorded.finish();
        assert!(said.contains("T3 Code couldn't say which thread"), "{said}");
        assert!(!record_path(&repo, "job-6").exists());
    }

    #[test]
    fn an_expired_record_keeps_the_reason_t3_code_couldnt_be_asked() {
        let (_temp, repo, worktree) = repo_with_worktree("t3/topic");
        let recorded = record(&repo, "job-13", "DEV-13", &at_main("DEV-13 fix: thirteen"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        setup_with(&worktree, |_| Err("Could not reach the T3 Code server".to_string())).expect_err("unknown");
        let mut text: serde_json::Value = serde_json::from_str(&fs::read_to_string(record_path(&repo, "job-13")).expect("record")).expect("json");
        text["created_at"] = serde_json::json!(now() - PENDING_SECS - 1);
        fs::write(record_path(&repo, "job-13"), text.to_string()).expect("expired record");
        let _other = record(&repo, "job-14", "DEV-14", &on_branch("agent/DEV-14"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        assert!(!record_path(&repo, "job-13").exists(), "deleted by record");
        let said = recorded.finish();
        assert!(said.contains("T3 Code couldn't say which thread") && said.contains("Could not reach the T3 Code server"), "{said}");
    }

    #[test]
    fn the_newest_of_two_matching_records_wins() {
        let (_temp, repo, worktree) = repo_with_worktree("t3/topic");
        let old = record(&repo, "job-7", "DEV-7", &at_main("DEV-7 fix: seven"), &["Old".to_string()], BTreeMap::new()).expect("recorded").expect("a record");
        let mut text: serde_json::Value = serde_json::from_str(&fs::read_to_string(record_path(&repo, "job-7")).expect("record")).expect("json");
        text["created_at"] = serde_json::json!(now() - 60);
        fs::write(record_path(&repo, "job-7"), text.to_string()).expect("older record");
        let _new = record(&repo, "job-8", "DEV-7", &at_main("DEV-7 fix: seven"), &["New".to_string()], BTreeMap::new()).expect("recorded").expect("a record");
        assert!(setup_with(&worktree, titled("DEV-7 fix: seven")).expect("setup").is_some());
        let settings: serde_json::Value = serde_json::from_str(&fs::read_to_string(worktree.join(SETTINGS_FILE)).expect("settings")).expect("json");
        assert_eq!(settings["permissions"]["deny"], serde_json::json!(["New"]));
        assert!(old.finish().contains("doesn't run `lotar agent worktree setup`"));
    }

    #[test]
    fn a_record_dropped_before_its_session_finishes_is_removed() {
        let (_temp, repo, _worktree) = repo_with_worktree("agent/DEV-9");
        let recorded = record(&repo, "job-9", "DEV-9", &on_branch("agent/DEV-9"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        assert!(record_path(&repo, "job-9").exists());
        drop(recorded);
        assert!(!record_path(&repo, "job-9").exists());
        assert!(!repo.join(".git/lotar/t3-worktree-setup/job-9.result").exists());
    }

    #[test]
    fn expired_records_are_deleted_and_their_jobs_learn_why() {
        let (_temp, repo, worktree) = repo_with_worktree("agent/DEV-10");
        let expired = record(&repo, "job-10", "DEV-10", &on_branch("agent/DEV-10"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        let mut text: serde_json::Value = serde_json::from_str(&fs::read_to_string(record_path(&repo, "job-10")).expect("record")).expect("json");
        text["created_at"] = serde_json::json!(now() - PENDING_SECS - 1);
        fs::write(record_path(&repo, "job-10"), text.to_string()).expect("expired record");
        let broken = repo.join(".git/lotar/t3-worktree-setup/broken.json");
        fs::write(&broken, "{").expect("unreadable record");
        assert!(setup_with(&worktree, titled("")).expect("setup").is_none(), "an expired record isn't claimed");
        assert!(!record_path(&repo, "job-10").exists(), "deleted by setup");
        assert!(broken.exists(), "a fresh unreadable record may still be being written");
        let old = SystemTime::now() - std::time::Duration::from_secs(PENDING_SECS + 60);
        fs::File::options().write(true).open(&broken).and_then(|file| file.set_modified(old)).expect("older unreadable record");
        assert!(setup_with(&worktree, titled("")).expect("setup").is_none());
        assert!(!broken.exists(), "an old unreadable record is deleted by its file's age");
        assert!(expired.finish().contains("doesn't run `lotar agent worktree setup`"));

        let _other = record(&repo, "job-11", "DEV-11", &on_branch("agent/DEV-11"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        let mut text: serde_json::Value = serde_json::from_str(&fs::read_to_string(record_path(&repo, "job-11")).expect("record")).expect("json");
        text["created_at"] = serde_json::json!(now() - PENDING_SECS - 1);
        fs::write(record_path(&repo, "job-11"), text.to_string()).expect("expired record");
        let _fresh = record(&repo, "job-12", "DEV-12", &on_branch("agent/DEV-12"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        assert!(!record_path(&repo, "job-11").exists(), "deleted by record");
        assert!(record_path(&repo, "job-12").exists());
    }

    #[test]
    fn results_older_than_two_days_are_deleted_and_fresh_ones_kept() {
        let (_temp, repo, worktree) = repo_with_worktree("agent/DEV-15");
        let dir = repo.join(".git/lotar/t3-worktree-setup");
        fs::create_dir_all(&dir).expect("setup dir");
        let (old, fresh, almost) = (dir.join("job-15.result"), dir.join("job-16.result"), dir.join("job-17.result"));
        for path in [&old, &fresh, &almost] {
            fs::write(path, "Wrote the settings.").expect("result");
        }
        let age = |path: &Path, secs: u64| {
            let when = SystemTime::now() - std::time::Duration::from_secs(secs);
            fs::File::options().write(true).open(path).and_then(|file| file.set_modified(when)).expect("aged result");
        };
        age(&old, RESULT_SECS + 60);
        age(&almost, RESULT_SECS - 60);
        assert!(setup_with(&worktree, titled("")).expect("setup").is_none());
        assert!(!old.exists(), "a result older than two days is deleted");
        assert!(fresh.exists() && almost.exists(), "younger results are kept for their jobs");

        age(&fresh, RESULT_SECS + 60);
        let _recorded = record(&repo, "job-18", "DEV-18", &on_branch("agent/DEV-18"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        assert!(!fresh.exists(), "deleted by record");
        assert!(almost.exists());
    }

    #[test]
    fn other_worktrees_and_profiles_without_settings_are_unchanged() {
        let (_temp, repo, worktree) = repo_with_worktree("t3/mine");
        assert!(record(&repo, "job-3", "DEV-3", &at_main("DEV-3 fix: three"), &[], BTreeMap::new()).expect("nothing to record").is_none());
        let recorded = record(&repo, "job-4", "DEV-4", &on_branch("agent/DEV-4"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        assert!(setup_with(&worktree, titled("DEV-1 fix: one")).expect("setup").is_none());
        assert!(!worktree.join(".claude").exists());
        assert!(recorded.finish().contains("doesn't run `lotar agent worktree setup`"));
        let outside = tempfile::tempdir().expect("tempdir");
        assert!(setup(outside.path()).expect("no git").is_none());
    }

    #[test]
    fn a_file_that_cannot_be_merged_is_left_and_the_job_learns_why() {
        let (_temp, repo, worktree) = repo_with_worktree("agent/DEV-5");
        fs::create_dir_all(worktree.join(".claude")).expect("claude dir");
        fs::write(worktree.join(SETTINGS_FILE), "not json").expect("broken settings");
        let recorded = record(&repo, "job-5", "DEV-5", &on_branch("agent/DEV-5"), &deny(), BTreeMap::new()).expect("recorded").expect("a record");
        let err = setup(&worktree).expect_err("can't merge");
        assert!(err.contains("isn't valid JSON"), "{err}");
        assert_eq!(fs::read_to_string(worktree.join(SETTINGS_FILE)).expect("settings"), "not json");
        assert!(recorded.finish().contains("isn't valid JSON"));
    }
}
