//! The machine's check lock (DEV-784): one heavy check run at a time across
//! jobs, worktrees and processes. Several jobs that run the full suites at
//! once load the machine far beyond its cores, and timing tests then fail.
//!
//! The lock is a listening socket on `127.0.0.1:PORT`, so the OS frees it
//! when its holder ends or is killed. The repository's scripts take the
//! same port (`scripts/check-lock.mjs`); a holder answers a connection with
//! one line that says who it is, which a waiting run logs. A run that holds
//! the lock gives its commands `HELD_ENV=1`, so the scripts they run don't
//! wait for it again.
//!
//! Every merge job takes it (DEV-797): lotar's own merge around a round's
//! checks, and a merge agent from before it starts until the job ends, also
//! when lotar's merge hands off to it in the middle of its checks.

use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The port DEV-586's smoke lock held, so older smoke runs wait as well.
pub(super) const PORT: u16 = 47_586;
/// Another port for the lock, where tests hold their own.
pub(super) const PORT_ENV: &str = "LOTAR_CHECK_LOCK_PORT";
/// Set for the commands of a run that holds the lock.
pub(super) const HELD_ENV: &str = "LOTAR_CHECK_LOCK_HELD";
/// How often a waiting run tries again.
const RETRY: Duration = Duration::from_millis(500);
/// How long a waiting run waits for the holder's line.
const HOLDER_TIMEOUT: Duration = Duration::from_secs(1);

/// The held lock; dropping it frees the port.
pub(super) struct CheckLock {
    /// How long the run waited for it, `None` when it was free.
    pub waited: Option<Duration>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
    /// Kept here when the listener can't serve the holder's line.
    _listener: Option<TcpListener>,
}

impl Drop for CheckLock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

pub(super) enum Acquired {
    Held(CheckLock),
    /// The port can't be opened for another reason than being in use.
    Unavailable(String),
    Cancelled,
}

/// The lock's port: `LOTAR_CHECK_LOCK_PORT` when set, else [`PORT`].
pub(super) fn port() -> u16 {
    std::env::var(PORT_ENV).ok().and_then(|port| port.parse().ok()).unwrap_or(PORT)
}

/// What a job's wait for the lock ended with.
pub(super) enum ForJob {
    /// The lock, and whom the job waited for and how long (`None`: it was free).
    Held(CheckLock, Option<(String, Duration)>),
    /// The lotar run that started this one holds it (`LOTAR_CHECK_LOCK_HELD=1`).
    HeldAbove,
    /// The port can't be opened: the job goes on without the lock (why).
    Unavailable(String),
    Cancelled,
}

/// Take the lock for job `job_id` as `holder`, waiting while another run
/// holds it; `progress` logs the wait. Cancelling the job ends the wait,
/// also `lotar agent cancel` from another process.
pub(super) fn for_job(job_id: &str, tasks_dir: &Path, holder: &str, progress: impl Fn(&str)) -> ForJob {
    if std::env::var(HELD_ENV).is_ok_and(|held| held == "1") {
        return ForJob::HeldAbove;
    }
    let port = port();
    let mut held_by = String::new();
    let waiting = |line: &str| {
        progress(&format!("Waiting for the machine's check lock (port {port}), held by {line}"));
        held_by = line.to_string();
    };
    match acquire(port, holder, waiting, || super::wait_cancelled(job_id, tasks_dir)) {
        Acquired::Held(lock) => {
            let waited = lock.waited.map(|waited| {
                progress(&format!("Took the check lock after {}", duration_text(waited)));
                (held_by, waited)
            });
            ForJob::Held(lock, waited)
        }
        Acquired::Unavailable(err) => ForJob::Unavailable(format!("port {port} can't be opened ({err})")),
        Acquired::Cancelled => ForJob::Cancelled,
    }
}

/// Take the lock on `port`, waiting while another run holds it.
/// `waiting` gets the holder's line once; `cancelled` ends the wait.
pub(super) fn acquire(port: u16, holder: &str, mut waiting: impl FnMut(&str), mut cancelled: impl FnMut() -> bool) -> Acquired {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let started = Instant::now();
    let mut told = false;
    loop {
        match TcpListener::bind(addr) {
            Ok(listener) => {
                let waited = told.then(|| started.elapsed());
                return Acquired::Held(serve(listener, format!("{holder}\n"), waited));
            }
            Err(err) if err.kind() == ErrorKind::AddrInUse => {}
            Err(err) => return Acquired::Unavailable(err.to_string()),
        }
        if !told {
            waiting(&holder_of(addr.into()));
            told = true;
        }
        if cancelled() {
            return Acquired::Cancelled;
        }
        std::thread::sleep(RETRY);
    }
}

/// Answer each connection with `line` until the lock is dropped.
fn serve(listener: TcpListener, line: String, waited: Option<Duration>) -> CheckLock {
    let stop = Arc::new(AtomicBool::new(false));
    if listener.set_nonblocking(true).is_err() {
        return CheckLock { waited, stop, server: None, _listener: Some(listener) };
    }
    let stopped = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !stopped.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.write_all(line.as_bytes());
                }
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    });
    CheckLock { waited, stop, server: Some(server), _listener: None }
}

/// The holder's line, or a generic name when it sends none (an older smoke run).
fn holder_of(addr: SocketAddr) -> String {
    let line = TcpStream::connect_timeout(&addr, HOLDER_TIMEOUT)
        .ok()
        .and_then(|stream| {
            stream.set_read_timeout(Some(HOLDER_TIMEOUT)).ok()?;
            let mut line = String::new();
            let _ = stream.take(512).read_to_string(&mut line);
            Some(line.trim().to_string())
        })
        .unwrap_or_default();
    if line.is_empty() { "another check run".to_string() } else { line }
}

/// `3 min` or `40 s`.
pub(super) fn duration_text(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 { format!("{secs} s") } else { format!("{} min", secs.div_ceil(60)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn free_port() -> u16 {
        TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).and_then(|listener| listener.local_addr()).map(|addr| addr.port()).expect("a free port")
    }

    #[test]
    fn two_runs_never_hold_the_lock_at_once() {
        let port = free_port();
        let spans = Arc::new(Mutex::new(Vec::<(Instant, Instant)>::new()));
        let waits = Arc::new(Mutex::new(Vec::<String>::new()));
        let runs: Vec<_> = (0..3)
            .map(|run| {
                let (spans, waits) = (Arc::clone(&spans), Arc::clone(&waits));
                std::thread::spawn(move || {
                    let holder = format!("run {run} (pid 1)");
                    let Acquired::Held(lock) = acquire(port, &holder, |line| waits.lock().expect("waits").push(line.to_string()), || false) else {
                        panic!("run {run} didn't get the lock");
                    };
                    let start = Instant::now();
                    std::thread::sleep(Duration::from_millis(300));
                    let end = Instant::now();
                    drop(lock);
                    spans.lock().expect("spans").push((start, end));
                })
            })
            .collect();
        for run in runs {
            run.join().expect("run");
        }
        let mut spans = spans.lock().expect("spans").clone();
        spans.sort();
        assert_eq!(spans.len(), 3);
        assert!(spans.windows(2).all(|pair| pair[0].1 <= pair[1].0), "the runs overlapped: {spans:?}");
        let waits = waits.lock().expect("waits");
        assert!(!waits.is_empty(), "a run waited");
        assert!(waits.iter().all(|line| line.starts_with("run ") && line.ends_with("(pid 1)")), "the waiting runs name the holder: {waits:?}");
    }

    #[test]
    fn a_cancelled_wait_gives_up_and_a_dropped_lock_is_free_again() {
        let port = free_port();
        let Acquired::Held(lock) = acquire(port, "first", |_| {}, || false) else { panic!("the free lock") };
        assert_eq!(lock.waited, None);
        let mut told = None;
        assert!(matches!(acquire(port, "second", |line| told = Some(line.to_string()), || true), Acquired::Cancelled));
        assert_eq!(told.as_deref(), Some("first"));
        drop(lock);
        assert!(matches!(acquire(port, "third", |_| {}, || false), Acquired::Held(_)));
    }

    #[test]
    fn a_holder_that_sends_no_line_is_another_check_run() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let addr = listener.local_addr().expect("addr");
        let mut told = None;
        assert!(matches!(acquire(addr.port(), "me", |line| told = Some(line.to_string()), || true), Acquired::Cancelled));
        assert_eq!(told.as_deref(), Some("another check run"));
    }

    #[test]
    fn durations_read_in_seconds_then_minutes() {
        assert_eq!(duration_text(Duration::from_secs(40)), "40 s");
        assert_eq!(duration_text(Duration::from_secs(61)), "2 min");
    }
}
