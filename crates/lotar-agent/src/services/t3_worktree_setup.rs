//! A t3 job's Claude settings (DEV-618). T3 Code makes the job's worktree
//! inside `t3 thread start` and its CLI takes no Claude settings, so the job
//! leaves a record in the repository's git directory before the thread
//! starts, and the T3 project's worktree script (`lotar agent worktree
//! setup`) writes the profile's `tools.disallowed` and `env` into the new
//! worktree's `.claude/settings.local.json` before the agent starts.

use crate::utils::git::{git_command, verified_common_dir};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SETTINGS_FILE: &str = ".claude/settings.local.json";
/// T3 Code runs the worktree script right after `t3 thread start`; an older
/// record belongs to a job whose thread never got that far.
const PENDING_SECS: u64 = 600;

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

/// Record what job `job_id`'s new thread worktree gets: a worktree at `base`
/// (on `branch` when T3 Code makes that branch). Returns false when there is
/// nothing to write.
pub fn record(
    repo_root: &Path,
    job_id: &str,
    ticket_id: &str,
    base: &str,
    branch: Option<&str>,
    deny: &[String],
    env: BTreeMap<String, String>,
) -> Result<bool, String> {
    if deny.is_empty() && env.is_empty() {
        return Ok(false);
    }
    let common = verified_common_dir(repo_root).map_err(|err| format!("no git directory for {}: {err}", repo_root.display()))?;
    let commit = git_line(repo_root, &["rev-parse", "--verify", &format!("{base}^{{commit}}")]).ok_or_else(|| format!("{base} names no commit"))?;
    let pending = Pending {
        job_id: job_id.to_string(),
        ticket_id: ticket_id.to_string(),
        branch: branch.map(str::to_string),
        commit,
        created_at: now(),
        deny: deny.to_vec(),
        env,
    };
    let dir = pending_dir(&common);
    fs::create_dir_all(&dir).map_err(|err| format!("creating {}: {err}", dir.display()))?;
    let path = dir.join(format!("{job_id}.json"));
    let text = serde_json::to_string_pretty(&pending).map_err(|err| err.to_string())?;
    fs::write(&path, text).map_err(|err| format!("writing {}: {err}", path.display()))?;
    Ok(true)
}

/// Remove job `job_id`'s record and say what became of it: the worktree
/// script's result, or that the script never claimed it.
pub fn finish(repo_root: &Path, job_id: &str) -> String {
    let Ok(common) = verified_common_dir(repo_root) else {
        return format!("The thread's {SETTINGS_FILE} is unknown: the repository's git directory is gone.");
    };
    let dir = pending_dir(&common);
    let result = fs::read_to_string(dir.join(format!("{job_id}.result"))).ok();
    let _ = fs::remove_file(dir.join(format!("{job_id}.result")));
    let unclaimed = fs::remove_file(dir.join(format!("{job_id}.json"))).is_ok();
    match result {
        Some(result) => result.trim().to_string(),
        None if unclaimed => "The thread started without the profile's tools.disallowed and env: T3 Code's worktree script for this project \
doesn't run `lotar agent worktree setup` (docs/help/agent.md#t3-code-threads)."
            .to_string(),
        None => format!("The thread's {SETTINGS_FILE} is unknown: the worktree script claimed it but left no result."),
    }
}

/// `lotar agent worktree setup`, run by T3 Code in the new worktree
/// `worktree`: claim the record of the job starting a thread there and write
/// its settings. `Ok(None)` when no job is starting one (the owner's own
/// threads, other repositories).
pub fn setup(worktree: &Path) -> Result<Option<String>, String> {
    let Ok(common) = verified_common_dir(worktree) else {
        return Ok(None);
    };
    let dir = pending_dir(&common);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(None);
    };
    let branch = git_line(worktree, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let head = git_line(worktree, &["rev-parse", "HEAD"]);
    let mut candidates: Vec<(PathBuf, Pending)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| Some((path.clone(), serde_json::from_str::<Pending>(&fs::read_to_string(&path).ok()?).ok()?)))
        .filter(|(_, pending)| now().saturating_sub(pending.created_at) <= PENDING_SECS)
        .filter(|(_, pending)| match &pending.branch {
            Some(wanted) => branch.as_deref() == Some(wanted.as_str()),
            None => head.as_deref() == Some(pending.commit.as_str()),
        })
        .collect();
    candidates.sort_by_key(|(_, pending)| pending.created_at);
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

    #[test]
    fn the_job_worktree_gets_the_profile_settings_and_others_keep_theirs() {
        let (_temp, repo, worktree) = repo_with_worktree("agent/DEV-1");
        fs::create_dir_all(worktree.join(".claude")).expect("claude dir");
        fs::write(worktree.join(SETTINGS_FILE), r#"{"model":"x","permissions":{"deny":["Workflow"],"allow":["Bash"]},"env":{"A":"old","B":"keep"}}"#)
            .expect("existing settings");
        let env = BTreeMap::from([("A".to_string(), "new".to_string())]);
        assert!(record(&repo, "job-1", "DEV-1", "main", Some("agent/DEV-1"), &deny(), env).expect("recorded"));

        let said = setup(&worktree).expect("setup").expect("claimed");
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

        assert!(finish(&repo, "job-1").starts_with("Wrote"));
        assert!(setup(&worktree).expect("setup").is_none(), "claimed once");
    }

    #[test]
    fn a_thread_from_an_existing_branch_matches_by_commit() {
        let (_temp, repo, worktree) = repo_with_worktree("t3/topic");
        assert!(record(&repo, "job-2", "DEV-2", "main", None, &deny(), BTreeMap::new()).expect("recorded"));
        assert!(setup(&worktree).expect("setup").is_some());
        assert!(worktree.join(SETTINGS_FILE).exists());
    }

    #[test]
    fn other_worktrees_and_profiles_without_settings_are_unchanged() {
        let (_temp, repo, worktree) = repo_with_worktree("t3/mine");
        assert!(!record(&repo, "job-3", "DEV-3", "main", None, &[], BTreeMap::new()).expect("nothing to record"));
        assert!(record(&repo, "job-4", "DEV-4", "main", Some("agent/DEV-4"), &deny(), BTreeMap::new()).expect("recorded"));
        assert!(setup(&worktree).expect("setup").is_none());
        assert!(!worktree.join(".claude").exists());
        assert!(finish(&repo, "job-4").contains("doesn't run `lotar agent worktree setup`"));
        let outside = tempfile::tempdir().expect("tempdir");
        assert!(setup(outside.path()).expect("no git").is_none());
    }

    #[test]
    fn a_file_that_cannot_be_merged_is_left_and_the_job_learns_why() {
        let (_temp, repo, worktree) = repo_with_worktree("agent/DEV-5");
        fs::create_dir_all(worktree.join(".claude")).expect("claude dir");
        fs::write(worktree.join(SETTINGS_FILE), "not json").expect("broken settings");
        assert!(record(&repo, "job-5", "DEV-5", "main", Some("agent/DEV-5"), &deny(), BTreeMap::new()).expect("recorded"));
        let err = setup(&worktree).expect_err("can't merge");
        assert!(err.contains("isn't valid JSON"), "{err}");
        assert_eq!(fs::read_to_string(worktree.join(SETTINGS_FILE)).expect("settings"), "not json");
        assert!(finish(&repo, "job-5").contains("isn't valid JSON"));
    }
}
