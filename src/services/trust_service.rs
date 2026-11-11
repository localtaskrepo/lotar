//! Workspace trust for commands that come from repository files (DEV-259).
//!
//! Automation rules (`.tasks/automation.yml`, `.tasks/<PROJECT>/automation.yml`)
//! and agent profiles (`agents:` in `.tasks/config.yml` and project configs)
//! travel with the repository, and both can run programs. Like `direnv allow`,
//! LoTaR only runs them after the user trusted the current contents: a
//! fingerprint of those sources is recorded per tasks directory in a trust
//! file in the home directory, and any change (a pull, a PR, a fresh clone)
//! makes the workspace untrusted until `lotar trust` records it again.
//!
//! Writes LoTaR makes itself (UI, REST, CLI and MCP config or automation
//! edits) keep a trusted workspace trusted, see [`TrustService::rewrite`];
//! they cannot turn an untrusted workspace into a trusted one.

use crate::errors::{LoTaRError, LoTaRResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Set to `1` to trust every workspace (CI, containers, test harnesses).
pub const TRUST_ALL_ENV: &str = "LOTAR_TRUST_ALL";
/// Override the trust file location.
pub const TRUST_FILE_ENV: &str = "LOTAR_TRUST_FILE";

#[derive(Debug, Default, Serialize, Deserialize)]
struct TrustFile {
    #[serde(default)]
    workspaces: BTreeMap<String, TrustRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrustRecord {
    fingerprint: String,
    trusted_at: String,
}

/// One repository file that can make LoTaR run commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSource {
    /// Path relative to the tasks directory, e.g. `automation.yml`.
    pub path: String,
    /// What it contributes: `automation` or `agents`.
    pub kind: &'static str,
    content: String,
}

/// Trust state of a tasks directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustState {
    /// Nothing in the repository can run commands.
    NothingToTrust,
    /// The recorded fingerprint matches the current sources.
    Trusted,
    /// Trusted earlier, but the sources changed since.
    Changed,
    /// Never trusted.
    Untrusted,
}

impl TrustState {
    pub fn allows_commands(self) -> bool {
        matches!(self, TrustState::NothingToTrust | TrustState::Trusted)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TrustState::NothingToTrust => "nothing-to-trust",
            TrustState::Trusted => "trusted",
            TrustState::Changed => "changed",
            TrustState::Untrusted => "untrusted",
        }
    }
}

pub struct TrustService;

impl TrustService {
    /// Repository files that can run commands, in a stable order.
    pub fn sources(tasks_dir: &Path) -> Vec<CommandSource> {
        let mut out = Vec::new();
        let mut push_dir = |dir: &Path, rel_prefix: &str| {
            let automation = dir.join("automation.yml");
            if let Ok(content) = std::fs::read_to_string(&automation) {
                out.push(CommandSource { path: format!("{rel_prefix}automation.yml"), kind: "automation", content });
            }
            if let Some(agents) = agents_block(&dir.join("config.yml")) {
                out.push(CommandSource { path: format!("{rel_prefix}config.yml"), kind: "agents", content: agents });
            }
        };
        push_dir(tasks_dir, "");
        let mut projects = crate::utils::filesystem::list_visible_subdirs(tasks_dir);
        projects.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, dir) in projects {
            push_dir(&dir, &format!("{name}/"));
        }
        out
    }

    /// Fingerprint of the given sources; `None` when there are none.
    fn fingerprint(sources: &[CommandSource]) -> Option<String> {
        if sources.is_empty() {
            return None;
        }
        let mut hasher = blake3::Hasher::new();
        for source in sources {
            for part in [source.path.as_str(), source.kind, source.content.as_str()] {
                hasher.update(&(part.len() as u64).to_le_bytes());
                hasher.update(part.as_bytes());
            }
        }
        Some(hasher.finalize().to_hex().to_string())
    }

    pub fn state(tasks_dir: &Path) -> TrustState {
        let Some(fingerprint) = Self::fingerprint(&Self::sources(tasks_dir)) else {
            return TrustState::NothingToTrust;
        };
        if trust_all() {
            return TrustState::Trusted;
        }
        match load_trust_file().ok().and_then(|file| file.workspaces.get(&workspace_key(tasks_dir)).cloned()) {
            Some(record) if record.fingerprint == fingerprint => TrustState::Trusted,
            Some(_) => TrustState::Changed,
            None => TrustState::Untrusted,
        }
    }

    pub fn allows_commands(tasks_dir: &Path) -> bool {
        Self::state(tasks_dir).allows_commands()
    }

    /// Record the current sources as trusted. Returns false when there was
    /// nothing to trust.
    pub fn trust(tasks_dir: &Path) -> LoTaRResult<bool> {
        let Some(fingerprint) = Self::fingerprint(&Self::sources(tasks_dir)) else {
            return Ok(false);
        };
        update_trust_file(|file| {
            file.workspaces.insert(workspace_key(tasks_dir), TrustRecord { fingerprint, trusted_at: chrono::Utc::now().to_rfc3339() });
        })?;
        Ok(true)
    }

    /// Forget the trust record. Returns whether one existed.
    pub fn revoke(tasks_dir: &Path) -> LoTaRResult<bool> {
        let key = workspace_key(tasks_dir);
        let mut removed = false;
        update_trust_file(|file| removed = file.workspaces.remove(&key).is_some())?;
        Ok(removed)
    }

    /// Run a LoTaR write to a command source, keeping a trusted workspace
    /// trusted afterwards. An untrusted workspace stays untrusted.
    pub fn rewrite<T, E>(tasks_dir: &Path, write: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
        let before = (!trust_all() && Self::allows_commands(tasks_dir)).then(|| Self::fingerprint(&Self::sources(tasks_dir)));
        let result = write()?;
        let Some(before) = before else {
            return Ok(result);
        };
        if Self::fingerprint(&Self::sources(tasks_dir)) != before
            && let Err(err) = Self::trust(tasks_dir)
        {
            crate::output::OutputRenderer::new(crate::output::OutputFormat::Text, crate::output::LogLevel::Warn)
                .log_warn(format_args!("Could not update the trust record: {err}"));
        }
        Ok(result)
    }

    /// Whether an agent profile name is defined by a repository config file.
    pub fn profile_from_repository(tasks_dir: &Path, profile: &str) -> bool {
        Self::sources(tasks_dir).iter().filter(|source| source.kind == "agents").any(|source| {
            serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&source.content)
                .ok()
                .and_then(|value| value.as_mapping().cloned())
                .is_some_and(|map| map.contains_key(profile))
        })
    }

    /// The message shown when a command from the repository is held back.
    pub fn untrusted_message(tasks_dir: &Path, what: &str) -> String {
        let reason = match Self::state(tasks_dir) {
            TrustState::Changed => "have changed since you last trusted them",
            _ => "are not trusted yet",
        };
        format!(
            "Skipped {what}: this repository's automation rules and agent profiles {reason}. Review them with `lotar trust --status`, then run `lotar trust` (or set {TRUST_ALL_ENV}=1 in CI)."
        )
    }

    /// Lines describing what trusting would allow: every `run` action and
    /// every repository agent profile.
    pub fn describe(tasks_dir: &Path) -> Vec<String> {
        let mut lines = Vec::new();
        for source in Self::sources(tasks_dir) {
            match source.kind {
                "automation" => {
                    let runs = run_commands(&source.content);
                    if runs.is_empty() {
                        lines.push(format!("{}: no run commands", source.path));
                    }
                    for (rule, event, command) in runs {
                        lines.push(format!("{}: rule '{rule}' on {event} runs {command}", source.path));
                    }
                }
                _ => {
                    let Ok(serde_yaml_ng::Value::Mapping(map)) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&source.content) else {
                        continue;
                    };
                    for (name, profile) in map {
                        let name = name.as_str().unwrap_or_default().to_string();
                        let detail = serde_yaml_ng::to_string(&profile).unwrap_or_default().trim().replace('\n', ", ");
                        lines.push(format!("{}: agent profile '{name}' ({detail})", source.path));
                    }
                }
            }
        }
        lines
    }
}

fn trust_all() -> bool {
    std::env::var(TRUST_ALL_ENV).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

fn workspace_key(tasks_dir: &Path) -> String {
    tasks_dir.canonicalize().unwrap_or_else(|_| tasks_dir.to_path_buf()).to_string_lossy().into_owned()
}

/// `agents:` of a config file, re-serialized so unrelated settings in the
/// same file do not affect the fingerprint.
fn agents_block(config_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(config_path).ok()?;
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&content).ok()?;
    let agents = value.get("agents")?;
    if agents.is_null() || agents.as_mapping().is_some_and(|m| m.is_empty()) {
        return None;
    }
    serde_yaml_ng::to_string(agents).ok()
}

/// (rule name, event, command) for every `run` in an automation file.
fn run_commands(content: &str) -> Vec<(String, String, String)> {
    let Ok(file) = serde_yaml_ng::from_str::<crate::automation::types::AutomationFile>(content) else {
        return vec![("?".to_string(), "?".to_string(), "(file does not parse)".to_string())];
    };
    let mut out = Vec::new();
    for (index, rule) in file.automation.rules().iter().enumerate() {
        let name = rule.name.clone().unwrap_or_else(|| format!("#{}", index + 1));
        let value = serde_yaml_ng::to_value(&rule.on).unwrap_or_default();
        let Some(map) = value.as_mapping() else {
            continue;
        };
        for (event, action) in map {
            if let Some(run) = action.get("run") {
                let command = match run {
                    serde_yaml_ng::Value::String(raw) => raw.clone(),
                    other => serde_json::to_string(other).unwrap_or_default(),
                };
                out.push((name.clone(), event.as_str().unwrap_or_default().to_string(), command));
            }
        }
    }
    out
}

fn trust_file_path() -> LoTaRResult<PathBuf> {
    if let Ok(path) = std::env::var(TRUST_FILE_ENV)
        && !path.trim().is_empty()
    {
        return Ok(PathBuf::from(path));
    }
    let home = dirs::home_dir().ok_or_else(|| LoTaRError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found")))?;
    let legacy_dir = home.join(".lotar");
    if legacy_dir.is_dir() {
        return Ok(legacy_dir.join("trust.yml"));
    }
    Ok(home.join(".lotar.trust.yml"))
}

fn load_trust_file() -> LoTaRResult<TrustFile> {
    let path = trust_file_path()?;
    match std::fs::read_to_string(&path) {
        Ok(content) if content.trim().is_empty() => Ok(TrustFile::default()),
        Ok(content) => serde_yaml_ng::from_str(&content).map_err(LoTaRError::from),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(TrustFile::default()),
        Err(err) => Err(LoTaRError::IoError(err)),
    }
}

fn update_trust_file(apply: impl FnOnce(&mut TrustFile)) -> LoTaRResult<()> {
    let path = trust_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = load_trust_file()?;
    apply(&mut file);
    let yaml = serde_yaml_ng::to_string(&file)?;
    crate::storage::safety::atomic_write_file(&path, &yaml)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_covers_automation_and_agents_only() {
        let dir = tempfile::tempdir().unwrap();
        let tasks = dir.path();
        assert!(TrustService::sources(tasks).is_empty());

        std::fs::write(tasks.join("config.yml"), "default:\n  project: A\n").unwrap();
        assert!(TrustService::sources(tasks).is_empty(), "no agents block");

        std::fs::write(tasks.join("config.yml"), "default:\n  project: A\nagents:\n  bot:\n    runner: command\n    command: sh\n").unwrap();
        std::fs::create_dir_all(tasks.join("A")).unwrap();
        std::fs::write(tasks.join("A").join("automation.yml"), "automation:\n  rules:\n    - name: hi\n      on:\n        created:\n          run: echo hi\n")
            .unwrap();
        let sources = TrustService::sources(tasks);
        let paths: Vec<_> = sources.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, vec!["config.yml", "A/automation.yml"]);
        let before = TrustService::fingerprint(&sources);

        // Unrelated config edits keep the fingerprint.
        std::fs::write(tasks.join("config.yml"), "default:\n  project: B\nagents:\n  bot:\n    runner: command\n    command: sh\n").unwrap();
        assert_eq!(TrustService::fingerprint(&TrustService::sources(tasks)), before);
        assert!(TrustService::profile_from_repository(tasks, "bot"));
        assert!(!TrustService::profile_from_repository(tasks, "other"));

        let described = TrustService::describe(tasks).join("\n");
        assert!(described.contains("rule 'hi' on created runs echo hi"), "{described}");
        assert!(described.contains("agent profile 'bot'"), "{described}");
    }
}
