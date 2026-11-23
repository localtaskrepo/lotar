//! Workspace trust for commands that come from repository files (DEV-259).
//!
//! Automation rules (`.tasks/automation.yml`, `.tasks/<PROJECT>/automation.yml`),
//! agent profiles (`agents:` in `.tasks/config.yml` and project configs),
//! the merge's checks (`agent.merge`, DEV-605) and the command for waiting
//! tickets (`agent.worktree.waiting_run`, DEV-712) travel with the repository,
//! and all can run programs. Like `direnv allow`,
//! LoTaR only runs them after the user trusted the current contents: a
//! fingerprint of those sources is recorded per tasks directory in a trust
//! file in the home directory, and any change (a pull, a PR, a fresh clone)
//! makes the workspace untrusted until `lotar trust` records it again.
//!
//! The record lists the source kinds the LoTaR that wrote it fingerprinted
//! (DEV-716), so a newer LoTaR that adds a kind checks only those and a
//! trusted repository stays trusted across the upgrade.
//!
//! A rule's `run` skipped while the workspace was untrusted is kept with
//! the trust record (DEV-717), so `lotar trust` can run it afterwards.
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
    /// Rule runs skipped while untrusted, per workspace key (DEV-717).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    skipped_runs: BTreeMap<String, Vec<SkippedRun>>,
}

/// A rule's `run` skipped in an untrusted workspace (DEV-717): the command
/// as it would have run then, with the ticket's values of that moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedRun {
    pub ticket: String,
    pub rule: String,
    pub skipped_at: String,
    /// The rule's `run` as written (JSON), to tell whether it changed since.
    pub run: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub cwd: PathBuf,
    pub wait: bool,
    #[serde(default)]
    pub ignore_failure: bool,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrustRecord {
    fingerprint: String,
    trusted_at: String,
    /// The kinds `fingerprint` covers. Records written before DEV-716 have
    /// none and cover [`LEGACY_KINDS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kinds: Option<Vec<String>>,
}

impl TrustRecord {
    fn covers(&self, kind: &str) -> bool {
        match &self.kinds {
            Some(kinds) => kinds.iter().any(|known| known == kind),
            None => LEGACY_KINDS.contains(&kind),
        }
    }

    /// Compares the sources of the kinds this record covers. Sources of a
    /// kind the record's LoTaR didn't know don't count: that LoTaR never ran
    /// them (DEV-716).
    fn state(&self, sources: &[CommandSource]) -> TrustState {
        let known = TrustService::fingerprint(sources.iter().filter(|source| self.covers(source.kind)));
        if known.as_deref() == Some(self.fingerprint.as_str()) { TrustState::Trusted } else { TrustState::Changed }
    }
}

/// The config blocks that can run commands, in fingerprint order. A new
/// kind goes here; records written by older builds don't list it, so it
/// doesn't make their repositories `changed`.
type BlockFn = fn(&serde_yaml_ng::Value) -> Option<String>;
const CONFIG_BLOCKS: &[(&str, BlockFn)] = &[("agents", agents_block), ("merge", merge_block), ("waiting_run", waiting_run_block)];

/// Every kind of [`CommandSource`] this build fingerprints: automation files
/// and [`CONFIG_BLOCKS`].
fn kinds() -> impl Iterator<Item = &'static str> {
    std::iter::once("automation").chain(CONFIG_BLOCKS.iter().map(|(kind, _)| *kind))
}
/// The kinds a record without `kinds:` covers: 0.15.4 and earlier, before
/// DEV-716 (and DEV-712's `waiting_run`).
const LEGACY_KINDS: &[&str] = &["automation", "agents", "merge"];

/// One repository file that can make LoTaR run commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSource {
    /// Path relative to the tasks directory, e.g. `automation.yml`.
    pub path: String,
    /// What it contributes: `automation`, `agents`, `merge` or `waiting_run`.
    pub kind: &'static str,
    content: String,
}

impl CommandSource {
    /// The file's command part: the whole automation file, the `agents`
    /// block, the `agent.merge` block or `agent.worktree.waiting_run`.
    pub fn content(&self) -> &str {
        &self.content
    }
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
            let config =
                std::fs::read_to_string(dir.join("config.yml")).ok().and_then(|content| serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&content).ok());
            for (kind, block) in CONFIG_BLOCKS {
                if let Some(content) = config.as_ref().and_then(block) {
                    out.push(CommandSource { path: format!("{rel_prefix}config.yml"), kind, content });
                }
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
    fn fingerprint<'a>(sources: impl IntoIterator<Item = &'a CommandSource>) -> Option<String> {
        let mut sources = sources.into_iter().peekable();
        sources.peek()?;
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
        let sources = Self::sources(tasks_dir);
        if sources.is_empty() {
            return TrustState::NothingToTrust;
        }
        if trust_all() {
            return TrustState::Trusted;
        }
        let Some(record) = load_trust_file().ok().and_then(|file| file.workspaces.get(&workspace_key(tasks_dir)).cloned()) else {
            return TrustState::Untrusted;
        };
        record.state(&sources)
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
        let kinds = Some(kinds().map(str::to_string).collect());
        update_trust_file(|file| {
            file.workspaces.insert(workspace_key(tasks_dir), TrustRecord { fingerprint, trusted_at: crate::utils::time::now_rfc3339(), kinds });
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

    /// Keep a rule's `run` skipped while untrusted, for `lotar trust`.
    pub fn record_skipped_run(tasks_dir: &Path, run: SkippedRun) -> LoTaRResult<()> {
        let key = workspace_key(tasks_dir);
        update_trust_file(|file| file.skipped_runs.entry(key).or_default().push(run))
    }

    /// The runs skipped in this workspace, oldest first, removed from the
    /// trust file.
    pub fn take_skipped_runs(tasks_dir: &Path) -> LoTaRResult<Vec<SkippedRun>> {
        let key = workspace_key(tasks_dir);
        if !load_trust_file()?.skipped_runs.contains_key(&key) {
            return Ok(Vec::new());
        }
        let mut taken = Vec::new();
        update_trust_file(|file| taken = file.skipped_runs.remove(&key).unwrap_or_default())?;
        Ok(taken)
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
}

fn trust_all() -> bool {
    std::env::var(TRUST_ALL_ENV).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

fn workspace_key(tasks_dir: &Path) -> String {
    tasks_dir.canonicalize().unwrap_or_else(|_| tasks_dir.to_path_buf()).to_string_lossy().into_owned()
}

/// `agents:` of a config file, re-serialized so unrelated settings in the
/// same file do not affect the fingerprint.
fn agents_block(config: &serde_yaml_ng::Value) -> Option<String> {
    non_empty_block(config.get("agents")?)
}

/// `agent.merge` of a config file (also written as a dotted key), like
/// [`agents_block`].
fn merge_block(config: &serde_yaml_ng::Value) -> Option<String> {
    non_empty_block(config.get("agent").and_then(|agent| agent.get("merge")).or_else(|| config.get("agent.merge"))?)
}

/// `agent.worktree.waiting_run` of a config file, in any of its dotted
/// spellings, like [`agents_block`].
fn waiting_run_block(config: &serde_yaml_ng::Value) -> Option<String> {
    let block = config
        .get("agent")
        .and_then(|agent| agent.get("worktree").and_then(|worktree| worktree.get("waiting_run")).or_else(|| agent.get("worktree.waiting_run")))
        .or_else(|| config.get("agent.worktree").and_then(|worktree| worktree.get("waiting_run")))
        .or_else(|| config.get("agent.worktree.waiting_run"))?;
    non_empty_block(block)
}

fn non_empty_block(block: &serde_yaml_ng::Value) -> Option<String> {
    if block.is_null() || block.as_mapping().is_some_and(|m| m.is_empty()) || block.as_sequence().is_some_and(|s| s.is_empty()) {
        return None;
    }
    serde_yaml_ng::to_string(block).ok()
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
    // Serve, the CLI and agent jobs all write this file: without the lock a
    // concurrent update loses the other's skipped run (DEV-821).
    let lock_name = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("trust").trim_start_matches('.').to_string();
    let _lock = crate::storage::safety::acquire_storage_lock_within(
        path.parent().unwrap_or(Path::new(".")),
        &lock_name,
        crate::utils::time::env_millis(crate::storage::safety::STORAGE_LOCK_WAIT_ENV, std::time::Duration::from_secs(10)),
    )?;
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
    }

    #[test]
    fn fingerprint_covers_the_merge_checks() {
        let dir = tempfile::tempdir().unwrap();
        let tasks = dir.path();
        std::fs::write(tasks.join("config.yml"), "agent:\n  worktree:\n    enabled: true\n").unwrap();
        assert!(TrustService::sources(tasks).is_empty(), "no agent.merge block");

        std::fs::write(tasks.join("config.yml"), "agent:\n  merge:\n    checks: [npm test]\n").unwrap();
        let sources = TrustService::sources(tasks);
        assert_eq!(sources.iter().map(|s| (s.path.as_str(), s.kind)).collect::<Vec<_>>(), vec![("config.yml", "merge")]);
        let before = TrustService::fingerprint(&sources);

        std::fs::write(tasks.join("config.yml"), "agent:\n  merge:\n    checks: [rm -rf build]\n").unwrap();
        assert_ne!(TrustService::fingerprint(&TrustService::sources(tasks)), before, "a changed check needs trust again");
        std::fs::write(tasks.join("config.yml"), "agent.merge:\n  checks: [npm test]\n").unwrap();
        assert_eq!(TrustService::sources(tasks).len(), 1, "the dotted spelling counts too");
    }

    #[test]
    fn fingerprint_covers_the_waiting_run_command() {
        let dir = tempfile::tempdir().unwrap();
        let tasks = dir.path();
        std::fs::write(tasks.join("config.yml"), "agent:\n  worktree:\n    waiting_run: []\n").unwrap();
        assert!(TrustService::sources(tasks).is_empty(), "an empty command runs nothing");

        std::fs::write(tasks.join("config.yml"), "agent:\n  worktree:\n    waiting_run: [cargo, clean]\n").unwrap();
        let sources = TrustService::sources(tasks);
        assert_eq!(sources.iter().map(|s| (s.path.as_str(), s.kind)).collect::<Vec<_>>(), vec![("config.yml", "waiting_run")]);
        let before = TrustService::fingerprint(&sources);

        std::fs::write(tasks.join("config.yml"), "agent:\n  worktree:\n    waiting_run: [rm, -rf, src]\n").unwrap();
        assert_ne!(TrustService::fingerprint(&TrustService::sources(tasks)), before, "a changed command needs trust again");
        for dotted in [
            "agent.worktree:\n  waiting_run: [cargo, clean]\n",
            "agent.worktree.waiting_run: [cargo, clean]\n",
            "agent:\n  worktree.waiting_run: [cargo, clean]\n",
        ] {
            std::fs::write(tasks.join("config.yml"), dotted).unwrap();
            assert_eq!(TrustService::fingerprint(&TrustService::sources(tasks)), before, "{dotted}");
        }
    }

    fn record(sources: &[CommandSource], kinds: Option<&[&str]>) -> TrustRecord {
        TrustRecord {
            fingerprint: TrustService::fingerprint(sources).unwrap(),
            trusted_at: String::new(),
            kinds: kinds.map(|kinds| kinds.iter().map(|kind| (*kind).to_string()).collect()),
        }
    }

    fn source(path: &str, kind: &'static str, content: &str) -> CommandSource {
        CommandSource { path: path.to_string(), kind, content: content.to_string() }
    }

    #[test]
    fn every_source_kind_is_listed() {
        let dir = tempfile::tempdir().unwrap();
        let tasks = dir.path();
        std::fs::write(
            tasks.join("config.yml"),
            "agents:\n  bot:\n    runner: command\n    command: sh\nagent:\n  merge:\n    checks: [npm test]\n  worktree:\n    waiting_run: [cargo, clean]\n",
        )
        .unwrap();
        std::fs::write(tasks.join("automation.yml"), "automation:\n  rules: []\n").unwrap();
        let sources = TrustService::sources(tasks);
        assert_eq!(sources.iter().map(|source| source.kind).collect::<Vec<_>>(), kinds().collect::<Vec<_>>(), "a test config with every kind");

        // What `lotar trust` records is trusted right away.
        let fresh = record(&sources, Some(&kinds().collect::<Vec<_>>()));
        assert_eq!(fresh.state(&sources), TrustState::Trusted);
        // A 0.15.4 record (no `kinds:`, no `waiting_run`) stays trusted.
        let legacy_sources: Vec<_> = sources.iter().filter(|source| LEGACY_KINDS.contains(&source.kind)).cloned().collect();
        assert_eq!(record(&legacy_sources, None).state(&sources), TrustState::Trusted, "DEV-712's waiting_run doesn't untrust it");
    }

    #[test]
    fn a_kind_added_by_a_newer_build_keeps_the_trust() {
        let automation = source("automation.yml", "automation", "automation: {}\n");
        let merge = source("config.yml", "merge", "checks:\n- npm test\n");
        // A build that knew only automation and agents trusted the repository,
        // which already had `agent.merge`.
        let trusted = record(std::slice::from_ref(&automation), Some(&["automation", "agents"]));
        assert_eq!(trusted.state(&[automation.clone(), merge.clone()]), TrustState::Trusted, "the newly fingerprinted kind doesn't untrust it");

        let edited = source("automation.yml", "automation", "automation: {rules: []}\n");
        assert_eq!(trusted.state(&[edited.clone(), merge.clone()]), TrustState::Changed, "a kind the record covers still counts");

        // A record of this build covers merge too.
        let current = record(&[automation.clone(), merge.clone()], Some(&kinds().collect::<Vec<_>>()));
        let other_merge = source("config.yml", "merge", "checks:\n- rm -rf build\n");
        assert_eq!(current.state(&[automation.clone(), other_merge]), TrustState::Changed);
        assert_eq!(current.state(std::slice::from_ref(&automation)), TrustState::Changed, "a removed source counts");
    }

    #[test]
    fn a_record_without_kinds_covers_the_kinds_before_dev716() {
        let automation = source("automation.yml", "automation", "automation: {}\n");
        let agents = source("config.yml", "agents", "bot:\n  runner: command\n");
        let merge = source("config.yml", "merge", "checks:\n- npm test\n");
        let added = source("config.yml", "later_kind", "- cargo\n- clean\n");
        let all = [automation.clone(), agents.clone(), merge.clone()];
        let legacy = record(&all, None);
        assert_eq!(legacy.state(&all), TrustState::Trusted, "this build reads the old record");
        assert_eq!(legacy.state(&[automation.clone(), agents.clone(), merge.clone(), added]), TrustState::Trusted, "nor does a later kind untrust it");
        let other_merge = source("config.yml", "merge", "checks:\n- rm -rf build\n");
        assert_eq!(legacy.state(&[automation, agents, other_merge]), TrustState::Changed);
    }

    #[test]
    fn a_kind_a_newer_build_drops_needs_trust_again() {
        let automation = source("automation.yml", "automation", "automation: {}\n");
        let gone = source("config.yml", "gone_kind", "x\n");
        let trusted = record(&[automation.clone(), gone], Some(&["automation", "gone_kind"]));
        assert_eq!(trusted.state(&[automation]), TrustState::Changed);
    }

    /// DEV-821: skips written by several processes at once, with `lotar
    /// trust` taking them in between, are each taken exactly once.
    #[test]
    fn concurrent_skips_and_a_trust_lose_no_skipped_run() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: test-local override; nextest runs each test in its own process.
        unsafe {
            std::env::remove_var(TRUST_ALL_ENV);
            std::env::set_var(TRUST_FILE_ENV, dir.path().join("trust.yml"));
        }
        let tasks = dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        std::fs::write(tasks.join("automation.yml"), "automation:\n  rules: []\n").unwrap();
        let skip = |ticket: String| SkippedRun {
            ticket,
            rule: "relay".to_string(),
            skipped_at: crate::utils::time::now_rfc3339(),
            run: "\"echo\"".to_string(),
            program: "echo".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: tasks.clone(),
            wait: true,
            ignore_failure: false,
            timeout_secs: 1,
        };
        let taken = std::sync::Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for writer in 0..8 {
                let skip = &skip;
                let tasks = &tasks;
                scope.spawn(move || {
                    for n in 0..10 {
                        TrustService::record_skipped_run(tasks, skip(format!("T-{writer}-{n}"))).unwrap();
                    }
                });
            }
            scope.spawn(|| {
                for _ in 0..5 {
                    assert!(TrustService::trust(&tasks).unwrap());
                    taken.lock().unwrap().extend(TrustService::take_skipped_runs(&tasks).unwrap());
                }
            });
        });
        let mut tickets: Vec<String> =
            taken.into_inner().unwrap().into_iter().chain(TrustService::take_skipped_runs(&tasks).unwrap()).map(|run| run.ticket).collect();
        tickets.sort();
        let mut expected: Vec<String> = (0..8).flat_map(|writer| (0..10).map(move |n| format!("T-{writer}-{n}"))).collect();
        expected.sort();
        assert_eq!(tickets, expected, "every skipped run taken exactly once");
        assert_eq!(TrustService::state(&tasks), TrustState::Trusted, "no write lost the trust record");
    }
}
