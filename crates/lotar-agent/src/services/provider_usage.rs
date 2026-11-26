//! Each provider's newest usage-limit reading (DEV-862): how much of a
//! window (Claude's five-hour or weekly limit) is used and when it resets,
//! as the agents of lotar's jobs report it. Kept machine-wide in
//! `usage.json` next to the agent queue, so every repository's server
//! writes the same file and scripts read it with `lotar usage`.
//!
//! Readings come from claude-agent-acp's `_claude/rateLimit` (harnessmux
//! `Usage.rate_limit`) and from a job paused on a usage limit, the only
//! reading Codex and T3 Code give today.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::config::types::AgentProfileDetail;

/// An unchanged reading rewrites the file at most this often: Claude sends
/// one with every usage update.
const UNCHANGED_REWRITE: Duration = Duration::from_secs(60);

/// Who a reading is for: the provider and its login.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Source {
    /// `claude`, `codex`, or the runner's name for the others (`t3` when a
    /// T3 Code profile names no provider).
    pub provider: String,
    /// The profile's `CLAUDE_CONFIG_DIR` or `CODEX_HOME`, `None` for the
    /// default login.
    pub account: Option<String>,
}

impl Source {
    /// The provider and account a job of `profile` runs on.
    pub fn of(profile: &AgentProfileDetail) -> Self {
        let runner = profile.runner.trim().to_ascii_lowercase();
        let named = if runner == "t3" { profile.provider.as_deref().map(str::to_ascii_lowercase) } else { None };
        let provider = match named.as_deref().unwrap_or(&runner) {
            name if name.contains("claude") => "claude".to_string(),
            name if name.contains("codex") => "codex".to_string(),
            name => name.to_string(),
        };
        let account_var = match provider.as_str() {
            "claude" => Some("CLAUDE_CONFIG_DIR"),
            "codex" => Some("CODEX_HOME"),
            _ => None,
        };
        let account = account_var.and_then(|var| profile.env.get(var)).map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
        Self { provider, account }
    }
}

/// A reading's provider, account and window.
type Key = (String, Option<String>, Option<String>);

/// One window's newest reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageReading {
    pub provider: String,
    pub account: Option<String>,
    /// The window as the agent names it (`five_hour`, `seven_day`, …);
    /// `None` when a paused job's text names none.
    pub window: Option<String>,
    /// `allowed`, `allowed_warning` or `rejected`.
    pub status: String,
    /// How much of the window is used, 0 to 100; `None` when the agent says
    /// only the status.
    pub used_percent: Option<f64>,
    /// RFC 3339 UTC time when the window resets.
    pub resets_at: Option<String>,
    /// RFC 3339 UTC time when lotar got the reading.
    pub read_at: String,
    /// The job whose agent reported it.
    pub job_id: Option<String>,
}

impl UsageReading {
    /// A job agent's report of a window (`_claude/rateLimit`).
    pub fn from_rate_limit(source: &Source, limit: &harnessmux::RateLimit, job_id: &str, now: Timestamp) -> Self {
        Self {
            provider: source.provider.clone(),
            account: source.account.clone(),
            window: limit.window.clone(),
            status: limit.status.clone(),
            used_percent: limit.utilization.map(|share| (share * 1000.0).round() / 10.0),
            resets_at: limit.resets_at.clone(),
            read_at: now.to_string(),
            job_id: Some(job_id.to_string()),
        }
    }

    /// A job paused on a usage limit: the window is used up until
    /// `resets_at`. The window is the one `line` names, if it names one.
    pub fn rejected(source: &Source, line: &str, resets_at: Option<Timestamp>, job_id: &str, now: Timestamp) -> Self {
        let lower = line.to_lowercase();
        let window = if lower.contains("5-hour") || lower.contains("five-hour") {
            Some("five_hour".to_string())
        } else if lower.contains("weekly") {
            Some("seven_day".to_string())
        } else {
            None
        };
        Self {
            provider: source.provider.clone(),
            account: source.account.clone(),
            window,
            status: "rejected".to_string(),
            used_percent: None,
            resets_at: resets_at.map(|at| at.to_string()),
            read_at: now.to_string(),
            job_id: Some(job_id.to_string()),
        }
    }

    fn key(&self) -> Key {
        (self.provider.clone(), self.account.clone(), self.window.clone())
    }

    /// The same reading but for when it was read and by which job.
    fn same_values(&self, other: &Self) -> bool {
        self.status == other.status && self.used_percent == other.used_percent && self.resets_at == other.resets_at
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UsageFile {
    #[serde(default)]
    readings: Vec<UsageReading>,
}

/// `usage.json` in lotar's machine-wide state directory, next to the agent
/// queue.
pub fn file_path() -> PathBuf {
    crate::services::agent_queue_service::queue_base_dir().join("usage.json")
}

/// The saved readings by provider, account and window; none when there is
/// no file yet.
pub fn readings() -> Result<Vec<UsageReading>, String> {
    readings_in(&file_path())
}

fn readings_in(path: &std::path::Path) -> Result<Vec<UsageReading>, String> {
    let mut readings = load(path)?.readings;
    readings.sort_by_key(UsageReading::key);
    Ok(readings)
}

fn load(path: &std::path::Path) -> Result<UsageFile, String> {
    match std::fs::read_to_string(path) {
        Ok(content) if content.trim().is_empty() => Ok(UsageFile::default()),
        Ok(content) => serde_json::from_str(&content).map_err(|err| format!("{} is not a usage file: {err}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(UsageFile::default()),
        Err(err) => Err(format!("Cannot read {}: {err}", path.display())),
    }
}

/// What this process wrote last per window, and when.
static WRITTEN: LazyLock<Mutex<HashMap<Key, (UsageReading, Instant)>>> = LazyLock::new(Default::default);
static WARNED: AtomicBool = AtomicBool::new(false);

/// Save `reading` as its window's newest. A failure warns once per process
/// and leaves the job alone.
pub fn record(reading: UsageReading) {
    let key = reading.key();
    if let Ok(written) = WRITTEN.lock()
        && let Some((last, at)) = written.get(&key)
        && last.same_values(&reading)
        && at.elapsed() < UNCHANGED_REWRITE
    {
        return;
    }
    match save(&file_path(), &reading) {
        Ok(()) => {
            if let Ok(mut written) = WRITTEN.lock() {
                written.insert(key, (reading, Instant::now()));
            }
        }
        Err(err) if !WARNED.swap(true, Ordering::Relaxed) => {
            crate::output::OutputRenderer::new(crate::output::OutputFormat::Text, crate::output::LogLevel::Warn)
                .log_warn(format_args!("Could not save the {} usage reading: {err}", reading.provider));
        }
        Err(_) => {}
    }
}

fn save(path: &std::path::Path, reading: &UsageReading) -> Result<(), String> {
    let dir = path.parent().map(std::path::Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&dir).map_err(|err| format!("Cannot create {}: {err}", dir.display()))?;
    let _lock = crate::storage::safety::acquire_storage_lock_within(&dir, "usage", Duration::from_secs(5)).map_err(|err| err.to_string())?;
    let mut file = load(path)?;
    let newer = |old: &UsageReading| match (old.read_at.parse::<Timestamp>(), reading.read_at.parse::<Timestamp>()) {
        (Ok(old), Ok(new)) => new >= old,
        _ => true,
    };
    match file.readings.iter_mut().find(|old| old.key() == reading.key()) {
        Some(old) if newer(old) => *old = reading.clone(),
        Some(_) => return Ok(()),
        None => file.readings.push(reading.clone()),
    }
    let json = serde_json::to_string_pretty(&file).map_err(|err| err.to_string())?;
    crate::storage::safety::atomic_write_file(path, &json).map_err(|err| format!("Cannot write {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(runner: &str, provider: Option<&str>, env: &[(&str, &str)]) -> AgentProfileDetail {
        AgentProfileDetail {
            runner: runner.to_string(),
            provider: provider.map(str::to_string),
            env: env.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect(),
            ..AgentProfileDetail::default()
        }
    }

    #[test]
    fn provider_and_account_follow_the_runner_and_profile() {
        let source = |runner, provider, env: &[(&str, &str)]| Source::of(&profile(runner, provider, env));
        assert_eq!(source("claude", None, &[]), Source { provider: "claude".into(), account: None });
        assert_eq!(source("codex", None, &[("CODEX_HOME", "/w/codex")]), Source { provider: "codex".into(), account: Some("/w/codex".into()) });
        assert_eq!(source("t3", Some("claudeAgent"), &[("CLAUDE_CONFIG_DIR", "/w/c")]).account.as_deref(), Some("/w/c"));
        assert_eq!(source("t3", Some("claudeAgent"), &[]).provider, "claude");
        assert_eq!(source("t3", Some("codex"), &[]).provider, "codex");
        assert_eq!(source("t3", None, &[]).provider, "t3");
        // Another provider's variable is not this one's account.
        assert_eq!(source("claude", None, &[("CODEX_HOME", "/w/codex")]).account, None);
        // A runner's provider setting applies only to t3.
        assert_eq!(source("codex", Some("claudeAgent"), &[]).provider, "codex");
    }

    #[test]
    fn readings_from_a_rate_limit_and_from_a_pause() {
        let source = Source { provider: "claude".into(), account: None };
        let now: Timestamp = "2026-11-25T12:00:00Z".parse().unwrap();
        let limit = harnessmux::RateLimit {
            status: "allowed_warning".into(),
            window: Some("seven_day".into()),
            resets_at: Some("2026-12-01T16:00:00.000Z".into()),
            utilization: Some(0.26),
        };
        let reading = UsageReading::from_rate_limit(&source, &limit, "job-1", now);
        assert_eq!(reading.used_percent, Some(26.0));
        assert_eq!(
            (reading.window.as_deref(), reading.status.as_str(), reading.read_at.as_str()),
            (Some("seven_day"), "allowed_warning", "2026-11-25T12:00:00Z")
        );
        let reset: Timestamp = "2026-11-25T17:00:00Z".parse().unwrap();
        let paused = UsageReading::rejected(&source, "You've hit your 5-hour limit · resets 6pm", Some(reset), "job-2", now);
        assert_eq!(
            (paused.window.as_deref(), paused.status.as_str(), paused.resets_at.as_deref()),
            (Some("five_hour"), "rejected", Some("2026-11-25T17:00:00Z"))
        );
        assert_eq!(UsageReading::rejected(&source, "You've hit your weekly limit", None, "j", now).window.as_deref(), Some("seven_day"));
        assert_eq!(UsageReading::rejected(&source, "You've hit your usage limit.", None, "j", now).window, None);
    }

    #[test]
    fn the_newest_reading_per_window_wins() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        let reading = |window: &str, status: &str, read_at: &str| UsageReading {
            provider: "claude".into(),
            account: None,
            window: Some(window.into()),
            status: status.into(),
            used_percent: None,
            resets_at: None,
            read_at: read_at.into(),
            job_id: None,
        };
        save(&path, &reading("five_hour", "allowed", "2026-11-25T12:00:00Z")).unwrap();
        save(&path, &reading("seven_day", "allowed_warning", "2026-11-25T12:00:00Z")).unwrap();
        save(&path, &reading("five_hour", "rejected", "2026-11-25T13:00:00Z")).unwrap();
        // An older reading (another server's, late) doesn't replace a newer one.
        save(&path, &reading("five_hour", "allowed", "2026-11-25T12:30:00Z")).unwrap();
        let saved = readings_in(&path).unwrap();
        let statuses: Vec<_> = saved.iter().map(|r| (r.window.clone().unwrap_or_default(), r.status.clone())).collect();
        assert_eq!(statuses, [("five_hour".to_string(), "rejected".to_string()), ("seven_day".to_string(), "allowed_warning".to_string())]);
        std::fs::write(&path, "not json").unwrap();
        assert!(readings_in(&path).is_err_and(|err| err.contains("usage.json")));
    }
}
