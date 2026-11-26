//! The agent queue starts no new job while the machine is short of memory
//! (DEV-623): macOS reports memory pressure, or swap is nearly full while
//! little memory is free. macOS keeps pages in swap long after a shortage,
//! so full swap with free memory holds nothing (DEV-819).
//!
//! The last reading and the hold it found are kept next to the queue file,
//! so the queue worker, `lotar serve` and `lotar agent queue` in another
//! process see the same hold. Each hold that ended is added to a history
//! file beside it, so the hours the gate held can be counted (DEV-882).

use crate::config::manager::ConfigManager;
use crate::config::types::AgentMemoryGateConfig;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How often a held queue reads memory again.
pub const HELD_RECHECK_SECS: u64 = 30;
/// How long an open gate reuses its reading.
const OPEN_RECHECK_SECS: u64 = 5;
/// A held or failed reading older than this has no process reading memory
/// again; `status` reads it itself (DEV-645).
const STALE_AFTER_SECS: u64 = 2 * HELD_RECHECK_SECS;
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
/// Full swap holds only while less memory than this is free, in percent.
pub const SWAP_HOLD_FREE_PERCENT: u8 = 20;
/// The hold history keeps this many of the last holds.
const HISTORY_LIMIT: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressure {
    Normal,
    Warn,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryReading {
    /// `None` where the system reports no pressure level (Linux).
    pub pressure: Option<Pressure>,
    pub swap_used: u64,
    pub swap_total: u64,
    /// Free memory in percent of RAM (`kern.memorystatus_level`,
    /// `MemAvailable`); `None` where unknown.
    pub free_percent: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct GateState {
    #[serde(default, skip_serializing_if = "Option::is_none", with = "rfc3339")]
    checked_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "rfc3339")]
    held_since: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "rfc3339")]
    normal_since: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    /// Why the last reading failed; a failed reading holds nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// The last short-of-memory reason of a hold, kept while it settles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    held_reason: Option<String>,
    /// The last hold that ended, kept until the next one ends, so `lotar
    /// serve` logs it also when another process ended it (DEV-827).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_hold: Option<EndedHold>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EndedHold {
    reason: String,
    #[serde(with = "rfc3339_required")]
    since: Timestamp,
    #[serde(with = "rfc3339_required")]
    until: Timestamp,
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

/// The state file's times as RFC 3339 text.
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

/// Why new jobs wait for memory, and since when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryHold {
    pub reason: String,
    pub since: Timestamp,
}

impl MemoryHold {
    pub fn describe(&self) -> String {
        format!("{} (held since {})", self.reason, stamp(self.since))
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "reason": self.reason, "since": stamp(self.since) })
    }
}

/// What the last reading found, for status lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GateStatus {
    pub hold: Option<MemoryHold>,
    pub error: Option<String>,
}

/// Whether new jobs for this tasks directory wait for memory. Reads memory
/// again once the last reading is older than 30 s (held) or 5 s (open).
pub fn check(tasks_dir: &Path) -> Option<MemoryHold> {
    let config = gate_config(tasks_dir);
    if !config.is_enabled() {
        return None;
    }
    let path = state_path(tasks_dir);
    let previous = load(&path);
    let now = Timestamp::now();
    let recheck = if previous.held_since.is_some() { HELD_RECHECK_SECS } else { OPEN_RECHECK_SECS };
    if previous.checked_at.is_some_and(|at| now - at < seconds(recheck)) {
        return hold_of(&previous);
    }
    let next = decide(&previous, read_memory(), now, &config);
    let _ = crate::storage::safety::atomic_write_file(&path, &serde_json::to_string_pretty(&next).unwrap_or_default());
    record_if_ended(tasks_dir, &previous, &next);
    hold_of(&next)
}

/// A hold the gate kept, from the history (DEV-882); `until` is `None` for
/// the hold in effect now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedHold {
    pub reason: String,
    pub since: Timestamp,
    pub until: Option<Timestamp>,
}

impl RecordedHold {
    pub fn to_json(&self) -> serde_json::Value {
        let seconds = self.until.map(|until| (until - self.since).as_secs());
        serde_json::json!({ "reason": self.reason, "since": stamp(self.since), "until": self.until.map(stamp), "seconds": seconds })
    }
}

/// The holds of the history, oldest first, and the one in effect now. Two
/// processes can end the same hold; it is listed once.
pub fn holds(tasks_dir: &Path) -> Vec<RecordedHold> {
    let mut holds: Vec<RecordedHold> = Vec::new();
    let history = std::fs::read_to_string(history_path(tasks_dir)).unwrap_or_default();
    for ended in history.lines().filter_map(|line| serde_json::from_str::<EndedHold>(line).ok()) {
        holds.retain(|hold| hold.since != ended.since);
        holds.push(RecordedHold { reason: ended.reason, since: ended.since, until: Some(ended.until) });
    }
    let state = load(&state_path(tasks_dir));
    if let Some(since) = state.held_since.filter(|since| holds.iter().all(|hold| hold.since != *since)) {
        holds.push(RecordedHold { reason: state.short_reason(), since, until: None });
    }
    holds.sort_by_key(|hold| hold.since);
    holds
}

/// Adds the hold that ended between `previous` and `next` to the history.
fn record_if_ended(tasks_dir: &Path, previous: &GateState, next: &GateState) {
    if let Some(ended) = next.last_hold.as_ref().filter(|ended| previous.last_hold.as_ref() != Some(*ended)) {
        record_ended(&history_path(tasks_dir), ended);
    }
}

/// Adds `ended` to the history, keeping the last [`HISTORY_LIMIT`] holds.
fn record_ended(path: &Path, ended: &EndedHold) {
    let Ok(line) = serde_json::to_string(ended) else {
        return;
    };
    let mut lines: Vec<String> = std::fs::read_to_string(path).unwrap_or_default().lines().map(str::to_string).collect();
    lines.push(line);
    let keep = lines.split_off(lines.len().saturating_sub(HISTORY_LIMIT));
    let _ = crate::storage::safety::atomic_write_file(path, &(keep.join("\n") + "\n"));
}

/// The last reading's hold and error. A held or failed reading that no
/// process read again for 60 s is replaced by a reading of its own, which
/// is not saved: only `check` moves the gate on.
pub fn status(tasks_dir: &Path) -> GateStatus {
    let config = gate_config(tasks_dir);
    if !config.is_enabled() {
        return GateStatus::default();
    }
    let mut state = load(&state_path(tasks_dir));
    let now = Timestamp::now();
    let stale = state.checked_at.is_none_or(|at| now - at > seconds(STALE_AFTER_SECS));
    if stale && (state.held_since.is_some() || state.error.is_some()) {
        state = decide(&GateState::default(), read_memory(), now, &config);
    }
    GateStatus { hold: hold_of(&state), error: state.error }
}

fn decide(previous: &GateState, reading: Result<MemoryReading, String>, now: Timestamp, config: &AgentMemoryGateConfig) -> GateState {
    // A hold ends when its memory settled or a reading failed.
    let ended = |previous: &GateState| {
        previous.held_since.map(|since| EndedHold { reason: previous.short_reason(), since, until: now }).or_else(|| previous.last_hold.clone())
    };
    let reading = match reading {
        Ok(reading) => reading,
        Err(error) => return GateState { checked_at: Some(now), error: Some(error), last_hold: ended(previous), ..GateState::default() },
    };
    if let Some(reason) = short_of_memory(&reading, config) {
        return GateState {
            checked_at: Some(now),
            held_since: previous.held_since.or(Some(now)),
            reason: Some(reason.clone()),
            held_reason: Some(reason),
            last_hold: previous.last_hold.clone(),
            ..GateState::default()
        };
    }
    let Some(held_since) = previous.held_since else {
        return GateState { checked_at: Some(now), last_hold: previous.last_hold.clone(), ..GateState::default() };
    };
    let normal_since = previous.normal_since.unwrap_or(now);
    if now - normal_since >= seconds(config.settle_seconds) {
        return GateState { checked_at: Some(now), last_hold: ended(previous), ..GateState::default() };
    }
    let reason = format!("memory is normal again since {}; held jobs start after {} s of normal memory", stamp(normal_since), config.settle_seconds);
    GateState {
        checked_at: Some(now),
        held_since: Some(held_since),
        normal_since: Some(normal_since),
        reason: Some(reason),
        error: None,
        held_reason: previous.held_reason.clone(),
        last_hold: previous.last_hold.clone(),
    }
}

impl GateState {
    /// Why memory was short; a state file from before DEV-827 has only `reason`.
    fn short_reason(&self) -> String {
        self.held_reason.clone().or_else(|| self.reason.clone()).unwrap_or_default()
    }
}

/// Writes each hold's start and end once, for `lotar serve`'s log: it reads
/// the state file that `check` keeps in whichever process, and no memory.
#[derive(Debug, Default)]
pub struct HoldLog {
    started: Option<Timestamp>,
    ended: Option<Timestamp>,
}

impl HoldLog {
    /// A hold that ended before now is not logged again; one in effect is.
    pub fn new(tasks_dir: &Path) -> Self {
        Self { started: None, ended: load(&state_path(tasks_dir)).last_hold.map(|hold| hold.since) }
    }

    /// The lines for what changed since the last call.
    pub fn poll(&mut self, tasks_dir: &Path) -> Vec<String> {
        if !gate_config(tasks_dir).is_enabled() {
            return Vec::new();
        }
        self.lines(&load(&state_path(tasks_dir)))
    }

    fn lines(&mut self, state: &GateState) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(hold) = state.last_hold.as_ref().filter(|hold| self.ended != Some(hold.since)) {
            // Started and ended between two reads: both lines.
            if self.started != Some(hold.since) {
                lines.push(started_line(&hold.reason, hold.since));
            }
            lines.push(format!("[lotar] memory hold ended: {}, since {}, until {}", hold.reason, stamp(hold.since), stamp(hold.until)));
            self.started = Some(hold.since);
            self.ended = Some(hold.since);
        }
        if let Some(since) = state.held_since.filter(|since| self.started != Some(*since)) {
            lines.push(started_line(&state.short_reason(), since));
            self.started = Some(since);
        }
        lines
    }
}

fn started_line(reason: &str, since: Timestamp) -> String {
    format!("[lotar] memory hold started: {reason}, since {}", stamp(since))
}

fn short_of_memory(reading: &MemoryReading, config: &AgentMemoryGateConfig) -> Option<String> {
    let mut reasons = Vec::new();
    match reading.pressure {
        Some(Pressure::Warn) => reasons.push("macOS reports memory pressure warn".to_string()),
        Some(Pressure::Critical) => reasons.push("macOS reports memory pressure critical".to_string()),
        Some(Pressure::Normal) | None => {}
    }
    let limit = u128::from(config.max_swap_percent);
    let free_is_low = reading.free_percent.is_none_or(|free| free < SWAP_HOLD_FREE_PERCENT);
    if free_is_low && reading.swap_total > 0 && u128::from(reading.swap_used) * 100 > limit * u128::from(reading.swap_total) {
        let percent = (u128::from(reading.swap_used) * 100 + u128::from(reading.swap_total) / 2) / u128::from(reading.swap_total);
        let (used, total) = (reading.swap_used as f64 / GIB, reading.swap_total as f64 / GIB);
        let free = reading.free_percent.map(|free| format!(", {free}% of memory free")).unwrap_or_default();
        reasons.push(format!("swap {used:.1} of {total:.1} GB used ({percent}%, limit {limit}%{free})"));
    }
    (!reasons.is_empty()).then(|| reasons.join("; "))
}

fn hold_of(state: &GateState) -> Option<MemoryHold> {
    Some(MemoryHold { reason: state.reason.clone()?, since: state.held_since? })
}

fn seconds(secs: u64) -> SignedDuration {
    SignedDuration::from_secs(i64::try_from(secs).unwrap_or(i64::MAX))
}

/// `2026-11-16T20:57:48Z`.
fn stamp(at: Timestamp) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn gate_config(tasks_dir: &Path) -> AgentMemoryGateConfig {
    ConfigManager::new_manager_with_tasks_dir_readonly(tasks_dir).map(|manager| manager.get_resolved_config().agent_memory_gate.clone()).unwrap_or_default()
}

fn state_path(tasks_dir: &Path) -> PathBuf {
    use crate::services::agent_queue_service::{queue_dir, queue_hash};
    queue_dir().join(format!("memory-gate-{}.json", queue_hash(tasks_dir)))
}

fn history_path(tasks_dir: &Path) -> PathBuf {
    use crate::services::agent_queue_service::{queue_dir, queue_hash};
    queue_dir().join(format!("memory-gate-{}-holds.jsonl", queue_hash(tasks_dir)))
}

/// Record a hold that a reading found just now.
#[cfg(test)]
pub(crate) fn hold_for_test(tasks_dir: &Path, reason: &str) {
    let path = state_path(tasks_dir);
    let now = Timestamp::now();
    let state = GateState { checked_at: Some(now), held_since: Some(now), reason: Some(reason.to_string()), ..GateState::default() };
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).ok();
    std::fs::write(&path, serde_json::to_string(&state).unwrap_or_default()).ok();
}

fn load(path: &Path) -> GateState {
    std::fs::read_to_string(path).ok().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

/// `LOTAR_MEMORY_READING` replaces the system's reading, for tests:
/// `normal`, or `pressure=warn,swap=9.5/10,free=12` (swap in GB, free
/// memory in percent; without `free` it is unknown).
fn read_memory() -> Result<MemoryReading, String> {
    if let Ok(fake) = std::env::var("LOTAR_MEMORY_READING") {
        return parse_fake(&fake);
    }
    read_system()
}

#[cfg(target_os = "macos")]
fn read_system() -> Result<MemoryReading, String> {
    let output = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "kern.memorystatus_vm_pressure_level", "vm.swapusage", "kern.memorystatus_level"])
        .output()
        .map_err(|err| format!("sysctl did not run: {err}"))?;
    if !output.status.success() {
        return Err(format!("sysctl failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    parse_sysctl(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "linux")]
fn read_system() -> Result<MemoryReading, String> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").map_err(|err| format!("/proc/meminfo: {err}"))?;
    parse_meminfo(&meminfo)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn read_system() -> Result<MemoryReading, String> {
    Err("lotar reads no memory state on this system".to_string())
}

/// `sysctl -n kern.memorystatus_vm_pressure_level vm.swapusage
/// kern.memorystatus_level`: the level (1 normal, 2 warn, 4 critical), then
/// `total = 9216.00M  used = 7894.81M  free = 1321.19M  (encrypted)`, then
/// free memory in percent.
#[cfg(any(target_os = "macos", test))]
fn parse_sysctl(output: &str) -> Result<MemoryReading, String> {
    let mut lines = output.lines().map(str::trim).filter(|line| !line.is_empty());
    let level = lines.next().ok_or("sysctl printed nothing")?;
    let pressure = match level.parse::<u32>().map_err(|_| format!("unknown memory pressure level '{level}'"))? {
        2 => Pressure::Warn,
        4 => Pressure::Critical,
        _ => Pressure::Normal,
    };
    let swap = lines.next().ok_or("sysctl printed no vm.swapusage")?;
    let size_after = |key: &str| -> Result<u64, String> {
        let rest = swap.split_once(key).map(|(_, rest)| rest.trim_start()).ok_or_else(|| format!("vm.swapusage has no '{key}': {swap}"))?;
        parse_size(rest.split_whitespace().next().unwrap_or_default()).ok_or_else(|| format!("vm.swapusage: unreadable size after '{key}': {swap}"))
    };
    let free = lines.next().ok_or("sysctl printed no kern.memorystatus_level")?;
    let free_percent = Some(free.parse::<u8>().map_err(|_| format!("unknown kern.memorystatus_level '{free}'"))?.min(100));
    Ok(MemoryReading { pressure: Some(pressure), swap_used: size_after("used =")?, swap_total: size_after("total =")?, free_percent })
}

/// `9216.00M` in bytes.
#[cfg(any(target_os = "macos", test))]
fn parse_size(raw: &str) -> Option<u64> {
    let (number, unit) = raw.split_at(raw.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(raw.len()));
    let factor = match unit {
        "" | "B" => 1.0,
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => GIB,
        _ => return None,
    };
    let value = number.parse::<f64>().ok()?;
    let bytes = (value * factor) as u64;
    Some(bytes)
}

#[cfg(any(target_os = "linux", test))]
fn parse_meminfo(meminfo: &str) -> Result<MemoryReading, String> {
    let kib = |key: &str| -> Result<u64, String> {
        meminfo
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|rest| rest.trim_start_matches(':').split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|value| value * 1024)
            .ok_or_else(|| format!("/proc/meminfo has no {key}"))
    };
    let (total, free) = (kib("SwapTotal")?, kib("SwapFree")?);
    let free_percent = match (kib("MemTotal"), kib("MemAvailable")) {
        (Ok(ram), Ok(available)) if ram > 0 => u8::try_from(u128::from(available.min(ram)) * 100 / u128::from(ram)).ok(),
        _ => None,
    };
    Ok(MemoryReading { pressure: None, swap_used: total.saturating_sub(free), swap_total: total, free_percent })
}

fn parse_fake(raw: &str) -> Result<MemoryReading, String> {
    let mut reading = MemoryReading { pressure: Some(Pressure::Normal), swap_used: 0, swap_total: 0, free_percent: None };
    for part in raw.split(',').map(str::trim).filter(|part| !part.is_empty() && *part != "normal") {
        match part.split_once('=') {
            Some(("pressure", "normal")) => reading.pressure = Some(Pressure::Normal),
            Some(("pressure", "warn")) => reading.pressure = Some(Pressure::Warn),
            Some(("pressure", "critical")) => reading.pressure = Some(Pressure::Critical),
            Some(("swap", swap)) => {
                let gb = |value: &str| value.trim().parse::<f64>().ok().filter(|value| *value >= 0.0);
                let (used, total) = swap
                    .split_once('/')
                    .and_then(|(used, total)| Some((gb(used)?, gb(total)?)))
                    .ok_or_else(|| format!("LOTAR_MEMORY_READING: bad swap '{swap}'"))?;
                let (used, total) = ((used * GIB) as u64, (total * GIB) as u64);
                reading.swap_used = used;
                reading.swap_total = total;
            }
            Some(("free", free)) => {
                let free = free.trim().parse::<u8>().ok().filter(|free| *free <= 100).ok_or_else(|| format!("LOTAR_MEMORY_READING: bad free '{free}'"))?;
                reading.free_percent = Some(free);
            }
            _ => return Err(format!("LOTAR_MEMORY_READING: unknown reading '{part}'")),
        }
    }
    Ok(reading)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AgentMemoryGateConfig {
        AgentMemoryGateConfig { enabled: Some(true), ..AgentMemoryGateConfig::default() }
    }

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + secs).unwrap()
    }

    fn reading(fake: &str) -> Result<MemoryReading, String> {
        parse_fake(fake)
    }

    #[test]
    fn pressure_warn_or_critical_holds() {
        for (fake, word) in [("pressure=warn", "warn"), ("pressure=critical", "critical")] {
            let state = decide(&GateState::default(), reading(fake), at(0), &config());
            let hold = hold_of(&state).unwrap();
            assert_eq!(hold.since, at(0));
            assert_eq!(hold.reason, format!("macOS reports memory pressure {word}"));
        }
    }

    #[test]
    fn swap_over_the_limit_holds() {
        let state = decide(&GateState::default(), reading("swap=9.4/10"), at(0), &config());
        assert_eq!(hold_of(&state).unwrap().reason, "swap 9.4 of 10.0 GB used (94%, limit 85%)");
        // At the limit, and with no swap at all, nothing waits.
        assert!(hold_of(&decide(&GateState::default(), reading("swap=8.5/10"), at(0), &config())).is_none());
        assert!(hold_of(&decide(&GateState::default(), reading("swap=0/0"), at(0), &config())).is_none());
        // The configured share wins over the default.
        let strict = AgentMemoryGateConfig { max_swap_percent: 50, ..config() };
        assert!(hold_of(&decide(&GateState::default(), reading("swap=6/10"), at(0), &strict)).is_some());
    }

    #[test]
    fn full_swap_holds_only_while_little_memory_is_free() {
        // macOS keeps pages in swap after a shortage: full swap with free
        // memory and normal pressure holds nothing (DEV-819).
        assert!(hold_of(&decide(&GateState::default(), reading("swap=3.6/4,free=63"), at(0), &config())).is_none());
        assert!(hold_of(&decide(&GateState::default(), reading("swap=3.6/4,free=20"), at(0), &config())).is_none());
        let low = decide(&GateState::default(), reading("swap=3.6/4,free=19"), at(0), &config());
        assert_eq!(hold_of(&low).unwrap().reason, "swap 3.6 of 4.0 GB used (90%, limit 85%, 19% of memory free)");
        // A held queue whose memory is free again settles and starts.
        let settling = decide(&low, reading("swap=3.6/4,free=63"), at(30), &config());
        assert!(hold_of(&settling).unwrap().reason.starts_with("memory is normal again since "));
        assert!(hold_of(&decide(&settling, reading("swap=3.6/4,free=63"), at(150), &config())).is_none());
    }

    #[test]
    fn both_readings_name_both_reasons_and_keep_the_first_since() {
        let first = decide(&GateState::default(), reading("pressure=warn"), at(0), &config());
        let both = decide(&first, reading("pressure=warn,swap=9.9/10"), at(30), &config());
        let hold = hold_of(&both).unwrap();
        assert_eq!(hold.since, at(0));
        assert_eq!(hold.reason, "macOS reports memory pressure warn; swap 9.9 of 10.0 GB used (99%, limit 85%)");
    }

    #[test]
    fn a_hold_ends_after_the_settle_time_of_normal_memory() {
        let held = decide(&GateState::default(), reading("pressure=warn"), at(0), &config());
        let settling = decide(&held, reading("normal"), at(30), &config());
        let hold = hold_of(&settling).unwrap();
        assert_eq!(hold.since, at(0));
        assert!(hold.reason.starts_with("memory is normal again since "), "{}", hold.reason);
        let still = decide(&settling, reading("normal"), at(149), &config());
        assert!(hold_of(&still).is_some(), "119 s of normal memory are not 120");
        // Short of memory again: the settle time starts over.
        let again = decide(&still, reading("swap=9/10"), at(150), &config());
        assert_eq!(again.normal_since, None);
        assert_eq!(hold_of(&again).unwrap().since, at(0));
        let settled = decide(&decide(&again, reading("normal"), at(180), &config()), reading("normal"), at(300), &config());
        assert!(hold_of(&settled).is_none());
        let ended = EndedHold { reason: "swap 9.0 of 10.0 GB used (90%, limit 85%)".to_string(), since: at(0), until: at(300) };
        assert_eq!(settled, GateState { checked_at: Some(at(300)), last_hold: Some(ended), ..GateState::default() });
    }

    #[test]
    fn a_failed_reading_holds_nothing_and_says_why() {
        let held = decide(&GateState::default(), reading("pressure=warn"), at(0), &config());
        let failed = decide(&held, reading("bogus"), at(30), &config());
        assert!(hold_of(&failed).is_none());
        assert_eq!(failed.error.as_deref(), Some("LOTAR_MEMORY_READING: unknown reading 'bogus'"));
        // The failed reading ends the hold at its own time.
        assert_eq!(failed.last_hold, Some(EndedHold { reason: "macOS reports memory pressure warn".to_string(), since: at(0), until: at(30) }));
    }

    #[test]
    fn hold_log_writes_start_and_end_once_each() {
        let mut log = HoldLog::default();
        let open = decide(&GateState::default(), reading("normal"), at(0), &config());
        assert!(log.lines(&open).is_empty());
        let held = decide(&open, reading("swap=9.5/10"), at(5), &config());
        assert_eq!(log.lines(&held), ["[lotar] memory hold started: swap 9.5 of 10.0 GB used (95%, limit 85%), since 2027-01-15T08:00:05Z"]);
        let still = decide(&held, reading("pressure=warn,swap=9.5/10"), at(35), &config());
        assert!(log.lines(&still).is_empty(), "the same hold with another reason starts once");
        let settling = decide(&still, reading("normal"), at(65), &config());
        assert!(log.lines(&settling).is_empty());
        let settled = decide(&settling, reading("normal"), at(185), &config());
        assert_eq!(
            log.lines(&settled),
            [
                "[lotar] memory hold ended: macOS reports memory pressure warn; swap 9.5 of 10.0 GB used (95%, limit 85%), since 2027-01-15T08:00:05Z, until 2027-01-15T08:03:05Z"
            ]
        );
        let later = decide(&settled, reading("normal"), at(215), &config());
        assert!(log.lines(&later).is_empty(), "an ended hold is logged once");
        // The next hold starts and ends again, here between two reads of the log.
        let next = decide(&later, reading("pressure=critical"), at(300), &config());
        let failed = decide(&next, reading("bogus"), at(310), &config());
        assert_eq!(
            log.lines(&failed),
            [
                "[lotar] memory hold started: macOS reports memory pressure critical, since 2027-01-15T08:05:00Z",
                "[lotar] memory hold ended: macOS reports memory pressure critical, since 2027-01-15T08:05:00Z, until 2027-01-15T08:05:10Z",
            ]
        );
    }

    #[test]
    fn hold_log_skips_a_hold_that_ended_before_serve_started() {
        let dir = tempfile::tempdir().unwrap();
        let tasks_dir = dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(tasks_dir.join("config.yml"), "agent:\n  memory_gate:\n    enabled: true\n").unwrap();
        let path = state_path(&tasks_dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // No state file yet: nothing to log.
        assert!(HoldLog::new(&tasks_dir).poll(&tasks_dir).is_empty());
        let ended = GateState {
            checked_at: Some(at(100)),
            last_hold: Some(EndedHold { reason: "swap full".to_string(), since: at(0), until: at(90) }),
            ..GateState::default()
        };
        std::fs::write(&path, serde_json::to_string(&ended).unwrap()).unwrap();
        let mut log = HoldLog::new(&tasks_dir);
        assert!(log.poll(&tasks_dir).is_empty());
        // A hold in effect when serve starts is logged.
        let held =
            GateState { held_since: Some(at(200)), reason: Some("swap full again".to_string()), held_reason: Some("swap full again".to_string()), ..ended };
        std::fs::write(&path, serde_json::to_string(&held).unwrap()).unwrap();
        assert_eq!(HoldLog::new(&tasks_dir).poll(&tasks_dir), ["[lotar] memory hold started: swap full again, since 2027-01-15T08:03:20Z"]);
        // Gate off: no lines.
        std::fs::write(tasks_dir.join("config.yml"), "agent:\n  memory_gate:\n    enabled: false\n").unwrap();
        assert!(HoldLog::new(&tasks_dir).poll(&tasks_dir).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unset_enabled_is_on_only_on_macos() {
        assert_eq!(AgentMemoryGateConfig::default().is_enabled(), cfg!(target_os = "macos"));
        assert!(!AgentMemoryGateConfig { enabled: Some(false), ..AgentMemoryGateConfig::default() }.is_enabled());
        assert!(AgentMemoryGateConfig { enabled: Some(true), ..AgentMemoryGateConfig::default() }.is_enabled());
    }

    #[test]
    fn parses_sysctl_output() {
        let reading = parse_sysctl("2\ntotal = 9216.00M  used = 7894.81M  free = 1321.19M  (encrypted)\n71\n").unwrap();
        assert_eq!(reading.pressure, Some(Pressure::Warn));
        assert_eq!(reading.free_percent, Some(71));
        assert_eq!(reading.swap_total, 9216 * 1024 * 1024);
        assert_eq!(reading.swap_used / (1024 * 1024), 7894);
        let none = parse_sysctl("1\ntotal = 0.00M  used = 0.00M  free = 0.00M  (encrypted)\n90\n").unwrap();
        assert_eq!(none, MemoryReading { pressure: Some(Pressure::Normal), swap_used: 0, swap_total: 0, free_percent: Some(90) });
        assert_eq!(parse_sysctl("4\ntotal = 1.00G  used = 1.00G  free = 0.00M\n5").unwrap().pressure, Some(Pressure::Critical));
        assert!(parse_sysctl("1\ntotal = 1.00G  used = 1.00G  free = 0.00M").is_err());
        assert!(parse_sysctl("1\ntotal = 1.00G  used = 1.00G  free = 0.00M\nlots").is_err());
        assert!(parse_sysctl("").is_err());
        assert!(parse_sysctl("x\ntotal = 1M used = 1M").is_err());
        assert!(parse_sysctl("1\nsomething else").is_err());
    }

    #[test]
    fn parses_proc_meminfo() {
        let reading =
            parse_meminfo("MemTotal:       16000000 kB\nMemAvailable:    4000000 kB\nSwapTotal:       2097148 kB\nSwapFree:        1048576 kB\n").unwrap();
        assert_eq!(reading, MemoryReading { pressure: None, swap_used: (2_097_148 - 1_048_576) * 1024, swap_total: 2_097_148 * 1024, free_percent: Some(25) });
        // Without MemAvailable free memory is unknown, and full swap alone holds.
        assert_eq!(parse_meminfo("SwapTotal: 2 kB\nSwapFree: 1 kB\n").unwrap().free_percent, None);
        assert!(parse_meminfo("MemTotal: 1 kB\n").is_err());
    }

    #[test]
    fn check_reuses_a_held_reading_for_30_seconds_and_status_shows_it() {
        let dir = tempfile::tempdir().unwrap();
        let tasks_dir = dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(tasks_dir.join("config.yml"), "agent:\n  memory_gate:\n    enabled: true\n").unwrap();
        let path = state_path(&tasks_dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let held = GateState { checked_at: Some(Timestamp::now()), held_since: Some(at(0)), reason: Some("swap full".to_string()), ..GateState::default() };
        std::fs::write(&path, serde_json::to_string(&held).unwrap()).unwrap();
        // The reading in this test build is normal, but the recent hold stands.
        assert_eq!(check(&tasks_dir), Some(MemoryHold { reason: "swap full".to_string(), since: at(0) }));
        assert_eq!(status(&tasks_dir).hold.unwrap().describe(), "swap full (held since 2027-01-15T08:00:00Z)");
        // Once 30 s old, the next check reads again and starts the settle time.
        let old = GateState { checked_at: Some(Timestamp::now() - SignedDuration::from_secs(31)), ..held };
        std::fs::write(&path, serde_json::to_string(&old).unwrap()).unwrap();
        let settling = check(&tasks_dir).unwrap();
        assert!(settling.reason.starts_with("memory is normal again since"), "{}", settling.reason);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn status_reads_again_once_no_process_reread_a_held_reading() {
        let dir = tempfile::tempdir().unwrap();
        let tasks_dir = dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(tasks_dir.join("config.yml"), "agent:\n  memory_gate:\n    enabled: true\n").unwrap();
        let path = state_path(&tasks_dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Hours ago a process held new jobs, then stopped reading memory.
        let hours_ago = Timestamp::now() - SignedDuration::from_hours(3);
        let old = GateState { checked_at: Some(hours_ago), held_since: Some(hours_ago), reason: Some("swap full".to_string()), ..GateState::default() };
        let saved = serde_json::to_string(&old).unwrap();
        std::fs::write(&path, &saved).unwrap();
        // The reading in this test build is normal: no hold, and the saved state stays for `check`.
        assert_eq!(status(&tasks_dir), GateStatus::default());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
        // Read less than 60 s ago, a held worker is still due to read again: the hold stands.
        let recent = GateState { checked_at: Some(Timestamp::now() - seconds(STALE_AFTER_SECS - 5)), ..old };
        std::fs::write(&path, serde_json::to_string(&recent).unwrap()).unwrap();
        assert_eq!(status(&tasks_dir).hold.unwrap().reason, "swap full");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn each_ended_hold_goes_to_the_history_once_and_holds_lists_it() {
        let dir = tempfile::tempdir().unwrap();
        let tasks_dir = dir.path().join(".tasks");
        let path = state_path(&tasks_dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Held from at(0); memory settles and the hold ends at at(200).
        let held = decide(&GateState::default(), reading("pressure=warn"), at(0), &config());
        record_if_ended(&tasks_dir, &GateState::default(), &held);
        let settling = decide(&held, reading("normal"), at(60), &config());
        let ended = decide(&settling, reading("normal"), at(200), &config());
        record_if_ended(&tasks_dir, &settling, &ended);
        // The next readings keep the ended hold in the state: not recorded again.
        let open = decide(&ended, reading("normal"), at(205), &config());
        record_if_ended(&tasks_dir, &ended, &open);
        // A second hold, still in effect.
        let held_again = decide(&open, reading("swap=9.4/10"), at(300), &config());
        std::fs::write(&path, serde_json::to_string(&held_again).unwrap()).unwrap();

        let holds = holds(&tasks_dir);
        assert_eq!(
            holds,
            vec![
                RecordedHold { reason: "macOS reports memory pressure warn".to_string(), since: at(0), until: Some(at(200)) },
                RecordedHold { reason: "swap 9.4 of 10.0 GB used (94%, limit 85%)".to_string(), since: at(300), until: None },
            ]
        );
        assert_eq!(holds[0].to_json()["seconds"], 200);
        assert!(holds[1].to_json()["until"].is_null());

        // Ended by two processes at once, the hold is listed once; a failed reading ends one too.
        let failed = decide(&held_again, Err("sysctl failed".to_string()), at(400), &config());
        record_if_ended(&tasks_dir, &held_again, &failed);
        record_if_ended(&tasks_dir, &held_again, &failed);
        std::fs::write(&path, serde_json::to_string(&failed).unwrap()).unwrap();
        let holds = super::holds(&tasks_dir);
        assert_eq!(holds.len(), 2, "{holds:?}");
        assert_eq!(holds[1].until, Some(at(400)));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(history_path(&tasks_dir));
    }

    #[test]
    fn the_history_keeps_the_last_holds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("holds.jsonl");
        for second in 0..=i64::try_from(HISTORY_LIMIT).unwrap() {
            record_ended(&path, &EndedHold { reason: "r".to_string(), since: at(second), until: at(second + 1) });
        }
        let lines: Vec<String> = std::fs::read_to_string(&path).unwrap().lines().map(str::to_string).collect();
        assert_eq!(lines.len(), HISTORY_LIMIT);
        assert!(lines[0].contains(&stamp(at(1))), "{}", lines[0]);
    }
}
