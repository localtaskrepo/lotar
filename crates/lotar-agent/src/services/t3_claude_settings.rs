//! A t3 job's Claude settings (DEV-618). T3's CLI takes no Claude settings,
//! and T3's Claude sessions read the worktree's `.claude/settings.local.json`,
//! so before a new thread starts in the ticket's worktree (DEV-652) the job
//! writes the profile's `tools.disallowed` and `env` there.

use crate::utils::git::{git_command, verified_common_dir};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const SETTINGS_FILE: &str = ".claude/settings.local.json";

/// Write `deny` and `env` into `worktree`'s Claude settings. The note for
/// the job log; `None` when there is nothing to write.
pub fn write(worktree: &Path, deny: &[String], env: &BTreeMap<String, String>) -> Option<String> {
    if deny.is_empty() && env.is_empty() {
        return None;
    }
    Some(match write_settings(worktree, deny, env) {
        Ok(()) => format!(
            "Wrote {SETTINGS_FILE} in {} for the thread ({} deny rules, {} env keys), before its agent started.",
            worktree.display(),
            deny.len(),
            env.len()
        ),
        Err(err) => format!("The thread started without the profile's tools.disallowed and env: {err}"),
    })
}

fn write_settings(worktree: &Path, deny: &[String], env: &BTreeMap<String, String>) -> Result<(), String> {
    let common_dir = verified_common_dir(worktree).map_err(|err| format!("no git directory for {}: {err}", worktree.display()))?;
    if git_command(worktree).args(["ls-files", "--error-unmatch", SETTINGS_FILE]).output().is_ok_and(|output| output.status.success()) {
        return Err(format!("{SETTINGS_FILE} is tracked in git, so it isn't changed"));
    }
    keep_out_of_commits(worktree, &common_dir)?;
    let path = worktree.join(SETTINGS_FILE);
    let mut settings = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<serde_json::Value>(&text).map_err(|err| format!("{} isn't valid JSON: {err}", path.display()))?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(err) => return Err(format!("reading {}: {err}", path.display())),
    };
    merge(&mut settings, deny, env).map_err(|err| format!("{}: {err}", path.display()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("creating {}: {err}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&settings).map_err(|err| err.to_string())?;
    fs::write(&path, format!("{text}\n")).map_err(|err| format!("writing {}: {err}", path.display()))
}

/// Add the deny rules the file lacks; the profile's env keys win.
fn merge(settings: &mut serde_json::Value, deny: &[String], env: &BTreeMap<String, String>) -> Result<(), String> {
    let root = settings.as_object_mut().ok_or("the file holds no JSON object")?;
    if !deny.is_empty() {
        let permissions = root.entry("permissions").or_insert_with(|| serde_json::json!({})).as_object_mut().ok_or("permissions isn't an object")?;
        let existing = permissions.entry("deny").or_insert_with(|| serde_json::json!([])).as_array_mut().ok_or("permissions.deny isn't a list")?;
        for rule in deny {
            if !existing.iter().any(|known| known.as_str() == Some(rule)) {
                existing.push(serde_json::Value::String(rule.clone()));
            }
        }
    }
    if !env.is_empty() {
        let vars = root.entry("env").or_insert_with(|| serde_json::json!({})).as_object_mut().ok_or("env isn't an object")?;
        for (key, value) in env {
            vars.insert(key.clone(), serde_json::Value::String(value.clone()));
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
    use std::path::PathBuf;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git").args(args).current_dir(dir).output().expect("git runs");
        assert!(status.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&status.stderr));
    }

    /// A repository with one commit and a linked worktree on `branch`.
    fn repo_with_worktree(branch: &str) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).expect("repo dir");
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "init"]);
        let worktree = temp.path().join("wt");
        git(&repo, &["worktree", "add", "-q", "-b", branch, worktree.to_str().expect("utf-8 path")]);
        let worktree = fs::canonicalize(worktree).expect("canonical worktree");
        (temp, worktree)
    }

    fn deny() -> Vec<String> {
        vec!["Artifact".to_string(), "Workflow".to_string()]
    }

    #[test]
    fn the_worktree_gets_the_profile_settings_and_keeps_its_own() {
        let (_temp, worktree) = repo_with_worktree("agent/DEV-1");
        fs::create_dir_all(worktree.join(".claude")).expect("claude dir");
        fs::write(worktree.join(SETTINGS_FILE), r#"{"model":"x","permissions":{"deny":["Workflow"],"allow":["Bash"]},"env":{"A":"old","B":"keep"}}"#)
            .expect("existing settings");
        let env = BTreeMap::from([("A".to_string(), "new".to_string())]);

        let said = write(&worktree, &deny(), &env).expect("a note");
        assert!(said.starts_with("Wrote"), "{said}");
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
    }

    #[test]
    fn a_profile_without_settings_writes_nothing() {
        let (_temp, worktree) = repo_with_worktree("agent/DEV-3");
        assert!(write(&worktree, &[], &BTreeMap::new()).is_none());
        assert!(!worktree.join(".claude").exists());
    }

    #[test]
    fn a_file_that_cannot_be_merged_is_left_and_the_job_learns_why() {
        let (_temp, worktree) = repo_with_worktree("agent/DEV-5");
        fs::create_dir_all(worktree.join(".claude")).expect("claude dir");
        fs::write(worktree.join(SETTINGS_FILE), "not json").expect("broken settings");
        let said = write(&worktree, &deny(), &BTreeMap::new()).expect("a note");
        assert!(said.contains("without the profile's tools.disallowed and env") && said.contains("isn't valid JSON"), "{said}");
        assert_eq!(fs::read_to_string(worktree.join(SETTINGS_FILE)).expect("settings"), "not json");
    }
}
