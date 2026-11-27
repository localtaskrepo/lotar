//! A second machine agent jobs run on over ssh (DEV-901): `agent.hosts` in
//! the user's home config, and a profile's `host`.
//!
//! The Mac comes first. A job of a profile with a `host` goes there only
//! when the Mac has no free slot, the host is within its `hours`, has a free
//! slot of its own (`max_jobs`), answers with the Mac's T3 Code version (a
//! `claude` or `codex` job: with the programs it runs, DEV-911), and the
//! job's expected duration ends before the window closes. Otherwise the
//! job waits for the Mac and the queue says why. While work waits for a host
//! that doesn't answer within its hours, its `wake` command runs, once per
//! ten minutes.
//!
//! What a host answered is kept for a minute and asked again in the
//! background, so placing a job never waits for ssh.

use crate::config::types::{AgentHostConfig, AgentProfileDetail, HostHours};
use jiff::{SignedDuration, Timestamp, tz::TimeZone};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// A job with no history of its profile counts as this long.
pub(crate) const DEFAULT_EXPECTED: SignedDuration = SignedDuration::from_mins(60);
/// How many of the profile's last jobs give its expected duration.
const HISTORY: usize = 10;
/// How long a host's answer counts.
const PROBE_TTL: Duration = Duration::from_secs(60);
/// How long a profile's expected duration counts before the logs are read again.
const EXPECTED_TTL: Duration = Duration::from_secs(600);
/// A host's `wake` runs at most this often.
const WAKE_EVERY: Duration = Duration::from_secs(600);

/// How lotar reaches a host: `ssh` (or `LOTAR_SSH`, which tests point at a
/// fake), batch mode, never a password prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ssh {
    pub target: String,
}

impl Ssh {
    pub fn new(target: &str) -> Self {
        Self { target: target.trim().to_string() }
    }

    pub fn program() -> PathBuf {
        std::env::var_os("LOTAR_SSH").filter(|program| !program.is_empty()).map_or_else(|| PathBuf::from("ssh"), PathBuf::from)
    }

    /// The options every connection takes.
    pub fn options() -> [&'static str; 6] {
        ["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=15"]
    }

    /// `core.sshCommand` for git, so a push or fetch takes the same route.
    pub fn git_ssh_command() -> String {
        let mut parts = vec![quote(&Self::program().to_string_lossy())];
        parts.extend(Self::options().iter().map(|option| (*option).to_string()));
        parts.join(" ")
    }

    /// The git remote of `path` on the host.
    pub fn git_url(&self, path: &Path) -> String {
        format!("{}:{}", self.target, path.display())
    }

    /// `script` run on the host by a login shell, so its PATH has `t3` and `git`.
    pub fn command(&self, script: &str) -> Command {
        let mut command = Command::new(Self::program());
        command.args(Self::options()).arg(&self.target).arg(format!("sh -lc {}", quote(script)));
        command.stdin(Stdio::null());
        command
    }

    /// Run `script` on the host; its stdout, or why it failed.
    pub fn run(&self, script: &str) -> Result<String, String> {
        self.run_with_input(script, None)
    }

    /// [`Ssh::run`], with `input` on the script's stdin.
    pub fn run_with_input(&self, script: &str, input: Option<&[u8]>) -> Result<String, String> {
        let mut command = self.command(script);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if input.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command.spawn().map_err(|err| format!("could not run {}: {err}", Self::program().display()))?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            use std::io::Write as _;
            stdin.write_all(input).map_err(|err| format!("could not send to {}: {err}", self.target))?;
        }
        let output = child.wait_with_output().map_err(|err| format!("ssh to {} broke: {err}", self.target))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = last_line(&stderr).unwrap_or("no error output");
        Err(if output.status.code() == Some(255) { format!("{} doesn't answer over ssh: {line}", self.target) } else { line.to_string() })
    }
}

/// An ssh local forward to `host:port` on the remote machine, on a free port
/// here, for as long as it lives.
pub(crate) struct Forward {
    pub port: u16,
    child: std::process::Child,
}

impl Forward {
    pub fn open(ssh: &Ssh, host: &str, port: u16) -> Result<Self, String> {
        let local =
            std::net::TcpListener::bind("127.0.0.1:0").and_then(|listener| listener.local_addr()).map_err(|err| format!("no free local port: {err}"))?.port();
        let mut child = Command::new(Ssh::program())
            .args(Ssh::options())
            .args(["-N", "-o", "ExitOnForwardFailure=yes", "-L"])
            .arg(format!("127.0.0.1:{local}:{host}:{port}"))
            .arg(&ssh.target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("could not run {}: {err}", Ssh::program().display()))?;
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(15) {
            if let Ok(Some(_)) = child.try_wait() {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read as _;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return Err(format!("the ssh forward to {} failed: {}", ssh.target, last_line(&stderr).unwrap_or("no error output")));
            }
            if std::net::TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], local)), Duration::from_millis(200)).is_ok() {
                return Ok(Self { port: local, child });
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        stop_child(&mut child);
        Err(format!("the ssh forward to {} didn't open", ssh.target))
    }
}

impl Drop for Forward {
    fn drop(&mut self) {
        stop_child(&mut self.child);
    }
}

/// SIGTERM, then SIGKILL after ten seconds.
pub(crate) fn stop_child(child: &mut std::process::Child) {
    if let Ok(Some(_)) = child.try_wait() {
        return;
    }
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(child.id()) {
        // SAFETY: signals the forward's own pid, which this process spawned and hasn't reaped.
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// T3 Code threads that run on a host, by thread id: rename, settle, stop
/// and show go there.
static THREAD_ROUTES: LazyLock<Mutex<HashMap<String, Ssh>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn route_thread(thread: &str, ssh: &Ssh) {
    if let Ok(mut routes) = THREAD_ROUTES.lock() {
        routes.insert(thread.to_string(), ssh.clone());
    }
}

/// The host a thread runs on, if it is one of a host job.
pub(crate) fn thread_route(thread: &str) -> Option<Ssh> {
    THREAD_ROUTES.lock().ok()?.get(thread).cloned()
}

/// The hosts threads run on.
pub(crate) fn routed_hosts() -> Vec<Ssh> {
    let mut hosts: Vec<Ssh> = THREAD_ROUTES.lock().map(|routes| routes.values().cloned().collect()).unwrap_or_default();
    hosts.sort_by(|a, b| a.target.cmp(&b.target));
    hosts.dedup();
    hosts
}

/// Learn the threads of saved host jobs, so a later process routes them too.
pub(crate) fn route_saved(tasks_dir: &Path, jobs: &[crate::services::agent_log_service::SavedJob]) {
    let hosted: Vec<_> = jobs.iter().filter_map(|job| Some((job.header.host.clone()?, job.status.as_ref()?.agent_session.clone()?))).collect();
    if hosted.is_empty() {
        return;
    }
    let Ok(config) = crate::config::resolution::config_for_project(tasks_dir, None) else {
        return;
    };
    for (host, thread) in hosted {
        if let Some(host) = config.agent_hosts.get(&host) {
            route_thread(&thread, &Ssh::new(&host.ssh));
        }
    }
}

/// `text` quoted for a POSIX shell.
pub(crate) fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The last line of a command's output that isn't Electron's noise.
pub(crate) fn last_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).rev().find(|line| !line.is_empty() && !line.contains(":ERROR:") && !line.contains("electron"))
}

/// Whether the window is open at `now`, and when it closes or opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Window {
    Always,
    Open { closes: Timestamp },
    Closed { opens: Timestamp },
}

/// `hours` in `zone` at `now`.
pub(crate) fn window(hours: Option<HostHours>, zone: &TimeZone, now: Timestamp) -> Window {
    let Some(HostHours { start, end }) = hours else {
        return Window::Always;
    };
    let local = now.to_zoned_datetime(zone.clone());
    let today = local.date();
    let minute = u16::try_from(local.hour()).unwrap_or(0) * 60 + u16::try_from(local.minute()).unwrap_or(0);
    let at = |days: i64, minutes: u16| -> Timestamp {
        let date = today.checked_add(jiff::Span::new().days(days)).unwrap_or(today);
        let (date, minutes) = if minutes >= 24 * 60 { (date.tomorrow().unwrap_or(date), minutes - 24 * 60) } else { (date, minutes) };
        let hour = i8::try_from(minutes / 60).unwrap_or(0);
        let minute = i8::try_from(minutes % 60).unwrap_or(0);
        date.at(hour, minute, 0, 0).to_zoned_datetime(zone.clone()).map_or(now, |zoned| zoned.timestamp())
    };
    if start < end {
        if minute < start {
            Window::Closed { opens: at(0, start) }
        } else if minute < end {
            Window::Open { closes: at(0, end) }
        } else {
            Window::Closed { opens: at(1, start) }
        }
    } else if minute >= start {
        Window::Open { closes: at(1, end) }
    } else if minute < end {
        Window::Open { closes: at(0, end) }
    } else {
        Window::Closed { opens: at(0, start) }
    }
}

/// What a host last answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Probe {
    /// Not asked yet, or being asked.
    Unknown,
    Ready,
    /// ssh doesn't get through: the host sleeps or is off.
    Down(String),
    /// It answers, but a job can't start there (version, environment).
    Refused(String),
}

/// `HH:MM` of `at` in `zone`.
fn clock(at: Timestamp, zone: &TimeZone) -> String {
    at.to_zoned_datetime(zone.clone()).strftime("%H:%M").to_string()
}

fn duration_text(duration: SignedDuration) -> String {
    let minutes = duration.as_mins().max(1);
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// Whether a job may start on the host `name` now, else why not, in the
/// order: hours, slots, the expected duration, what the host answered.
pub(crate) fn decide(name: &str, host: &AgentHostConfig, running: usize, probe: &Probe, now: Timestamp, expected: SignedDuration) -> Result<(), String> {
    let zone = host.time_zone().map_err(|err| format!("{name}: {err}"))?;
    let hours = host.host_hours().map_err(|err| format!("{name}: {err}"))?;
    match window(hours, &zone, now) {
        Window::Closed { opens } => return Err(format!("{name}: quiet hours until {}", clock(opens, &zone))),
        Window::Open { closes } if now.checked_add(expected).is_ok_and(|end| end > closes) => {
            return Err(format!("{name}: the job takes about {}, past {}", duration_text(expected), clock(closes, &zone)));
        }
        _ => {}
    }
    if running >= host.max_jobs {
        return Err(format!("{name}: {running} of max_jobs {} running", host.max_jobs));
    }
    match probe {
        Probe::Ready => Ok(()),
        Probe::Unknown => Err(format!("{name}: asking whether it answers")),
        Probe::Down(why) | Probe::Refused(why) => Err(format!("{name}: {why}")),
    }
}

/// Whether the host is within its hours now.
pub(crate) fn within_hours(host: &AgentHostConfig, now: Timestamp) -> bool {
    match (host.time_zone(), host.host_hours()) {
        (Ok(zone), Ok(hours)) => !matches!(window(hours, &zone, now), Window::Closed { .. }),
        _ => false,
    }
}

/// The `t3` CLI on this Mac, to compare versions and list environments.
#[derive(Debug, Clone)]
pub(crate) struct LocalT3 {
    pub program: PathBuf,
    pub env: Vec<(String, String)>,
}

impl LocalT3 {
    fn run(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(&self.program)
            .args(args)
            .envs(self.env.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::null())
            .output()
            .map_err(|err| format!("could not run {}: {err}", self.program.display()))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(format!("`t3 {}` failed: {}", args.join(" "), last_line(&String::from_utf8_lossy(&output.stderr)).unwrap_or("no error output")))
        }
    }
}

/// What a job needs of the host besides ssh.
#[derive(Debug, Clone)]
pub(crate) enum Check {
    /// A T3 Code thread: the host's `t3 --version` is this Mac's and its
    /// environment is reachable.
    T3(LocalT3),
    /// A `claude` or `codex` agent over ssh (DEV-911): these programs are
    /// on the host's login PATH.
    Programs(Vec<String>),
}

impl Check {
    /// Tells the cached answers of a host apart.
    fn key(&self) -> String {
        match self {
            Self::T3(_) => "t3".to_string(),
            Self::Programs(programs) => programs.join(" "),
        }
    }

    pub fn run(&self, host: &AgentHostConfig) -> Probe {
        match self {
            Self::T3(local) => probe(host, local),
            Self::Programs(programs) => probe_programs(host, programs),
        }
    }
}

/// Ask the host: ssh gets through and has `programs` on its login PATH.
pub(crate) fn probe_programs(host: &AgentHostConfig, programs: &[String]) -> Probe {
    let names: Vec<String> = programs.iter().map(|program| quote(program)).collect();
    let script = format!("for p in {}; do command -v \"$p\" >/dev/null || {{ echo \"$p is not on its PATH\" >&2; exit 3; }}; done", names.join(" "));
    match Ssh::new(&host.ssh).run(&script) {
        Ok(_) => Probe::Ready,
        Err(err) if err.contains("doesn't answer") => Probe::Down(err),
        Err(err) => Probe::Refused(err),
    }
}

/// Ask the host: ssh gets through, its `t3 --version` is this Mac's, and
/// this Mac's T3 Code reaches its environment.
pub(crate) fn probe(host: &AgentHostConfig, local: &LocalT3) -> Probe {
    let ssh = Ssh::new(&host.ssh);
    let remote = match ssh.run("t3 --version") {
        Ok(output) => last_line(&output).unwrap_or_default().to_string(),
        Err(err) if err.contains("doesn't answer") => return Probe::Down(err),
        Err(err) => return Probe::Refused(format!("`t3 --version` failed there: {err}")),
    };
    let here = match local.run(&["--version"]) {
        Ok(output) => last_line(&output).unwrap_or_default().to_string(),
        Err(err) => return Probe::Refused(err),
    };
    if remote != here {
        return Probe::Refused(format!("it runs t3 {remote}, this Mac {here}"));
    }
    match local.run(&["environment", "list", "--json"]) {
        Ok(listed) => environment_status(&listed, &host.t3_environment).map_or(Probe::Ready, Probe::Refused),
        Err(err) => Probe::Refused(err),
    }
}

/// Why the environment `wanted` (id or label) can't take a thread, from
/// `t3 environment list --json`; `None` when it is reachable.
pub(crate) fn environment_status(listed: &str, wanted: &str) -> Option<String> {
    let json = listed.lines().filter(|line| !line.contains(":ERROR:")).collect::<Vec<_>>().join("\n");
    let Ok(serde_json::Value::Array(links)) = serde_json::from_str::<serde_json::Value>(&json) else {
        return Some("`t3 environment list --json` printed no list".to_string());
    };
    let field = |link: &serde_json::Value, key: &str| link.get(key).and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    let Some(link) = links.iter().find(|link| field(link, "environmentId") == wanted).or_else(|| links.iter().find(|link| field(link, "label") == wanted))
    else {
        return Some(format!("T3 Code has no linked environment {wanted}"));
    };
    let status = field(link, "status");
    (status != "reachable").then(|| format!("its T3 Code environment {wanted} is {status}"))
}

struct Asked {
    probe: Probe,
    at: Instant,
    asking: bool,
}

static PROBES: LazyLock<Mutex<HashMap<String, Asked>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn probe_key(name: &str, host: &AgentHostConfig) -> String {
    format!("{name}\u{0}{}\u{0}", host.ssh)
}

/// What the host last answered to `check`; asks it again in the background
/// once that answer is older than a minute.
pub(crate) fn cached_probe(name: &str, host: &AgentHostConfig, check: Result<Check, String>) -> Probe {
    let check = match check {
        Ok(check) => check,
        Err(why) => return Probe::Refused(why),
    };
    let Ok(mut probes) = PROBES.lock() else {
        return Probe::Unknown;
    };
    let key = format!("{}{}", probe_key(name, host), check.key());
    let asked = probes.entry(key.clone()).or_insert(Asked { probe: Probe::Unknown, at: Instant::now(), asking: false });
    let stale = asked.probe == Probe::Unknown || asked.at.elapsed() >= PROBE_TTL;
    let answer = asked.probe.clone();
    if stale && !asked.asking {
        asked.asking = true;
        let host = host.clone();
        std::thread::spawn(move || {
            let probe = check.run(&host);
            if let Ok(mut probes) = PROBES.lock() {
                probes.insert(key, Asked { probe, at: Instant::now(), asking: false });
            }
        });
    }
    answer
}

/// Forget what the host answered, so the next placement asks it again.
pub(crate) fn forget_probe(name: &str, host: &AgentHostConfig) {
    if let Ok(mut probes) = PROBES.lock() {
        let prefix = probe_key(name, host);
        probes.retain(|key, _| !key.starts_with(&prefix));
    }
}

/// Run the host's `wake` command, unless it ran within ten minutes (in any
/// process: the stamp file is shared). Never outside its hours.
pub(crate) fn wake(name: &str, host: &AgentHostConfig, now: Timestamp) -> bool {
    let Some(command) = host.wake.as_deref().map(str::trim).filter(|command| !command.is_empty()) else {
        return false;
    };
    if !within_hours(host, now) {
        return false;
    }
    let stamp = crate::services::agent_queue_service::queue_dir().join(format!("host-{}.woke", sanitize(name)));
    let recent = std::fs::metadata(&stamp).and_then(|meta| meta.modified()).is_ok_and(|at| at.elapsed().is_ok_and(|age| age < WAKE_EVERY));
    if recent {
        return false;
    }
    if let Some(parent) = stamp.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(&stamp, now.to_string()).is_err() {
        return false;
    }
    let shell = std::env::var_os("SHELL").filter(|shell| !shell.is_empty()).unwrap_or_else(|| "/bin/sh".into());
    let spawned = Command::new(shell).arg("-lc").arg(command).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    match spawned {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// This process's jobs that run on a host: job id to host name.
static HOST_JOBS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The job runs on `host`: it holds a slot there, none on the Mac.
pub(crate) fn assign(job_id: &str, host: &str) {
    if let Ok(mut jobs) = HOST_JOBS.lock() {
        jobs.insert(job_id.to_string(), host.to_string());
    }
}

/// The job no longer runs on a host: it ended, or waits for the Mac.
pub(crate) fn release(job_id: &str) {
    if let Ok(mut jobs) = HOST_JOBS.lock() {
        jobs.remove(job_id);
    }
}

/// The host this process runs the job on.
pub(crate) fn job_host(job_id: &str) -> Option<String> {
    HOST_JOBS.lock().ok()?.get(job_id).cloned()
}

/// Whether the job, of this process or another, runs on a host.
pub(crate) fn on_host(job_id: &str, others: &[crate::services::agent_active_jobs::ActiveJob]) -> bool {
    job_host(job_id).is_some() || others.iter().any(|job| job.job_id == job_id && job.host.is_some())
}

/// Queue entries placed on a host whose job doesn't exist yet: ticket to host.
static RESERVED: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The queue placed the ticket's entry on `host`; its job takes the slot as it starts.
pub(crate) fn reserve(ticket_id: &str, host: &str) {
    if let Ok(mut reserved) = RESERVED.lock() {
        reserved.insert(ticket_id.to_string(), host.to_string());
    }
}

/// The host the queue placed the ticket's entry on, once: its job starts now.
pub(crate) fn take_reserved(ticket_id: &str) -> Option<String> {
    RESERVED.lock().ok()?.remove(ticket_id)
}

/// The jobs on each host, in every process, with the entries placed there.
pub(crate) fn running_per_host(others: &[crate::services::agent_active_jobs::ActiveJob]) -> HashMap<String, usize> {
    let mut jobs: HashMap<String, String> = others.iter().filter_map(|job| Some((job.job_id.clone(), job.host.clone()?))).collect();
    if let Ok(own) = HOST_JOBS.lock() {
        jobs.extend(own.iter().map(|(job, host)| (job.clone(), host.clone())));
    }
    if let Ok(reserved) = RESERVED.lock() {
        jobs.extend(reserved.iter().map(|(ticket, host)| (format!("reserved:{ticket}"), host.clone())));
    }
    let mut counts = HashMap::new();
    for host in jobs.into_values() {
        *counts.entry(host).or_insert(0) += 1;
    }
    counts
}

type ExpectedCache = HashMap<String, (Instant, SignedDuration)>;
static EXPECTED: LazyLock<Mutex<ExpectedCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The median of `durations`, else [`DEFAULT_EXPECTED`].
pub(crate) fn median(mut durations: Vec<SignedDuration>) -> SignedDuration {
    if durations.is_empty() {
        return DEFAULT_EXPECTED;
    }
    durations.sort();
    let middle = durations.len() / 2;
    if durations.len() % 2 == 1 { durations[middle] } else { (durations[middle - 1] + durations[middle]) / 2 }
}

/// How long the profile's jobs take: the median of its last ten finished
/// jobs, from the job logs; without any, [`DEFAULT_EXPECTED`].
pub(crate) fn expected_duration(tasks_dir: &Path, profile: &str, jobs: impl FnOnce() -> Vec<crate::services::agent_log_service::SavedJob>) -> SignedDuration {
    let key = format!("{}\u{0}{profile}", tasks_dir.display());
    if let Some((at, duration)) = EXPECTED.lock().ok().and_then(|cache| cache.get(&key).copied())
        && at.elapsed() < EXPECTED_TTL
    {
        return duration;
    }
    let mut finished: Vec<(String, SignedDuration)> = jobs()
        .into_iter()
        .filter(|job| job.header.agent_profile.as_deref().map(|name| name.trim_start_matches('@')) == Some(profile))
        .filter_map(|job| {
            let status = job.status?;
            if matches!(status.status.as_str(), "running" | "queued") {
                return None;
            }
            let start = crate::utils::time::parse_rfc3339(&job.header.created_at).ok()?;
            let end = crate::utils::time::parse_rfc3339(&status.at).ok()?;
            Some((status.at, end.duration_since(start).max(SignedDuration::ZERO)))
        })
        .collect();
    finished.sort_by(|a, b| b.0.cmp(&a.0));
    let duration = median(finished.into_iter().take(HISTORY).map(|(_, duration)| duration).collect());
    if let Ok(mut cache) = EXPECTED.lock() {
        cache.insert(key, (Instant::now(), duration));
    }
    duration
}

/// The host a job of `agent` (a profile) on the ticket may run on: the
/// profile's `host` and its config, from the ticket's project.
pub(crate) fn profile_host(tasks_dir: &Path, ticket_id: &str, agent: Option<&str>) -> Option<(String, AgentProfileDetail, String, AgentHostConfig)> {
    let profile = agent?.trim().trim_start_matches('@').to_string();
    if profile.is_empty() {
        return None;
    }
    let project = crate::storage::TaskId::parse(ticket_id).ok().map(|id| id.project);
    let config = crate::config::resolution::config_for_project(tasks_dir, project.as_deref()).ok()?;
    let detail = config.agent_profiles.get(&profile)?.clone();
    let host = detail.host.clone()?.trim().to_string();
    let host_config = config.agent_hosts.get(&host)?.clone();
    Some((profile, detail, host, host_config))
}

/// Where a job of `agent` on the ticket may run now that the Mac is full:
/// `None` when its profile has no host; else the host, or why it waits. A
/// host that doesn't answer within its hours is woken.
pub(crate) fn place(
    tasks_dir: &Path,
    ticket_id: &str,
    agent: Option<&str>,
    taken: &HashMap<String, usize>,
    jobs: impl FnOnce() -> Vec<crate::services::agent_log_service::SavedJob>,
    check: impl FnOnce(&AgentProfileDetail) -> Result<Check, String>,
) -> Option<Result<String, String>> {
    let (profile, detail, name, host) = profile_host(tasks_dir, ticket_id, agent)?;
    let others = crate::services::agent_active_jobs::list(tasks_dir);
    let running = running_per_host(&others).get(&name).copied().unwrap_or(0) + taken.get(&name).copied().unwrap_or(0);
    let now = Timestamp::now();
    let expected = expected_duration(tasks_dir, &profile, jobs);
    let probe = if within_hours(&host, now) && running < host.max_jobs { cached_probe(&name, &host, check(&detail)) } else { Probe::Unknown };
    let decided = decide(&name, &host, running, &probe, now, expected);
    if matches!(probe, Probe::Down(_)) && wake(&name, &host, now) {
        return Some(Err(format!("{name}: doesn't answer; woken at {}", clock(now, &host.time_zone().unwrap_or(TimeZone::UTC)))));
    }
    Some(decided.map(|()| name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(hours: Option<&str>) -> AgentHostConfig {
        AgentHostConfig {
            ssh: "dyson".to_string(),
            t3_environment: "dyson".to_string(),
            max_jobs: 2,
            hours: hours.map(str::to_string),
            timezone: Some("America/Los_Angeles".to_string()),
            wake: None,
        }
    }

    fn at(local: &str) -> Timestamp {
        let zone = TimeZone::get("America/Los_Angeles").unwrap_or(TimeZone::UTC);
        local.parse::<jiff::civil::DateTime>().ok().and_then(|time| time.to_zoned_datetime(zone).ok()).map_or(Timestamp::UNIX_EPOCH, |zoned| zoned.timestamp())
    }

    #[test]
    fn a_job_waits_outside_the_hours_and_says_until_when() {
        let decided = decide("dyson", &host(Some("09:00-21:00")), 0, &Probe::Ready, at("2026-11-26T22:30"), SignedDuration::from_mins(30));
        assert_eq!(decided, Err("dyson: quiet hours until 09:00".to_string()));
        let early = decide("dyson", &host(Some("09:00-21:00")), 0, &Probe::Ready, at("2026-11-26T07:00"), SignedDuration::from_mins(30));
        assert_eq!(early, Err("dyson: quiet hours until 09:00".to_string()));
    }

    #[test]
    fn a_job_that_would_end_past_the_window_stays_on_the_mac() {
        let host = host(Some("09:00-21:00"));
        let late = decide("dyson", &host, 0, &Probe::Ready, at("2026-11-26T20:15"), SignedDuration::from_mins(50));
        assert_eq!(late, Err("dyson: the job takes about 50m, past 21:00".to_string()));
        assert_eq!(decide("dyson", &host, 0, &Probe::Ready, at("2026-11-26T20:15"), SignedDuration::from_mins(40)), Ok(()));
    }

    #[test]
    fn a_full_host_says_how_many_run() {
        let decided = decide("dyson", &host(None), 2, &Probe::Ready, at("2026-11-26T12:00"), SignedDuration::from_mins(10));
        assert_eq!(decided, Err("dyson: 2 of max_jobs 2 running".to_string()));
    }

    #[test]
    fn a_host_that_doesnt_answer_or_differs_keeps_the_job() {
        let now = at("2026-11-26T12:00");
        let short = SignedDuration::from_mins(10);
        assert_eq!(decide("dyson", &host(None), 0, &Probe::Unknown, now, short), Err("dyson: asking whether it answers".to_string()));
        let refused = Probe::Refused("it runs t3 0.1.12, this Mac 0.1.13".to_string());
        assert_eq!(decide("dyson", &host(None), 0, &refused, now, short), Err("dyson: it runs t3 0.1.12, this Mac 0.1.13".to_string()));
    }

    #[test]
    fn a_window_past_midnight_is_open_on_both_sides() {
        let zone = TimeZone::get("America/Los_Angeles").unwrap_or(TimeZone::UTC);
        let hours = HostHours::parse("22:00-06:00").ok();
        assert!(matches!(window(hours, &zone, at("2026-11-26T23:00")), Window::Open { closes } if closes == at("2026-11-27T06:00")));
        assert!(matches!(window(hours, &zone, at("2026-11-27T05:00")), Window::Open { closes } if closes == at("2026-11-27T06:00")));
        assert!(matches!(window(hours, &zone, at("2026-11-27T12:00")), Window::Closed { opens } if opens == at("2026-11-27T22:00")));
    }

    #[test]
    fn hours_parse_only_as_hh_mm_ranges() {
        assert_eq!(HostHours::parse("09:00-21:00"), Ok(HostHours { start: 540, end: 1260 }));
        assert_eq!(HostHours::parse("00:00-24:00"), Ok(HostHours { start: 0, end: 1440 }));
        assert!(HostHours::parse("9-21").is_err());
        assert!(HostHours::parse("09:00-09:00").is_err());
        assert!(HostHours::parse("09:61-10:00").is_err());
    }

    #[test]
    fn the_expected_duration_is_the_median_else_an_hour() {
        assert_eq!(median(Vec::new()), DEFAULT_EXPECTED);
        assert_eq!(median(vec![SignedDuration::from_mins(30), SignedDuration::from_mins(10), SignedDuration::from_mins(20)]), SignedDuration::from_mins(20));
        assert_eq!(median(vec![SignedDuration::from_mins(10), SignedDuration::from_mins(20)]), SignedDuration::from_mins(15));
    }

    #[test]
    fn the_environment_must_be_reachable() {
        let listed = r#"[{"environmentId":"df0b","label":"dyson","status":"unreachable"},{"environmentId":"aa","label":"other","status":"reachable"}]"#;
        assert_eq!(environment_status(listed, "dyson"), Some("its T3 Code environment dyson is unreachable".to_string()));
        assert_eq!(environment_status(listed, "aa"), None);
        assert_eq!(environment_status(listed, "nope"), Some("T3 Code has no linked environment nope".to_string()));
    }

    /// The host's `wake` runs once per ten minutes within its hours, never outside.
    #[test]
    fn wake_runs_within_hours_and_at_most_every_ten_minutes() {
        let dir = tempfile::tempdir().expect("tempdir");
        // SAFETY: nextest runs each test in its own process.
        unsafe {
            std::env::set_var("LOTAR_AGENT_QUEUE_DIR", dir.path().join("queue"));
            std::env::set_var("SHELL", "/bin/sh");
        }
        let woke = dir.path().join("woke");
        let mut dyson = host(Some("09:00-21:00"));
        dyson.wake = Some(format!("echo woke >> {}", woke.display()));
        assert!(!wake("dyson", &dyson, at("2026-11-26T22:00")), "quiet hours");
        assert!(wake("dyson", &dyson, at("2026-11-26T10:00")));
        assert!(!wake("dyson", &dyson, at("2026-11-26T10:05")), "within ten minutes");
        let started = Instant::now();
        // The shell makes the file before `echo` writes it.
        while std::fs::read_to_string(&woke).ok().as_deref() != Some("woke\n") && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(std::fs::read_to_string(&woke).ok().as_deref(), Some("woke\n"));
        assert!(!wake("nyx", &host(None), at("2026-11-26T10:00")), "a host without wake");
    }

    /// The fake ssh: `t3 --version` prints `remote`; without one, or for
    /// another command, it doesn't connect (exit 255).
    fn fake_ssh(dir: &Path, remote: Option<&str>) -> LocalT3 {
        use std::os::unix::fs::PermissionsExt as _;
        let write = |name: &str, text: String| {
            let path = dir.join(name);
            std::fs::write(&path, text).expect("script");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
            path
        };
        let answer = remote.map_or_else(|| "echo 'connect: down' >&2; exit 255".to_string(), |remote| format!("echo {remote}"));
        let ssh = write("ssh", format!("#!/bin/sh\ncase \"$*\" in *\"t3 --version\"*) {answer} ;; *) echo 'connect: down' >&2; exit 255 ;; esac\n"));
        // SAFETY: nextest runs each test in its own process.
        unsafe { std::env::set_var("LOTAR_SSH", ssh) };
        let t3 = write("t3", "#!/bin/sh\ncase \"$1\" in --version) echo 0.1.13 ;; *) echo '[{\"environmentId\":\"df0b\",\"label\":\"dyson\",\"status\":\"reachable\"}]' ;; esac\n".to_string());
        LocalT3 { program: t3, env: Vec::new() }
    }

    #[test]
    fn the_probe_compares_t3_versions_and_sees_a_host_that_is_down() {
        let dir = tempfile::tempdir().expect("tempdir");
        let local = fake_ssh(dir.path(), Some("0.1.13"));
        assert_eq!(probe(&host(None), &local), Probe::Ready);
        let local = fake_ssh(dir.path(), Some("0.1.12"));
        assert_eq!(probe(&host(None), &local), Probe::Refused("it runs t3 0.1.12, this Mac 0.1.13".to_string()));
        let local = fake_ssh(dir.path(), None);
        assert_eq!(probe(&host(None), &local), Probe::Down("dyson doesn't answer over ssh: connect: down".to_string()));
    }

    /// DEV-911: a claude/codex job needs its programs there, not T3 Code.
    #[test]
    fn the_probe_of_a_headless_agent_looks_for_its_programs() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("tempdir");
        let ssh = dir.path().join("ssh");
        // Runs the script here, with only `python3` and `claude` on PATH.
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        for program in ["python3", "claude"] {
            std::fs::write(bin.join(program), "#!/bin/sh\n").expect("program");
            std::fs::set_permissions(bin.join(program), std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        let fake = format!("#!/bin/sh\nfor last; do :; done\nPATH={} exec /bin/sh -c \"/bin/sh -c ${{last#sh -lc }}\"\n", bin.display());
        std::fs::write(&ssh, fake).expect("ssh");
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        // SAFETY: nextest runs each test in its own process.
        unsafe { std::env::set_var("LOTAR_SSH", &ssh) };
        let programs = |list: &[&str]| Check::Programs(list.iter().map(|program| (*program).to_string()).collect());
        assert_eq!(programs(&["python3", "claude"]).run(&host(None)), Probe::Ready);
        assert_eq!(programs(&["python3", "codex"]).run(&host(None)), Probe::Refused("codex is not on its PATH".to_string()));
    }

    #[test]
    fn scripts_are_quoted_for_the_remote_shell() {
        assert_eq!(quote("it's"), r"'it'\''s'");
    }
}
