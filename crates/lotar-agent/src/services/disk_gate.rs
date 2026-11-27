//! The agent queue starts no new job in a job worktree while the disk is
//! nearly full (DEV-837): free space on the worktrees' volume (the build
//! root's when set) is under `agent.disk_gate.min_free_gb`.
//!
//! Under the floor, the queue worker first removes the cargo build profiles
//! of idle job worktrees, oldest first, until there is room again
//! ([`clean`]). It never removes a worktree or anything but build output,
//! and it runs outside the job registry's lock: [`check`] only reads free
//! space.
//!
//! The last reading, the hold it found and the last cleanup are kept next to
//! the queue file, as the memory gate's are, so the queue worker, `lotar
//! serve` and `lotar agent queue` in another process see the same hold, and
//! `lotar serve` logs it ([`HoldLog`]).

use crate::config::types::{AgentDiskGateConfig, ResolvedConfig};
use crate::services::agent_job_service as jobs;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How often a held queue reads free space again.
pub const HELD_RECHECK_SECS: u64 = 30;
/// How long an open gate reuses its reading.
const OPEN_RECHECK_SECS: u64 = 5;
/// A held reading older than this has no process reading it again.
const STALE_AFTER_SECS: u64 = 2 * HELD_RECHECK_SECS;
/// Under the floor, idle builds are looked for at most this often.
const CLEANUP_EVERY_SECS: u64 = 10 * 60;
/// `df -h` counts in powers of 1024.
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct GateState {
    #[serde(default, skip_serializing_if = "Option::is_none", with = "rfc3339")]
    checked_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "rfc3339")]
    held_since: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    /// The floor the reading was compared with: a changed floor reads again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    floor_gb: Option<u64>,
    /// The last hold that ended, kept until the next one ends, so `lotar
    /// serve` logs it also when another process ended it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_hold: Option<EndedHold>,
    /// The last look for idle builds, and what it removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_cleanup: Option<Cleanup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EndedHold {
    reason: String,
    #[serde(with = "rfc3339_required")]
    since: Timestamp,
    #[serde(with = "rfc3339_required")]
    until: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Cleanup {
    #[serde(with = "rfc3339_required")]
    at: Timestamp,
    /// One line per worktree whose build profiles it removed.
    #[serde(default)]
    removed: Vec<String>,
    /// Free space after it, as `12.3 GB free, floor 60 GB`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    free: Option<String>,
}

mod rfc3339_required {
    use crate::utils::time::{self, Rfc3339};
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(at: &Timestamp, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&at.to_rfc3339_z())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Timestamp, D::Error> {
        time::parse_rfc3339(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

mod rfc3339 {
    use crate::utils::time::{self, Rfc3339};
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(at: &Option<Timestamp>, serializer: S) -> Result<S::Ok, S::Error> {
        match at {
            Some(at) => serializer.serialize_str(&at.to_rfc3339_z()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Timestamp>, D::Error> {
        Option::<String>::deserialize(deserializer)?.map(|raw| time::parse_rfc3339(&raw).map_err(serde::de::Error::custom)).transpose()
    }
}

/// Why new jobs wait for disk space, and since when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskHold {
    pub reason: String,
    pub since: Timestamp,
}

impl DiskHold {
    pub fn describe(&self) -> String {
        format!("{} (held since {})", self.reason, stamp(self.since))
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "reason": self.reason, "since": stamp(self.since) })
    }
}

/// What the gate reads for a tasks directory.
struct Gate {
    config: AgentDiskGateConfig,
    /// The folder whose volume's free space counts.
    volume: PathBuf,
    repo_root: Option<PathBuf>,
    branch_prefix: String,
}

impl Gate {
    /// `None` when jobs don't run in worktrees or the floor is 0.
    fn of(tasks_dir: &Path) -> Option<Self> {
        let config = crate::config::resolution::config_for_project(tasks_dir, None).ok()?;
        Self::from_config(tasks_dir, &config)
    }

    fn from_config(tasks_dir: &Path, config: &ResolvedConfig) -> Option<Self> {
        if !config.agent_worktree.enabled || config.agent_disk_gate.min_free_gb == 0 {
            return None;
        }
        let repo_root = jobs::resolve_repo_root(tasks_dir);
        let volume = repo_root
            .as_deref()
            .and_then(|root| {
                let build_root = jobs::resolve_build_root(root, config.agent_worktree.build_root.as_deref()).ok().flatten();
                build_root.or_else(|| jobs::resolve_worktree_root(root, config.agent_worktree.dir.as_deref()).ok())
            })
            .unwrap_or_else(|| tasks_dir.to_path_buf());
        Some(Self { config: config.agent_disk_gate.clone(), volume, repo_root, branch_prefix: config.agent_worktree.branch_prefix.clone() })
    }

    fn floor_bytes(&self) -> u64 {
        (self.config.min_free_gb as f64 * GIB) as u64
    }

    fn free(&self) -> Result<Free, String> {
        read_free(&self.volume)
    }
}

/// Whether new jobs in job worktrees wait for disk space. Reads free space
/// again once the last reading is older than 30 s (held) or 5 s (open), or
/// the floor changed. It only reads: the queue calls it under the job
/// registry's lock, and [`clean`] removes idle builds outside it.
pub fn check(tasks_dir: &Path) -> Option<DiskHold> {
    let gate = Gate::of(tasks_dir)?;
    let path = state_path(tasks_dir);
    let previous = load(&path);
    let now = Timestamp::now();
    let recheck = if previous.held_since.is_some() { HELD_RECHECK_SECS } else { OPEN_RECHECK_SECS };
    if previous.floor_gb == Some(gate.config.min_free_gb) && previous.checked_at.is_some_and(|at| now - at < seconds(recheck)) {
        return hold_of(&previous);
    }
    let next = decide(&previous, gate.free().ok(), now, &gate.config);
    save(&path, &next);
    hold_of(&next)
}

/// Under the floor, remove the cargo build profiles of idle job worktrees,
/// oldest first, until free space is back over it; at most every 10 min.
/// The queue worker calls it before it reads the gate, outside any lock: it
/// may remove many GB. The next [`check`] reads free space again.
pub fn clean(tasks_dir: &Path) {
    let Some(gate) = Gate::of(tasks_dir) else {
        return;
    };
    let Some(root) = gate.repo_root.as_deref() else {
        return;
    };
    let path = state_path(tasks_dir);
    let now = Timestamp::now();
    if load(&path).last_cleanup.is_some_and(|cleanup| now - cleanup.at < seconds(CLEANUP_EVERY_SECS)) {
        return;
    }
    let floor = gate.floor_bytes();
    if !gate.free().is_ok_and(|free| free.bytes < floor) {
        return;
    }
    let running = running_branches(tasks_dir, &gate.branch_prefix);
    let idle = Duration::from_secs(gate.config.idle_hours.saturating_mul(3600));
    let removed = clean_idle_builds(root, &gate.branch_prefix, &running, idle, SystemTime::now(), || gate.free().is_ok_and(|free| free.bytes >= floor));
    let free = gate.free().ok().map(|free| format!("{:.1} GB free ({}), floor {} GB", free.bytes as f64 / GIB, free.reading, gate.config.min_free_gb));
    // Another process may have saved a reading meanwhile: keep it.
    let mut state = load(&path);
    state.last_cleanup = Some(Cleanup { at: now, removed, free });
    state.checked_at = None;
    save(&path, &state);
}

/// The last reading's hold. A hold that no process read again for 60 s is
/// replaced by a reading of its own (no cleanup), which is not saved.
pub fn status(tasks_dir: &Path) -> Option<DiskHold> {
    let gate = Gate::of(tasks_dir)?;
    let state = load(&state_path(tasks_dir));
    let now = Timestamp::now();
    let stale = state.checked_at.is_none_or(|at| now - at > seconds(STALE_AFTER_SECS));
    if stale && state.held_since.is_some() {
        return hold_of(&decide(&GateState::default(), gate.free().ok(), now, &gate.config));
    }
    hold_of(&state)
}

/// A failed reading holds nothing and ends a hold.
fn decide(previous: &GateState, free: Option<Free>, now: Timestamp, config: &AgentDiskGateConfig) -> GateState {
    let floor = (config.min_free_gb as f64 * GIB) as u64;
    let kept = GateState {
        checked_at: Some(now),
        floor_gb: Some(config.min_free_gb),
        last_hold: previous.last_hold.clone(),
        last_cleanup: previous.last_cleanup.clone(),
        ..GateState::default()
    };
    match free {
        Some(free) if free.bytes < floor => GateState { held_since: previous.held_since.or(Some(now)), reason: Some(free_text(free, config)), ..kept },
        _ => {
            let ended = previous.held_since.map(|since| EndedHold { reason: previous.reason.clone().unwrap_or_default(), since, until: now });
            GateState { last_hold: ended.or(kept.last_hold.clone()), ..kept }
        }
    }
}

fn free_text(free: Free, config: &AgentDiskGateConfig) -> String {
    format!("{:.1} GB free on the job worktrees' disk ({}), under the floor of {} GB", free.bytes as f64 / GIB, free.reading, config.min_free_gb)
}

/// Writes each hold's start and end and each cleanup once, for `lotar
/// serve`'s log (as the memory gate's DEV-827 log): it reads the state file
/// that `check` and `clean` keep in whichever process.
#[derive(Debug, Default)]
pub struct HoldLog {
    started: Option<Timestamp>,
    ended: Option<Timestamp>,
    cleaned: Option<Timestamp>,
}

impl HoldLog {
    /// A hold that ended or a cleanup that ran before now is not logged
    /// again; a hold in effect is.
    pub fn new(tasks_dir: &Path) -> Self {
        let state = load(&state_path(tasks_dir));
        Self { started: None, ended: state.last_hold.map(|hold| hold.since), cleaned: state.last_cleanup.map(|cleanup| cleanup.at) }
    }

    /// The lines for what changed since the last call.
    pub fn poll(&mut self, tasks_dir: &Path) -> Vec<String> {
        if Gate::of(tasks_dir).is_none() {
            return Vec::new();
        }
        self.lines(&load(&state_path(tasks_dir)))
    }

    fn lines(&mut self, state: &GateState) -> Vec<String> {
        let mut lines = Vec::new();
        let started_line = |reason: &str, since: Timestamp| format!("[lotar] disk hold started: {reason}, since {}", stamp(since));
        if let Some(hold) = state.last_hold.as_ref().filter(|hold| self.ended != Some(hold.since)) {
            // Started and ended between two reads: both lines.
            if self.started != Some(hold.since) {
                lines.push(started_line(&hold.reason, hold.since));
            }
            lines.push(format!("[lotar] disk hold ended: {}, since {}, until {}", hold.reason, stamp(hold.since), stamp(hold.until)));
            self.started = Some(hold.since);
            self.ended = Some(hold.since);
        }
        if let Some(cleanup) = state.last_cleanup.as_ref().filter(|cleanup| self.cleaned != Some(cleanup.at)) {
            if cleanup.removed.is_empty() {
                lines.push(format!("[lotar] disk gate: no idle build profiles to remove at {}", stamp(cleanup.at)));
            }
            lines.extend(cleanup.removed.iter().map(|removed| format!("[lotar] disk gate: {removed} at {}", stamp(cleanup.at))));
            if let Some(free) = &cleanup.free {
                lines.push(format!("[lotar] disk gate: after the cleanup, {free}"));
            }
            self.cleaned = Some(cleanup.at);
        }
        if let (Some(since), Some(reason)) = (state.held_since.filter(|since| self.started != Some(*since)), state.reason.as_deref()) {
            lines.push(started_line(reason, since));
            self.started = Some(since);
        }
        lines
    }
}

/// The ticket branches of the jobs running for the tasks directory, in
/// every process.
fn running_branches(tasks_dir: &Path, prefix: &str) -> HashSet<String> {
    crate::services::agent_active_jobs::list(tasks_dir)
        .into_iter()
        .map(|job| jobs::build_worktree_branch(prefix, &jobs::sanitize_worktree_token(&job.ticket_id)))
        .collect()
}

/// Remove the cargo build profiles of job worktrees whose ticket has no
/// running job and in which nothing was built or committed for `idle`,
/// oldest first, until `enough` says there is room. Returns what it removed.
fn clean_idle_builds(
    repo_root: &Path,
    prefix: &str,
    running: &HashSet<String>,
    idle: Duration,
    now: SystemTime,
    mut enough: impl FnMut() -> bool,
) -> Vec<String> {
    let Ok(worktrees) = jobs::agent_worktrees(repo_root, prefix) else {
        return Vec::new();
    };
    let mut idle_builds: Vec<(SystemTime, PathBuf, Vec<String>)> = worktrees
        .into_iter()
        .filter(|worktree| !running.contains(&worktree.ticket_branch))
        .filter_map(|worktree| {
            let target = worktree.path.join("target");
            let profiles = cargo_profiles(&target);
            if profiles.is_empty() {
                return None;
            }
            let last = last_work(&worktree.path, &target, &profiles);
            now.duration_since(last).is_ok_and(|since| since >= idle).then_some((last, target, profiles))
        })
        .collect();
    idle_builds.sort_by_key(|(last, _, _)| *last);
    let mut removed = Vec::new();
    for (_, target, profiles) in idle_builds {
        if enough() {
            break;
        }
        let gone: Vec<String> = profiles.into_iter().filter(|profile| std::fs::remove_dir_all(target.join(profile)).is_ok()).collect();
        if !gone.is_empty() {
            removed.push(format!("removed the cargo build profiles {} from {}", gone.join(", "), target.display()));
        }
    }
    removed
}

/// The folders in a Rust `target/` that `cargo clean --profile` would
/// remove: those with a `.fingerprint`, also under a target triple.
fn cargo_profiles(target: &Path) -> Vec<String> {
    let is_profile = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) && path.join(".fingerprint").is_dir();
    let dirs = |dir: &Path| -> Vec<String> {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                    .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut profiles: Vec<String> = dirs(target)
        .into_iter()
        .flat_map(|name| {
            if is_profile(&target.join(&name)) {
                vec![name]
            } else {
                dirs(&target.join(&name)).into_iter().map(|sub| format!("{name}/{sub}")).filter(|path| is_profile(&target.join(path))).collect()
            }
        })
        .collect();
    profiles.sort();
    profiles
}

/// When the worktree was last worked in: the newest change to its folder,
/// its git index and HEAD, and its build profiles' folders (cargo rewrites
/// `deps` and `.fingerprint` on every build).
fn last_work(worktree: &Path, target: &Path, profiles: &[String]) -> SystemTime {
    let git_dir = std::fs::read_to_string(worktree.join(".git")).ok().and_then(|link| link.trim().strip_prefix("gitdir:").map(|dir| PathBuf::from(dir.trim())));
    let mut paths = vec![worktree.to_path_buf()];
    if let Some(git_dir) = git_dir {
        paths.extend(["index", "HEAD", "logs/HEAD"].map(|name| git_dir.join(name)));
    }
    for profile in profiles {
        let dir = target.join(profile);
        paths.extend([".fingerprint", "deps", "build", "incremental"].map(|name| dir.join(name)));
        paths.push(dir);
    }
    paths.iter().filter_map(|path| std::fs::metadata(path).and_then(|meta| meta.modified()).ok()).max().unwrap_or(SystemTime::UNIX_EPOCH)
}

/// A free space reading and which reading it is, for the hold message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Free {
    bytes: u64,
    reading: &'static str,
}

/// On macOS the space available for important use, which counts purgeable
/// space (local Time Machine snapshots, caches) that macOS frees by itself
/// when space runs short (DEV-937); statvfs's available space elsewhere, or
/// when macOS gives no such reading. `LOTAR_DISK_FREE_GB` replaces the
/// reading, for tests: free GB, as `12.5`.
fn read_free(volume: &Path) -> Result<Free, String> {
    if let Ok(fake) = std::env::var("LOTAR_DISK_FREE_GB") {
        let gb = fake.trim().parse::<f64>().ok().filter(|gb| *gb >= 0.0).ok_or_else(|| format!("LOTAR_DISK_FREE_GB: bad value '{fake}'"))?;
        return Ok(Free { bytes: (gb * GIB) as u64, reading: "LOTAR_DISK_FREE_GB" });
    }
    read_volume(volume)
}

fn read_volume(volume: &Path) -> Result<Free, String> {
    // The volume folder may not exist yet: its nearest existing parent is on the same disk.
    let existing = volume.ancestors().find(|dir| dir.is_dir()).unwrap_or(volume);
    #[cfg(target_os = "macos")]
    if let Some(bytes) = important_usage::available(existing) {
        return Ok(Free { bytes, reading: IMPORTANT_USAGE });
    }
    let bytes = fs2::available_space(existing).map_err(|err| format!("free space of {}: {err}", existing.display()))?;
    Ok(Free { bytes, reading: "statvfs" })
}

#[cfg(target_os = "macos")]
const IMPORTANT_USAGE: &str = "available for important use, purgeable included";

/// `NSURLVolumeAvailableCapacityForImportantUsageKey`, through CoreFoundation.
#[cfg(target_os = "macos")]
mod important_usage {
    use std::ffi::c_void;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    type CfRef = *const c_void;
    /// `kCFNumberSInt64Type`.
    const SINT64: isize = 4;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFURLVolumeAvailableCapacityForImportantUsageKey: CfRef;
        fn CFURLCreateFromFileSystemRepresentation(allocator: CfRef, buffer: *const u8, len: isize, is_directory: u8) -> CfRef;
        fn CFURLCopyResourcePropertyForKey(url: CfRef, key: CfRef, value: *mut CfRef, error: *mut CfRef) -> u8;
        fn CFGetTypeID(value: CfRef) -> usize;
        fn CFNumberGetTypeID() -> usize;
        fn CFNumberGetValue(number: CfRef, kind: isize, value: *mut c_void) -> u8;
        fn CFRelease(value: CfRef);
    }

    /// `None` when macOS gives no reading for the volume.
    pub fn available(dir: &Path) -> Option<u64> {
        let path = dir.as_os_str().as_bytes();
        let len = isize::try_from(path.len()).ok()?;
        // SAFETY: the buffer is `len` bytes long; each object created or
        // copied here is released once, and the number is read only after
        // its type is checked.
        unsafe {
            let url = CFURLCreateFromFileSystemRepresentation(std::ptr::null(), path.as_ptr(), len, 1);
            if url.is_null() {
                return None;
            }
            let mut value: CfRef = std::ptr::null();
            let copied = CFURLCopyResourcePropertyForKey(url, kCFURLVolumeAvailableCapacityForImportantUsageKey, &mut value, std::ptr::null_mut());
            CFRelease(url);
            if copied == 0 || value.is_null() {
                return None;
            }
            let mut bytes: i64 = 0;
            let read = CFGetTypeID(value) == CFNumberGetTypeID() && CFNumberGetValue(value, SINT64, (&raw mut bytes).cast()) != 0;
            CFRelease(value);
            if read { u64::try_from(bytes).ok() } else { None }
        }
    }
}

fn hold_of(state: &GateState) -> Option<DiskHold> {
    Some(DiskHold { reason: state.reason.clone()?, since: state.held_since? })
}

fn seconds(secs: u64) -> SignedDuration {
    SignedDuration::from_secs(i64::try_from(secs).unwrap_or(i64::MAX))
}

/// `2026-11-16T20:57:48Z`.
fn stamp(at: Timestamp) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn state_path(tasks_dir: &Path) -> PathBuf {
    use crate::services::agent_queue_service::{queue_dir, queue_hash};
    queue_dir().join(format!("disk-gate-{}.json", queue_hash(tasks_dir)))
}

fn save(path: &Path, state: &GateState) {
    let _ = crate::storage::safety::atomic_write_file(path, &serde_json::to_string_pretty(state).unwrap_or_default());
}

fn load(path: &Path) -> GateState {
    std::fs::read_to_string(path).ok().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + secs).unwrap()
    }

    fn gb(value: f64) -> Option<Free> {
        Some(Free { bytes: (value * GIB) as u64, reading: "statvfs" })
    }

    #[test]
    fn under_the_floor_holds_until_space_is_back() {
        let config = AgentDiskGateConfig::default();
        let held = decide(&GateState::default(), gb(41.5), at(0), &config);
        let hold = hold_of(&held).unwrap();
        assert_eq!(hold.since, at(0));
        assert_eq!(hold.reason, "41.5 GB free on the job worktrees' disk (statvfs), under the floor of 60 GB");
        // Still under: the first since stays.
        let still = decide(&held, gb(59.9), at(30), &config);
        assert_eq!(hold_of(&still).unwrap().since, at(0));
        // At the floor, held jobs start.
        let open = decide(&still, gb(60.0), at(60), &config);
        assert!(hold_of(&open).is_none());
        let ended =
            EndedHold { reason: "59.9 GB free on the job worktrees' disk (statvfs), under the floor of 60 GB".to_string(), since: at(0), until: at(60) };
        assert_eq!(open, GateState { checked_at: Some(at(60)), floor_gb: Some(60), last_hold: Some(ended), ..GateState::default() });
        // A failed reading holds nothing and ends the hold at its time.
        let failed = decide(&held, None, at(90), &config);
        assert!(hold_of(&failed).is_none());
        assert_eq!(failed.last_hold.map(|hold| hold.until), Some(at(90)));
    }

    /// The tests' `LOTAR_DISK_FREE_GB` stands in for `read_free`'s reading;
    /// this reads the disk.
    #[test]
    fn the_reading_counts_purgeable_space_on_macos_and_is_statvfs_elsewhere() {
        let dir = tempfile::tempdir().unwrap();
        let free = read_volume(&dir.path().join("not/yet")).unwrap();
        let statvfs = fs2::available_space(dir.path()).unwrap();
        #[cfg(target_os = "macos")]
        {
            assert_eq!(free.reading, IMPORTANT_USAGE);
            // Purgeable space comes on top; other writers move both a little.
            assert!(free.bytes as f64 >= statvfs as f64 * 0.9, "{free:?}, statvfs {statvfs}");
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(free.reading, "statvfs");
            assert!(free.bytes.abs_diff(statvfs) < 1 << 30, "{free:?}, statvfs {statvfs}");
        }
        assert_eq!(read_free(dir.path()).unwrap().reading, "LOTAR_DISK_FREE_GB");
    }

    #[test]
    fn hold_log_writes_start_end_and_cleanups_once_each() {
        let config = AgentDiskGateConfig::default();
        let mut log = HoldLog::default();
        let open = decide(&GateState::default(), gb(100.0), at(0), &config);
        assert!(log.lines(&open).is_empty());
        let mut held = decide(&open, gb(41.5), at(5), &config);
        assert_eq!(
            log.lines(&held),
            ["[lotar] disk hold started: 41.5 GB free on the job worktrees' disk (statvfs), under the floor of 60 GB, since 2027-01-15T08:00:05Z"]
        );
        held.last_cleanup = Some(Cleanup {
            at: at(6),
            removed: vec!["removed the cargo build profiles debug from /w/T-1/target".to_string()],
            free: Some("50.0 GB free (statvfs), floor 60 GB".to_string()),
        });
        let still = decide(&held, gb(50.0), at(35), &config);
        assert_eq!(
            log.lines(&still),
            [
                "[lotar] disk gate: removed the cargo build profiles debug from /w/T-1/target at 2027-01-15T08:00:06Z",
                "[lotar] disk gate: after the cleanup, 50.0 GB free (statvfs), floor 60 GB",
            ],
            "the same hold with another reason starts once"
        );
        let mut later = decide(&still, gb(50.0), at(700), &config);
        later.last_cleanup = Some(Cleanup { at: at(650), removed: Vec::new(), free: None });
        assert_eq!(log.lines(&later), ["[lotar] disk gate: no idle build profiles to remove at 2027-01-15T08:10:50Z"]);
        let back = decide(&later, gb(61.0), at(730), &config);
        assert_eq!(
            log.lines(&back),
            [
                "[lotar] disk hold ended: 50.0 GB free on the job worktrees' disk (statvfs), under the floor of 60 GB, since 2027-01-15T08:00:05Z, until 2027-01-15T08:12:10Z"
            ]
        );
        assert!(log.lines(&decide(&back, gb(61.0), at(760), &config)).is_empty(), "an ended hold is logged once");
        // A log started after the hold ended and the cleanup ran logs neither.
        let mut fresh = HoldLog { started: None, ended: back.last_hold.as_ref().map(|hold| hold.since), cleaned: Some(at(650)) };
        assert!(fresh.lines(&back).is_empty());
    }

    #[test]
    fn the_gate_applies_only_to_worktree_jobs_with_a_floor() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = ResolvedConfig::from_global(crate::config::types::GlobalConfig::default());
        assert!(Gate::from_config(dir.path(), &config).is_none(), "no worktrees, no gate");
        config.agent_worktree.enabled = true;
        let gate = Gate::from_config(dir.path(), &config).unwrap();
        assert_eq!(gate.config.min_free_gb, 60);
        config.agent_disk_gate.min_free_gb = 0;
        assert!(Gate::from_config(dir.path(), &config).is_none(), "0 turns it off");
    }

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git").args(args).current_dir(dir).env("GIT_CONFIG_NOSYSTEM", "1").output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// A worktree on `agent/<ticket>` with a debug and a triple's release
    /// profile, all last changed `age` ago.
    fn worktree_with_build(repo: &Path, ticket: &str, age: Duration) -> PathBuf {
        let path = repo.parent().unwrap().join(ticket);
        git(repo, &["worktree", "add", "-q", "-b", &format!("agent/{ticket}"), path.to_str().unwrap()]);
        for profile in ["debug", "aarch64-apple-darwin/release"] {
            for sub in [".fingerprint", "deps"] {
                std::fs::create_dir_all(path.join("target").join(profile).join(sub)).unwrap();
            }
            std::fs::write(path.join("target").join(profile).join("deps/liblotar.rlib"), "build").unwrap();
        }
        std::fs::write(path.join("notes.txt"), "uncommitted work").unwrap();
        let then = SystemTime::now() - age;
        let git_dir = repo.join(".git/worktrees").join(ticket);
        let mut paths = vec![path.clone(), git_dir.join("index"), git_dir.join("HEAD"), git_dir.join("logs/HEAD")];
        for profile in ["debug", "aarch64-apple-darwin/release"] {
            let dir = path.join("target").join(profile);
            paths.extend([dir.join(".fingerprint"), dir.join("deps"), dir.clone()]);
        }
        for path in paths.iter().filter(|path| path.exists()) {
            std::fs::File::open(path).unwrap().set_modified(then).unwrap();
        }
        path
    }

    #[test]
    fn cleanup_removes_idle_builds_oldest_first_and_spares_running_and_recent_ones() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "init"]);
        let hours = |h: u64| Duration::from_secs(h * 3600);
        let running = worktree_with_build(&repo, "T-1", hours(48));
        let oldest = worktree_with_build(&repo, "T-2", hours(30));
        let older = worktree_with_build(&repo, "T-3", hours(10));
        let recent = worktree_with_build(&repo, "T-4", hours(1));
        let running_branches: HashSet<String> = ["agent/T-1".to_string()].into();

        // Room after the first one: only the oldest idle build goes.
        let mut calls = 0;
        let removed = clean_idle_builds(&repo, "agent/", &running_branches, hours(6), SystemTime::now(), || {
            calls += 1;
            calls > 1
        });
        assert_eq!(removed.len(), 1, "{removed:?}");
        assert!(removed[0].contains("aarch64-apple-darwin/release, debug"), "{removed:?}");
        assert!(!oldest.join("target/debug").exists() && !oldest.join("target/aarch64-apple-darwin/release").exists());
        assert!(older.join("target/debug/deps/liblotar.rlib").exists());

        // Never enough: every idle build goes, the running and the recent one stay.
        let removed = clean_idle_builds(&repo, "agent/", &running_branches, hours(6), SystemTime::now(), || false);
        assert_eq!(removed.len(), 1, "{removed:?}");
        assert!(!older.join("target/debug").exists());
        assert!(running.join("target/debug/deps/liblotar.rlib").exists(), "a worktree with a running job stays untouched");
        assert!(recent.join("target/debug/deps/liblotar.rlib").exists(), "a build from an hour ago stays");
        // The worktrees and their uncommitted work stay.
        for worktree in [&running, &oldest, &older, &recent] {
            assert_eq!(std::fs::read_to_string(worktree.join("notes.txt")).unwrap(), "uncommitted work");
            assert!(worktree.join("target").is_dir());
        }
    }

    #[test]
    fn profiles_are_the_folders_with_a_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        for path in ["debug/.fingerprint", "x86_64-unknown-linux-gnu/debug/.fingerprint", "web/assets", "tmp"] {
            std::fs::create_dir_all(target.join(path)).unwrap();
        }
        assert_eq!(cargo_profiles(&target), ["debug", "x86_64-unknown-linux-gnu/debug"]);
        assert!(cargo_profiles(&dir.path().join("missing")).is_empty());
    }
}
