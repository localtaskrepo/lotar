//! The agent queue starts no new job while the machine is short of memory
//! (DEV-623): macOS reports memory pressure, or swap is nearly full.
//!
//! The last reading and the hold it found are kept next to the queue file,
//! so the queue worker, `lotar serve` and `lotar agent queue` in another
//! process see the same hold.

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
    hold_of(&next)
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
    let reading = match reading {
        Ok(reading) => reading,
        Err(error) => return GateState { checked_at: Some(now), error: Some(error), ..GateState::default() },
    };
    if let Some(reason) = short_of_memory(&reading, config) {
        return GateState { checked_at: Some(now), held_since: previous.held_since.or(Some(now)), normal_since: None, reason: Some(reason), error: None };
    }
    let Some(held_since) = previous.held_since else {
        return GateState { checked_at: Some(now), ..GateState::default() };
    };
    let normal_since = previous.normal_since.unwrap_or(now);
    if now - normal_since >= seconds(config.settle_seconds) {
        return GateState { checked_at: Some(now), ..GateState::default() };
    }
    let reason = format!("memory is normal again since {}; held jobs start after {} s of normal memory", stamp(normal_since), config.settle_seconds);
    GateState { checked_at: Some(now), held_since: Some(held_since), normal_since: Some(normal_since), reason: Some(reason), error: None }
}

fn short_of_memory(reading: &MemoryReading, config: &AgentMemoryGateConfig) -> Option<String> {
    let mut reasons = Vec::new();
    match reading.pressure {
        Some(Pressure::Warn) => reasons.push("macOS reports memory pressure warn".to_string()),
        Some(Pressure::Critical) => reasons.push("macOS reports memory pressure critical".to_string()),
        Some(Pressure::Normal) | None => {}
    }
    let limit = u128::from(config.max_swap_percent);
    if reading.swap_total > 0 && u128::from(reading.swap_used) * 100 > limit * u128::from(reading.swap_total) {
        let percent = (u128::from(reading.swap_used) * 100 + u128::from(reading.swap_total) / 2) / u128::from(reading.swap_total);
        let (used, total) = (reading.swap_used as f64 / GIB, reading.swap_total as f64 / GIB);
        reasons.push(format!("swap {used:.1} of {total:.1} GB used ({percent}%, limit {limit}%)"));
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
/// `normal`, or `pressure=warn,swap=9.5/10` (swap in GB).
fn read_memory() -> Result<MemoryReading, String> {
    if let Ok(fake) = std::env::var("LOTAR_MEMORY_READING") {
        return parse_fake(&fake);
    }
    read_system()
}

#[cfg(target_os = "macos")]
fn read_system() -> Result<MemoryReading, String> {
    let output = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "kern.memorystatus_vm_pressure_level", "vm.swapusage"])
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

/// `sysctl -n kern.memorystatus_vm_pressure_level vm.swapusage`: the level
/// (1 normal, 2 warn, 4 critical), then
/// `total = 9216.00M  used = 7894.81M  free = 1321.19M  (encrypted)`.
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
    Ok(MemoryReading { pressure: Some(pressure), swap_used: size_after("used =")?, swap_total: size_after("total =")? })
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
    Ok(MemoryReading { pressure: None, swap_used: total.saturating_sub(free), swap_total: total })
}

fn parse_fake(raw: &str) -> Result<MemoryReading, String> {
    let mut reading = MemoryReading { pressure: Some(Pressure::Normal), swap_used: 0, swap_total: 0 };
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
        assert_eq!(settled, GateState { checked_at: Some(at(300)), ..GateState::default() });
    }

    #[test]
    fn a_failed_reading_holds_nothing_and_says_why() {
        let held = decide(&GateState::default(), reading("pressure=warn"), at(0), &config());
        let failed = decide(&held, reading("bogus"), at(30), &config());
        assert!(hold_of(&failed).is_none());
        assert_eq!(failed.error.as_deref(), Some("LOTAR_MEMORY_READING: unknown reading 'bogus'"));
    }

    #[test]
    fn unset_enabled_is_on_only_on_macos() {
        assert_eq!(AgentMemoryGateConfig::default().is_enabled(), cfg!(target_os = "macos"));
        assert!(!AgentMemoryGateConfig { enabled: Some(false), ..AgentMemoryGateConfig::default() }.is_enabled());
        assert!(AgentMemoryGateConfig { enabled: Some(true), ..AgentMemoryGateConfig::default() }.is_enabled());
    }

    #[test]
    fn parses_sysctl_output() {
        let reading = parse_sysctl("2\ntotal = 9216.00M  used = 7894.81M  free = 1321.19M  (encrypted)\n").unwrap();
        assert_eq!(reading.pressure, Some(Pressure::Warn));
        assert_eq!(reading.swap_total, 9216 * 1024 * 1024);
        assert_eq!(reading.swap_used / (1024 * 1024), 7894);
        let none = parse_sysctl("1\ntotal = 0.00M  used = 0.00M  free = 0.00M  (encrypted)\n").unwrap();
        assert_eq!(none, MemoryReading { pressure: Some(Pressure::Normal), swap_used: 0, swap_total: 0 });
        assert_eq!(parse_sysctl("4\ntotal = 1.00G  used = 1.00G  free = 0.00M").unwrap().pressure, Some(Pressure::Critical));
        assert!(parse_sysctl("").is_err());
        assert!(parse_sysctl("x\ntotal = 1M used = 1M").is_err());
        assert!(parse_sysctl("1\nsomething else").is_err());
    }

    #[test]
    fn parses_proc_meminfo() {
        let reading = parse_meminfo("MemTotal:       16000000 kB\nSwapTotal:       2097148 kB\nSwapFree:        1048576 kB\n").unwrap();
        assert_eq!(reading, MemoryReading { pressure: None, swap_used: (2_097_148 - 1_048_576) * 1024, swap_total: 2_097_148 * 1024 });
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
}
