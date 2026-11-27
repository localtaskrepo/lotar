//! A job on an agent host (DEV-901). Before its thread starts, the ticket's
//! branch is pushed over ssh to a clone at the main checkout's absolute path
//! on the host, and a worktree added there at the Mac worktree's path, with
//! the Claude settings and the lotar MCP server the thread reads. The MCP
//! server runs on this Mac: the host's `.mcp.json` starts a bridge to a
//! socket that an ssh connection lotar holds for the job forwards here. The
//! tickets stay on the Mac.
//!
//! A `claude` or `codex` job (DEV-911) runs its agent over ssh, in the same
//! worktree: [`Hosted::agent_table`] makes ssh the agent's program, with
//! [`GUARD`] around the agent there. Its MCP server is the same bridge, given
//! with the ACP session instead of a `.mcp.json`.
//!
//! When the thread ends, what it left uncommitted is committed there, the
//! branch fetched, fast-forwarded into the Mac worktree and its new commits
//! signed again here. Then the host's worktree goes. A thread that waits for
//! an answer brings its commits so far back the same way and keeps its
//! worktree and MCP connection there for the answer; a headless agent's turn
//! is over then, so all of it comes back (DEV-931).

use crate::config::types::AgentHostConfig;
use crate::services::agent_hosts::{self, Check, Probe, Ssh, quote, stop_child};
use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The files the thread reads on the host, kept out of its commits.
const SETTINGS_FILE: &str = ".claude/settings.local.json";
const MCP_FILE: &str = ".mcp.json";

/// Connects the agent's stdio to the socket the ssh connection forwards to
/// this Mac's lotar MCP server.
const BRIDGE: &str = r"import os, socket, sys, threading
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
def up():
    while True:
        data = os.read(0, 65536)
        if not data:
            s.shutdown(socket.SHUT_WR)
            return
        s.sendall(data)
threading.Thread(target=up, daemon=True).start()
while True:
    data = s.recv(65536)
    if not data:
        break
    os.write(1, data)
";

/// Seconds the agent's process group has on the host after SIGTERM before
/// SIGKILL.
const GRACE_SECONDS: u32 = 10;

/// Runs a `claude`/`codex` agent on the host (DEV-911): `python3 -c GUARD
/// <grace> <agent command>`. The agent gets a process group of its own and
/// the guard passes stdin and stdout through. When stdin closes (the ssh
/// connection broke, or lotar ended the session) or the guard gets SIGHUP
/// or SIGTERM, the group gets SIGTERM, then SIGKILL after `grace` seconds;
/// so does what is left of it when the agent exits by itself.
pub(super) const GUARD: &str = r"import os, signal, subprocess, sys, threading, time
grace = float(sys.argv[1])
agent = subprocess.Popen(sys.argv[2:], stdin=subprocess.PIPE, stdout=subprocess.PIPE, start_new_session=True)
lock, done = threading.Lock(), []
def stop(*_):
    with lock:
        if done:
            return
        done.append(1)
        try:
            os.killpg(agent.pid, signal.SIGTERM)
        except OSError:
            return
        end = time.monotonic() + grace
        while time.monotonic() < end:
            try:
                os.killpg(agent.pid, 0)
            except OSError:
                return
            time.sleep(0.1)
        try:
            os.killpg(agent.pid, signal.SIGKILL)
        except OSError:
            pass
def later(*_):
    threading.Thread(target=stop).start()
signal.signal(signal.SIGHUP, later)
signal.signal(signal.SIGTERM, later)
def up():
    while True:
        try:
            data = os.read(0, 65536)
        except OSError:
            data = b''
        if not data:
            break
        try:
            agent.stdin.write(data)
            agent.stdin.flush()
        except OSError:
            return
    try:
        agent.stdin.close()
    except OSError:
        pass
    stop()
def down():
    while True:
        data = os.read(agent.stdout.fileno(), 65536)
        if not data:
            return
        try:
            os.write(1, data)
        except OSError:
            stop()
            return
threading.Thread(target=up, daemon=True).start()
out = threading.Thread(target=down, daemon=True)
out.start()
code = agent.wait()
stop()
out.join(5)
sys.exit(code if code >= 0 else 128 - code)
";

/// A job's run on a host, from its setup until its commits are back.
pub(super) struct Hosted {
    pub name: String,
    pub host: AgentHostConfig,
    pub ssh: Ssh,
    /// It runs a T3 Code thread there; else a headless agent over ssh.
    pub thread: bool,
    repo_root: PathBuf,
    worktree: PathBuf,
    branch: String,
    head_before: String,
    bridge: Option<Bridge>,
}

/// What the host's thread gets: the profile's deny rules and env.
pub(super) struct ThreadSettings {
    pub deny: Vec<String>,
    pub env: BTreeMap<String, String>,
    /// Give it lotar's MCP server (the profile's `mcp.lotar`).
    pub lotar_mcp: bool,
}

/// The lotar MCP server the bridge runs per connection, and what a broken
/// connection to the host does.
pub(super) struct McpServer {
    pub program: PathBuf,
    pub env: BTreeMap<String, String>,
    pub on_lost: Box<dyn Fn(&str) + Send>,
}

/// The job's checkout on the Mac, as the host gets it.
pub(super) struct Checkout<'a> {
    pub repo_root: &'a Path,
    pub worktree: &'a Path,
    pub branch: &'a str,
}

impl Hosted {
    /// Ready the host for the job: it answers `check` (this Mac's T3 Code,
    /// or a `claude`/`codex` agent's programs), its clone has the branch,
    /// its worktree a thread's settings. Any failure leaves the job to the
    /// Mac, with why.
    pub fn set_up(
        job_id: &str,
        name: &str,
        host: &AgentHostConfig,
        checkout: &Checkout<'_>,
        settings: &ThreadSettings,
        mcp: Option<McpServer>,
        check: Result<Check, String>,
    ) -> Result<(Self, Vec<String>), String> {
        let Checkout { repo_root, worktree, branch } = *checkout;
        let check = check?;
        let thread = matches!(check, Check::T3(_));
        match check.run(host) {
            Probe::Ready => {}
            Probe::Down(why) | Probe::Refused(why) => return Err(why),
            Probe::Unknown => return Err("it didn't answer".to_string()),
        }
        let ssh = Ssh::new(&host.ssh);
        let head_before = git(worktree, &["rev-parse", "HEAD"])?;
        let mut hosted = Self {
            name: name.to_string(),
            host: host.clone(),
            ssh,
            thread,
            repo_root: repo_root.to_path_buf(),
            worktree: worktree.to_path_buf(),
            branch: branch.to_string(),
            head_before,
            bridge: None,
        };
        let mut notes = Vec::new();
        let (repo, tree) = (hosted.remote(repo_root), hosted.remote(worktree));
        hosted.ssh.run(&format!("test -d {repo}/.git || {{ mkdir -p {repo} && git init -q {repo}; }}"))?;
        if let Some(note) = hosted.clear_earlier_worktree()? {
            notes.push(note);
        }
        hosted.push()?;
        let parent = worktree.parent().map_or_else(|| "/".to_string(), |parent| hosted.remote(parent));
        hosted.ssh.run(&format!("mkdir -p {parent} && git -C {repo} worktree prune && git -C {repo} worktree add -q {tree} {}", quote(branch)))?;
        if thread {
            match hosted.ssh.run(&format!("t3 project add {repo}")) {
                Ok(_) => {}
                Err(err) if err.contains("already") || err.contains("exists") => {}
                Err(err) => return Err(format!("`t3 project add` failed there: {err}")),
            }
        }
        let socket = match mcp.filter(|_| settings.lotar_mcp) {
            Some(server) => {
                let bridge = Bridge::open(&hosted.ssh, job_id, server)?;
                let socket = bridge.remote.clone();
                hosted.bridge = Some(bridge);
                Some(socket)
            }
            None => None,
        };
        // A headless agent takes its rules and MCP servers with its ACP session.
        if thread {
            hosted.write_settings(settings, socket.as_deref())?;
        }
        notes.push(format!("Runs on {name}: pushed {branch} to {} there, in the worktree {} at the same path.", repo_root.display(), worktree.display()));
        Ok((hosted, notes))
    }

    /// lotar's MCP server for a headless agent there (DEV-911): the bridge to
    /// the socket the job's ssh connection forwards to this Mac.
    pub fn lotar_mcp_server(&self) -> Option<harnessmux::McpServer> {
        let socket = self.bridge.as_ref()?.remote.clone();
        Some(harnessmux::McpServer::Stdio { command: "python3".to_string(), args: vec!["-c".to_string(), BRIDGE.to_string(), socket], env: BTreeMap::new() })
    }

    /// The job's agent table with `agent` run on the host over ssh
    /// (DEV-911), in the job's worktree there, with its table env and `env`. A profile's own
    /// `command` (already in `table`) runs as it is; else the agent's pinned
    /// adapter through npx.
    pub fn agent_table(&self, mut table: harnessmux::AgentTable, agent: &str, env: &BTreeMap<String, String>) -> Result<harnessmux::AgentTable, String> {
        let mut spec = table.get(agent).map_err(|err| err.to_string())?.clone();
        // The agent's env from the table and the profile go there; the Mac's
        // own (PATH, LOTAR_*) stays with the local ssh.
        let mut remote_env = std::mem::take(&mut spec.env);
        remote_env.extend(env.iter().map(|(key, value)| (key.clone(), value.clone())));
        spec.command = self.agent_over_ssh(&remote_agent_command(&spec), &remote_env);
        spec.adapter = None;
        table.insert(spec).map_err(|err| err.to_string())?;
        Ok(table)
    }

    /// `ssh ... <target> -- sh -lc 'cd <worktree> && exec env <env> python3 -c <guard> 10 <command>'`.
    fn agent_over_ssh(&self, command: &[String], env: &BTreeMap<String, String>) -> Vec<String> {
        let mut script = format!("cd {} && exec env", self.remote(&self.worktree));
        for (key, value) in env {
            script.push_str(&format!(" {}", quote(&format!("{key}={value}"))));
        }
        script.push_str(&format!(" python3 -c {} {GRACE_SECONDS}", quote(GUARD)));
        for part in command {
            script.push_str(&format!(" {}", quote(part)));
        }
        let mut line =
            vec![Ssh::program().to_string_lossy().into_owned(), "-T".into(), "-o".into(), "ControlMaster=no".into(), "-o".into(), "ControlPath=none".into()];
        line.extend(Ssh::options().iter().map(|option| (*option).to_string()));
        line.extend([self.ssh.target.clone(), "--".to_string(), format!("sh -lc {}", quote(&script))]);
        line
    }

    fn remote(&self, path: &Path) -> String {
        quote(&path.to_string_lossy())
    }

    fn git_ssh(&self) -> String {
        format!("core.sshCommand={}", Ssh::git_ssh_command())
    }

    /// An earlier job's worktree at the path: it goes when it holds no work
    /// the Mac lacks, else the job stays here.
    fn clear_earlier_worktree(&self) -> Result<Option<String>, String> {
        let (repo, tree) = (self.remote(&self.repo_root), self.remote(&self.worktree));
        let found = self.ssh.run(&format!("if test -e {tree}; then git -C {tree} status --porcelain; echo '--lotar--'; git -C {tree} rev-parse HEAD; fi"))?;
        let Some((status, head)) = found.split_once("--lotar--") else {
            return Ok(None);
        };
        if !status.trim().is_empty() {
            return Err(format!("{} has uncommitted work of an earlier job in {}; commit or remove it there first", self.name, self.worktree.display()));
        }
        let head = head.trim();
        if !head.is_empty() && !self.mac_has(head) {
            self.fetch()?;
            if !self.mac_has(head) {
                return Err(format!("the worktree {} on {} has commits the Mac lacks ({head}); bring them back first", self.worktree.display(), self.name));
            }
        }
        self.ssh.run(&format!("git -C {repo} worktree remove --force {tree}"))?;
        Ok(Some(format!("Removed the worktree of an earlier job on {}: its commits are on the Mac.", self.name)))
    }

    /// The commit is in the Mac branch's history.
    fn mac_has(&self, commit: &str) -> bool {
        git(&self.worktree, &["merge-base", "--is-ancestor", commit, "HEAD"]).is_ok()
    }

    fn push(&self) -> Result<(), String> {
        let refspec = format!("HEAD:refs/heads/{}", self.branch);
        git(&self.worktree, &["-c", &self.git_ssh(), "push", "--force", "--quiet", &self.ssh.git_url(&self.repo_root), &refspec])
            .map(|_| ())
            .map_err(|err| format!("pushing {} to {} failed: {err}", self.branch, self.name))
    }

    fn fetched_ref(&self) -> String {
        format!("refs/lotar-host/{}", self.branch)
    }

    fn fetch(&self) -> Result<(), String> {
        let refspec = format!("+refs/heads/{}:{}", self.branch, self.fetched_ref());
        git(&self.worktree, &["-c", &self.git_ssh(), "fetch", "--quiet", &self.ssh.git_url(&self.repo_root), &refspec])
            .map(|_| ())
            .map_err(|err| format!("fetching {} from {} failed: {err}", self.branch, self.name))
    }

    /// The Claude settings and `.mcp.json` the thread reads, out of its commits.
    fn write_settings(&self, settings: &ThreadSettings, socket: Option<&str>) -> Result<(), String> {
        let mut claude = serde_json::json!({});
        crate::services::t3_claude_settings::merge(&mut claude, &settings.deny, &settings.env)?;
        let mut files = vec![];
        if let Some(socket) = socket {
            claude["enabledMcpjsonServers"] = serde_json::json!(["lotar"]);
            let mcp = serde_json::json!({"mcpServers": {"lotar": {"command": "python3", "args": ["-c", BRIDGE, socket]}}});
            files.push((MCP_FILE, mcp));
        }
        files.push((SETTINGS_FILE, claude));
        let tree = self.remote(&self.worktree);
        let exclude = format!("\"$(git -C {tree} rev-parse --path-format=absolute --git-common-dir)/info/exclude\"");
        for (file, content) in files {
            let path = self.remote(&self.worktree.join(file));
            let text = serde_json::to_string_pretty(&content).map_err(|err| err.to_string())?;
            let script = format!(
                "mkdir -p \"$(dirname {path})\" && cat > {path} && mkdir -p \"$(dirname {exclude})\" && {{ grep -qxF {file} {exclude} 2>/dev/null || echo {file} >> {exclude}; }}",
                file = quote(&format!("/{file}"))
            );
            self.ssh.run_with_input(&script, Some(format!("{text}\n").as_bytes())).map_err(|err| format!("writing {file} on {} failed: {err}", self.name))?;
        }
        Ok(())
    }

    /// Whether the connection to the MCP server broke while the job ran.
    pub fn lost(&self) -> Option<String> {
        self.bridge.as_ref().and_then(Bridge::lost)
    }

    /// A thread (`thread`) or headless agent (DEV-931) that already runs on
    /// the host in a worktree an earlier job set up, taken over after lotar
    /// restarted: no MCP connection, and its commits come back when it ends.
    pub fn attach(name: &str, host: &AgentHostConfig, checkout: &Checkout<'_>, thread: bool) -> Result<Self, String> {
        let Checkout { repo_root, worktree, branch } = *checkout;
        Ok(Self {
            name: name.to_string(),
            host: host.clone(),
            ssh: Ssh::new(&host.ssh),
            thread,
            repo_root: repo_root.to_path_buf(),
            worktree: worktree.to_path_buf(),
            branch: branch.to_string(),
            head_before: git(worktree, &["rev-parse", "HEAD"])?,
            bridge: None,
        })
    }

    /// Bring the thread's commits back: commit its leftovers there (only
    /// after a run that ended well), fetch, fast-forward the Mac worktree
    /// and sign the new commits here. The host's worktree goes once all is
    /// back; else it stays and the error says why.
    pub fn bring_back(&mut self, ticket_id: &str, commit_leftovers: bool) -> Result<Vec<String>, String> {
        self.close();
        let mut notes = self.collect(ticket_id, commit_leftovers)?;
        let tree = self.remote(&self.worktree);
        if self.ssh.run(&format!("git -C {tree} status --porcelain")).is_ok_and(|status| status.trim().is_empty()) {
            let repo = self.remote(&self.repo_root);
            let removed = self.ssh.run(&format!("git -C {repo} worktree remove --force {tree} && git -C {repo} branch -q -D {}", quote(&self.branch)));
            if let Err(err) = removed {
                notes.push(format!("Could not remove the worktree on {}: {err}", self.name));
            }
        } else {
            notes.push(format!("Kept the worktree {} on {}: it holds uncommitted changes.", self.worktree.display(), self.name));
        }
        Ok(notes)
    }

    /// The thread waits for an answer: its commits so far come back and are
    /// signed here, and the host's branch moves to those signed commits, so
    /// the answered thread goes on from them. Its worktree, uncommitted
    /// changes and MCP connection stay there.
    pub fn bring_back_so_far(&mut self, ticket_id: &str) -> Result<Vec<String>, String> {
        let before = self.head_before.clone();
        let notes = self.collect(ticket_id, false)?;
        if self.head_before != before {
            let side = format!("refs/lotar-mac/{}", self.branch);
            git(&self.worktree, &["-c", &self.git_ssh(), "push", "--force", "--quiet", &self.ssh.git_url(&self.repo_root), &format!("HEAD:{side}")])
                .map_err(|err| format!("pushing the signed commits back to {} failed: {err}", self.name))?;
            let (tree, side) = (self.remote(&self.worktree), quote(&side));
            self.ssh
                .run(&format!("git -C {tree} reset -q --soft {side} && git -C {tree} update-ref -d {side}"))
                .map_err(|err| format!("moving {}'s {} to the signed commits failed: {err}", self.name, self.branch))?;
        }
        Ok(notes)
    }

    /// Commit the leftovers (when asked), fetch the host's branch,
    /// fast-forward the Mac worktree to it and sign its new commits here.
    fn collect(&mut self, ticket_id: &str, commit_leftovers: bool) -> Result<Vec<String>, String> {
        let mut notes = Vec::new();
        let tree = self.remote(&self.worktree);
        let status = self.ssh.run(&format!("git -C {tree} status --porcelain"))?;
        if !status.trim().is_empty() && commit_leftovers {
            let who = |key: &str| git(&self.worktree, &["config", key]).unwrap_or_default();
            let message = format!("{ticket_id} Changes the agent left uncommitted on {}", self.name);
            self.ssh.run(&format!(
                "git -C {tree} add -A && git -C {tree} -c commit.gpgsign=false -c user.name={} -c user.email={} commit -q -m {}",
                quote(&who("user.name")),
                quote(&who("user.email")),
                quote(&message)
            ))?;
            notes.push(format!("Committed what the agent left uncommitted on {}.", self.name));
        }
        self.fetch()?;
        let fetched = self.fetched_ref();
        git(&self.worktree, &["merge", "--ff-only", "--quiet", &fetched])
            .map_err(|err| format!("the Mac worktree {} can't fast-forward to {}'s {}: {err}", self.worktree.display(), self.name, self.branch))?;
        let head = git(&self.worktree, &["rev-parse", "HEAD"])?;
        if head != self.head_before {
            let count = git(&self.worktree, &["rev-list", "--count", &format!("{}..HEAD", self.head_before)]).unwrap_or_default();
            git(&self.worktree, &["rebase", "--quiet", "--force-rebase", &self.head_before]).map_err(|err| {
                let _ = git(&self.worktree, &["rebase", "--abort"]);
                format!("signing the commits from {} again failed: {err}", self.name)
            })?;
            notes.push(format!("Brought {count} commits back from {} and signed them here.", self.name));
            self.head_before = git(&self.worktree, &["rev-parse", "HEAD"])?;
        }
        let _ = git(&self.worktree, &["update-ref", "-d", &fetched]);
        Ok(notes)
    }

    /// Close the MCP connection; the host's worktree stays (an answer or a
    /// later job continues there).
    pub fn close(&mut self) {
        if let Some(mut bridge) = self.bridge.take() {
            bridge.close(&self.ssh);
        }
    }
}

impl Drop for Hosted {
    fn drop(&mut self) {
        self.close();
    }
}

/// The agent's command on the host: the table's command when it has no
/// pinned adapter (a profile's own `command`), else the adapter through npx,
/// which the host has no harnessmux to install.
pub(super) fn remote_agent_command(spec: &harnessmux::AgentSpec) -> Vec<String> {
    let rest = spec.command.iter().skip(1).cloned();
    match &spec.adapter {
        Some(adapter) => ["npx", "--yes"]
            .into_iter()
            .map(str::to_string)
            .chain([format!("--package={}@{}", adapter.npm, adapter.version), "--".to_string(), adapter.bin.clone()])
            .chain(rest)
            .collect(),
        None => spec.command.clone(),
    }
}

/// The programs a headless agent needs on the host: the guard's python3,
/// its command's program and, for the adapter, the agent's own CLI.
pub(super) fn remote_programs(spec: &harnessmux::AgentSpec) -> Vec<String> {
    let mut programs = vec!["python3".to_string()];
    programs.extend(remote_agent_command(spec).into_iter().next());
    if spec.adapter.is_some() {
        programs.extend(spec.requires.clone());
    }
    programs.dedup();
    programs
}

/// `git <args>` in `dir`: its trimmed stdout, or its error.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = crate::utils::git::git_command(dir)
        .args(args)
        .timeout(crate::utils::git::GIT_TIMEOUT * 6)
        .output()
        .map_err(|err| format!("could not run git: {err}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(agent_hosts::last_line(&stderr).unwrap_or("git failed").to_string())
    }
}

/// The socket a host's thread reaches this Mac's lotar MCP server through:
/// a listener here that starts the server per connection, and the ssh
/// connection that forwards the host's socket to it.
struct Bridge {
    remote: String,
    local: PathBuf,
    ssh: Arc<Mutex<Child>>,
    stop: Arc<AtomicBool>,
    lost: Arc<Mutex<Option<String>>>,
    servers: Arc<Mutex<Vec<Child>>>,
}

impl Bridge {
    fn open(ssh: &Ssh, job_id: &str, server: McpServer) -> Result<Self, String> {
        let short: String = job_id.chars().filter(char::is_ascii_alphanumeric).rev().take(24).collect::<String>().chars().rev().collect();
        let local = std::env::temp_dir().join(format!("lotar-mcp-{short}.sock"));
        let _ = std::fs::remove_file(&local);
        let listener = UnixListener::bind(&local).map_err(|err| format!("could not listen on {}: {err}", local.display()))?;
        listener.set_nonblocking(true).map_err(|err| err.to_string())?;
        let remote = format!("/tmp/lotar-mcp-{short}.sock");
        ssh.run(&format!("rm -f {}", quote(&remote)))?;
        let mut child = Command::new(Ssh::program())
            .args(Ssh::options())
            .args(["-N", "-o", "ExitOnForwardFailure=yes", "-o", "StreamLocalBindUnlink=yes", "-R"])
            .arg(format!("{remote}:{}", local.display()))
            .arg(&ssh.target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("could not run {}: {err}", Ssh::program().display()))?;
        let started = Instant::now();
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                let _ = std::fs::remove_file(&local);
                return Err(format!("the ssh forward for lotar's MCP server ended ({status})"));
            }
            if ssh.run(&format!("test -S {}", quote(&remote))).is_ok() {
                break;
            }
            if started.elapsed() > Duration::from_secs(15) {
                stop_child(&mut child);
                let _ = std::fs::remove_file(&local);
                return Err("the ssh forward for lotar's MCP server didn't open".to_string());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let McpServer { program, env, on_lost } = server;
        let server = (program, env);
        let stop = Arc::new(AtomicBool::new(false));
        let servers = Arc::new(Mutex::new(Vec::new()));
        let (accepting, kept) = (Arc::clone(&stop), Arc::clone(&servers));
        std::thread::spawn(move || {
            while !accepting.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if let Some(child) = serve(stream, &server)
                            && let Ok(mut kept) = kept.lock()
                        {
                            kept.push(child);
                        }
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        });
        let child = Arc::new(Mutex::new(child));
        let lost = Arc::new(Mutex::new(None));
        let (watching, watched, noted, target) = (Arc::clone(&stop), Arc::clone(&child), Arc::clone(&lost), ssh.target.clone());
        // A connection that drops while the job runs stops its thread.
        std::thread::spawn(move || {
            while !watching.load(Ordering::SeqCst) {
                let ended = watched.lock().map_or(true, |mut child| !matches!(child.try_wait(), Ok(None)));
                if ended && !watching.load(Ordering::SeqCst) {
                    let why = format!("the ssh connection to {target} for lotar's MCP server broke");
                    if let Ok(mut lost) = noted.lock() {
                        *lost = Some(why.clone());
                    }
                    on_lost(&why);
                    return;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        Ok(Self { remote, local, ssh: child, stop, lost, servers })
    }

    fn lost(&self) -> Option<String> {
        self.lost.lock().ok()?.clone()
    }

    fn close(&mut self, ssh: &Ssh) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut child) = self.ssh.lock() {
            stop_child(&mut child);
        }
        if let Ok(mut servers) = self.servers.lock() {
            for server in servers.iter_mut() {
                stop_child(server);
            }
        }
        let _ = std::fs::remove_file(&self.local);
        let _ = ssh.run(&format!("rm -f {}", quote(&self.remote)));
    }
}

/// Run lotar's MCP server for one connection.
fn serve(stream: UnixStream, (program, env): &(PathBuf, BTreeMap<String, String>)) -> Option<Child> {
    let _ = stream.set_nonblocking(false);
    let mut child = Command::new(program).arg("mcp").envs(env).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let (mut stdin, mut stdout) = (child.stdin.take()?, child.stdout.take()?);
    let mut reader = stream.try_clone().ok()?;
    let mut writer = stream;
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 65536];
        while let Ok(read) = reader.read(&mut buffer) {
            if read == 0 || stdin.write_all(&buffer[..read]).is_err() {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 65536];
        while let Ok(read) = stdout.read(&mut buffer) {
            if read == 0 || writer.write_all(&buffer[..read]).is_err() {
                break;
            }
        }
        let _ = writer.shutdown(std::net::Shutdown::Both);
    });
    Some(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::agent_hosts::LocalT3;
    use std::os::unix::fs::PermissionsExt as _;

    /// A Mac checkout and a "host" in one temp folder: the fake ssh runs each
    /// command here, with `<root>/mac` paths rewritten to `<root>/host`, logs
    /// it to `<root>/ssh.log`, and fakes `-R` (a socket at the forwarded
    /// path) and `-L` (a TCP forward). The host has a `t3` whose version is in
    /// `<root>/host-version` and T3 Code's state in `<root>/host-t3`.
    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        local: LocalT3,
    }

    fn script(path: &Path, text: &str) {
        std::fs::write(path, text).expect("write script");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    fn run_git(dir: &Path, args: &[&str]) -> String {
        git(dir, args).unwrap_or_else(|err| panic!("git {args:?}: {err}"))
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical");
        for sub in ["mac", "host", "bin", "host-bin"] {
            std::fs::create_dir_all(root.join(sub)).expect("mkdir");
        }
        script(
            &root.join("bin/ssh"),
            &format!(
                r#"#!/bin/sh
remote= local=
while [ $# -gt 0 ]; do
  case "$1" in
    -R) remote="$2"; shift 2 ;;
    -L) local="$2"; shift 2 ;;
    -o) shift 2 ;;
    -N|-T) shift ;;
    *) break ;;
  esac
done
shift
[ "$1" = "--" ] && shift
if [ -n "$remote" ]; then
  exec python3 -c 'import os, socket, sys, time
os.path.exists(sys.argv[1]) and os.remove(sys.argv[1])
s = socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.listen()
time.sleep(600)' "${{remote%%:*}}"
fi
if [ -n "$local" ]; then
  exec python3 -c 'import socket, sys, threading
_, port, host, to = sys.argv[1].split(":")
server = socket.socket(); server.bind(("127.0.0.1", int(port))); server.listen()
def pipe(a, b):
    while data := a.recv(65536):
        b.sendall(data)
    b.shutdown(socket.SHUT_WR)
while True:
    a, _ = server.accept()
    b = socket.create_connection((host, int(to)))
    threading.Thread(target=pipe, args=(a, b), daemon=True).start()
    threading.Thread(target=pipe, args=(b, a), daemon=True).start()' "$local"
fi
cmd="$*"
printf '%s\n' "$cmd" >> {root}/ssh.log
case "$cmd" in "sh -lc "*) cmd="sh -c ${{cmd#sh -lc }}" ;; esac
cmd=$(printf '%s' "$cmd" | sed "s#{root}/mac#{root}/host#g")
T3CODE_HOME="{root}/host-t3" PATH="{root}/host-bin:$PATH" exec sh -c "$cmd"
"#,
                root = root.display()
            ),
        );
        std::fs::write(root.join("host-version"), "0.1.13\n").expect("version");
        script(
            &root.join("host-bin/t3"),
            &format!(
                "#!/bin/sh\ncase \"$1 $2 $3\" in --version*) cat {}/host-version ;; project*) echo added ;; 'auth session issue') echo '{{\"sessionId\":\"s1\",\"token\":\"secret\"}}' ;; esac\n",
                root.display()
            ),
        );
        script(
            &root.join("bin/t3"),
            "#!/bin/sh\ncase \"$1\" in --version) echo 0.1.13 ;; environment) echo '[{\"environmentId\":\"df0b\",\"label\":\"dyson\",\"status\":\"reachable\"}]' ;; esac\n",
        );
        // A signer git accepts, so the test sees which commits were signed here.
        script(
            &root.join("bin/gpg"),
            "#!/bin/sh\ncat > /dev/null\necho '[GNUPG:] SIG_CREATED ' >&2\nprintf -- '-----BEGIN PGP SIGNATURE-----\\n\\nfake\\n-----END PGP SIGNATURE-----\\n'\n",
        );
        std::fs::write(root.join("gitconfig"), "[user]\n\tname = Mac\n\temail = mac@example.com\n[init]\n\tdefaultBranch = main\n").expect("gitconfig");
        // SAFETY: nextest runs each test in its own process.
        unsafe {
            std::env::set_var("LOTAR_SSH", root.join("bin/ssh"));
            std::env::set_var("GIT_CONFIG_GLOBAL", root.join("gitconfig"));
            std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        }
        let repo = root.join("mac/repo");
        std::fs::create_dir_all(&repo).expect("repo");
        run_git(&repo, &["init", "-q"]);
        std::fs::write(repo.join("README"), "one\n").expect("readme");
        run_git(&repo, &["add", "README"]);
        run_git(&repo, &["commit", "-q", "-m", "first"]);
        run_git(&repo, &["config", "commit.gpgsign", "true"]);
        run_git(&repo, &["config", "gpg.program", &root.join("bin/gpg").to_string_lossy()]);
        run_git(&repo, &["worktree", "add", "-q", "-b", "agent/DEV-1", &root.join("mac/wt").to_string_lossy()]);
        let local = LocalT3 { program: root.join("bin/t3"), env: Vec::new() };
        Fixture { _dir: dir, root, local }
    }

    fn dyson() -> AgentHostConfig {
        AgentHostConfig { ssh: "dyson".to_string(), t3_environment: "dyson".to_string(), max_jobs: 1, hours: None, timezone: None, wake: None }
    }

    fn set_up(fixture: &Fixture) -> Result<(Hosted, Vec<String>), String> {
        let (repo, worktree) = (fixture.root.join("mac/repo"), fixture.root.join("mac/wt"));
        let settings = ThreadSettings {
            deny: vec!["mcp__claude_ai_Gmail".to_string()],
            env: BTreeMap::from([("LOTAR_DEFAULT_REPORTER".to_string(), "claude".to_string())]),
            lotar_mcp: false,
        };
        let checkout = Checkout { repo_root: &repo, worktree: &worktree, branch: "agent/DEV-1" };
        Hosted::set_up("job-1", "dyson", &dyson(), &checkout, &settings, None, Ok(Check::T3(fixture.local.clone())))
    }

    #[test]
    fn a_headless_agents_host_job_gets_the_worktree_without_thread_settings() {
        let fixture = fixture();
        let (repo, worktree) = (fixture.root.join("mac/repo"), fixture.root.join("mac/wt"));
        let settings = ThreadSettings { deny: vec!["mcp__claude_ai_Gmail".to_string()], env: BTreeMap::new(), lotar_mcp: false };
        let checkout = Checkout { repo_root: &repo, worktree: &worktree, branch: "agent/DEV-1" };
        script(&fixture.root.join("host-bin/t3"), &format!("#!/bin/sh\ntouch {}/t3-called\n", fixture.root.display()));
        let (hosted, _) = Hosted::set_up("job-1", "dyson", &dyson(), &checkout, &settings, None, Ok(Check::Programs(vec!["sh".to_string()]))).expect("set up");
        assert!(!hosted.thread);
        let host_tree = fixture.root.join("host/wt");
        assert_eq!(run_git(&host_tree, &["rev-parse", "HEAD"]), run_git(&worktree, &["rev-parse", "HEAD"]));
        assert!(!host_tree.join(SETTINGS_FILE).exists() && !host_tree.join(".mcp.json").exists());
        assert!(!fixture.root.join("t3-called").exists(), "no `t3 project add` for a headless agent");

        let refused = Hosted::set_up("job-2", "dyson", &dyson(), &checkout, &settings, None, Ok(Check::Programs(vec!["no-such-agent".to_string()])));
        assert_eq!(refused.err().as_deref(), Some("no-such-agent is not on its PATH"));
    }

    fn headless(fixture: &Fixture, command: &[&str], env: &[(&str, &str)]) -> Vec<String> {
        let (repo, worktree) = (fixture.root.join("mac/repo"), fixture.root.join("mac/wt"));
        let settings = ThreadSettings { deny: Vec::new(), env: BTreeMap::new(), lotar_mcp: false };
        let checkout = Checkout { repo_root: &repo, worktree: &worktree, branch: "agent/DEV-1" };
        let (hosted, _) = Hosted::set_up("job-1", "dyson", &dyson(), &checkout, &settings, None, Ok(Check::Programs(vec!["sh".to_string()]))).expect("set up");
        let mut table = crate::services::agent_runner::agent_table().expect("table");
        let mut spec = table.get("claude").expect("claude").clone();
        if !command.is_empty() {
            spec.command = command.iter().map(|part| (*part).to_string()).collect();
            spec.adapter = None;
        }
        table.insert(spec).expect("insert");
        let env = env.iter().map(|(key, value)| ((*key).to_string(), (*value).to_string())).collect();
        hosted.agent_table(table, "claude", &env).expect("table").get("claude").expect("claude").command.clone()
    }

    #[test]
    fn a_headless_agent_runs_over_ssh_in_the_worktree_there_with_the_profile_env() {
        let fixture = fixture();
        let line = headless(&fixture, &[], &[("CLAUDE_CODE_EFFORT", "high")]);
        assert_eq!(line[1..6], ["-T", "-o", "ControlMaster=no", "-o", "ControlPath=none"]);
        let script = line.last().expect("script");
        let target = line.iter().position(|part| part == "dyson").expect("target");
        assert_eq!(line[target + 1], "--");
        assert!(script.starts_with("sh -lc "), "{script}");
        assert!(script.contains(&format!("cd '\\''{}'\\'' && exec env", fixture.root.join("mac/wt").display())), "{script}");
        assert!(script.contains("CLAUDE_CODE_EFFORT=high"), "{script}");
        assert!(
            script.contains("npx") && script.contains("--package=@agentclientprotocol/claude-agent-acp@") && script.contains("claude-agent-acp"),
            "{script}"
        );
        assert!(!script.contains("PATH=") && !script.contains("LOTAR_"), "{script}");

        let spec = crate::services::agent_runner::agent_table().expect("table").get("claude").expect("claude").clone();
        assert_eq!(remote_programs(&spec), ["python3", "npx", "claude"]);
        let own = headless(&fixture, &["my-claude", "--fast"], &[]);
        let script = own.last().expect("script");
        assert!(script.contains("my-claude") && script.contains("--fast") && !script.contains("npx"), "{script}");
    }

    /// The launch line, run through the fake ssh: the guard passes stdin and
    /// stdout through, in the host's worktree.
    #[test]
    fn the_guard_passes_the_agents_stdio_through() {
        let fixture = fixture();
        let line = headless(&fixture, &["sh", "-c", "pwd; echo \"$GREETING\"; head -n 1"], &[("GREETING", "hi there")]);
        let mut child = Command::new(&line[0]).args(&line[1..]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn");
        // The connection stays open until the agent is done: closing it stops the agent.
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(b"from lotar\n").expect("write");
        let mut stdout = child.stdout.take().expect("stdout");
        let mut text = String::new();
        stdout.read_to_string(&mut text).expect("read");
        drop(stdin);
        let output = child.wait_with_output().expect("wait");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(text, format!("{}\nhi there\nfrom lotar\n", fixture.root.join("host/wt").display()));
    }

    fn alive(pid: &str) -> bool {
        Command::new("kill").args(["-0", pid.trim()]).stderr(Stdio::null()).status().is_ok_and(|status| status.success())
    }

    fn guard(args: &[&str]) -> Child {
        Command::new("python3").args(["-c", GUARD, "1"]).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().expect("python3")
    }

    fn wait_for(path: &Path) -> String {
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            if let Ok(text) = std::fs::read_to_string(path)
                && text.ends_with('\n')
            {
                return text;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("{} never came", path.display());
    }

    /// A closed connection (stdin) stops the agent's whole process group:
    /// SIGTERM, then SIGKILL for what ignores it.
    #[test]
    fn the_guard_stops_the_agents_group_when_the_connection_closes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pids = dir.path().join("pids");
        let program = format!("trap '' TERM; sleep 60 & echo $! > {}; wait", pids.display());
        let mut child = guard(&["sh", "-c", &program]);
        let pid = wait_for(&pids);
        assert!(alive(&pid));
        drop(child.stdin.take());
        let status = child.wait().expect("wait");
        assert!(!status.success(), "the agent was killed");
        let end = Instant::now() + Duration::from_secs(5);
        while alive(&pid) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(&pid), "the agent's child that ignored SIGTERM got SIGKILL");
    }

    /// An agent that exits leaves nothing behind there.
    #[test]
    fn the_guard_stops_what_the_agent_leaves_running() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pids = dir.path().join("pids");
        let program = format!("sleep 60 & echo $! > {}; exit 3", pids.display());
        let mut child = guard(&["sh", "-c", &program]);
        // Its connection stays open: `wait` alone would close stdin first.
        let _stdin = child.stdin.take();
        assert_eq!(child.wait().expect("wait").code(), Some(3));
        let pid = wait_for(&pids);
        let end = Instant::now() + Duration::from_secs(5);
        while alive(&pid) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(&pid));
    }

    fn commit_on_host(fixture: &Fixture, file: &str) {
        let tree = fixture.root.join("host/wt");
        std::fs::write(tree.join(file), "from the host\n").expect("write");
        run_git(&tree, &["add", file]);
        run_git(&tree, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", &format!("DEV-1 add {file}")]);
    }

    #[test]
    fn a_host_job_gets_the_branch_and_settings_and_its_commits_come_back_signed() {
        let fixture = fixture();
        let (mut hosted, notes) = set_up(&fixture).expect("set up");
        assert!(notes.iter().any(|note| note.starts_with("Runs on dyson")), "{notes:?}");
        let host_tree = fixture.root.join("host/wt");
        assert_eq!(run_git(&host_tree, &["rev-parse", "HEAD"]), run_git(&fixture.root.join("mac/wt"), &["rev-parse", "HEAD"]));
        let settings = std::fs::read_to_string(host_tree.join(SETTINGS_FILE)).expect("settings");
        assert!(settings.contains("mcp__claude_ai_Gmail") && settings.contains("LOTAR_DEFAULT_REPORTER"), "{settings}");
        assert_eq!(run_git(&host_tree, &["status", "--porcelain"]), "", "the settings stay out of commits");

        commit_on_host(&fixture, "work.txt");
        std::fs::write(host_tree.join("left.txt"), "uncommitted\n").expect("leftover");
        let notes = hosted.bring_back("DEV-1", true).expect("bring back");
        assert!(notes.iter().any(|note| note.contains("Brought 2 commits back from dyson")), "{notes:?}");

        let mac = fixture.root.join("mac/wt");
        assert!(mac.join("work.txt").is_file() && mac.join("left.txt").is_file());
        for commit in ["HEAD", "HEAD~1"] {
            assert!(run_git(&mac, &["cat-file", "commit", commit]).contains("gpgsig"), "{commit} is signed here");
        }
        assert!(!host_tree.exists(), "the host's worktree goes");
        assert!(git(&fixture.root.join("host/repo"), &["rev-parse", "--verify", "refs/heads/agent/DEV-1"]).is_err(), "and its branch");
        assert!(git(&mac, &["rev-parse", "--verify", "refs/lotar-host/agent/DEV-1"]).is_err(), "the fetched ref goes");
    }

    #[test]
    fn a_thread_that_waits_brings_its_commits_back_and_the_answered_rest_follows() {
        let fixture = fixture();
        let (mut hosted, _) = set_up(&fixture).expect("set up");
        let (mac, host_tree) = (fixture.root.join("mac/wt"), fixture.root.join("host/wt"));
        commit_on_host(&fixture, "asked.txt");
        std::fs::write(host_tree.join("draft.txt"), "uncommitted\n").expect("draft");
        let notes = hosted.bring_back_so_far("DEV-1").expect("so far");
        assert!(notes.iter().any(|note| note.contains("Brought 1 commits back from dyson")), "{notes:?}");
        assert!(mac.join("asked.txt").is_file() && !mac.join("draft.txt").exists(), "commits come back, not the draft");
        assert!(run_git(&mac, &["cat-file", "commit", "HEAD"]).contains("gpgsig"), "signed here");
        let signed = run_git(&mac, &["rev-parse", "HEAD"]);
        assert_eq!(run_git(&host_tree, &["rev-parse", "HEAD"]), signed, "the host's branch moves to the signed commits");
        assert_eq!(run_git(&host_tree, &["status", "--porcelain"]), "?? draft.txt", "the draft stays there");
        assert!(git(&fixture.root.join("host/repo"), &["rev-parse", "--verify", "refs/lotar-mac/agent/DEV-1"]).is_err(), "the side ref goes");

        // The owner answers; the thread goes on and ends.
        commit_on_host(&fixture, "answered.txt");
        let notes = hosted.bring_back("DEV-1", true).expect("bring back");
        assert!(notes.iter().any(|note| note.contains("Brought 2 commits back from dyson")), "{notes:?}");
        assert!(mac.join("answered.txt").is_file() && mac.join("draft.txt").is_file());
        assert_eq!(run_git(&mac, &["rev-parse", "HEAD~2"]), signed, "the commits up to the question keep their signed hashes");
        for commit in ["HEAD", "HEAD~1"] {
            assert!(run_git(&mac, &["cat-file", "commit", commit]).contains("gpgsig"), "{commit} is signed here");
        }
        assert!(!host_tree.exists(), "the host's worktree goes");
    }

    #[test]
    fn a_thread_taken_over_after_a_restart_brings_its_commits_back() {
        let fixture = fixture();
        let (mut hosted, _) = set_up(&fixture).expect("set up");
        commit_on_host(&fixture, "asked.txt");
        hosted.bring_back_so_far("DEV-1").expect("so far");
        drop(hosted);
        commit_on_host(&fixture, "answered.txt");
        let (repo, worktree) = (fixture.root.join("mac/repo"), fixture.root.join("mac/wt"));
        let checkout = Checkout { repo_root: &repo, worktree: &worktree, branch: "agent/DEV-1" };
        let mut attached = Hosted::attach("dyson", &dyson(), &checkout, true).expect("attach");
        let notes = attached.bring_back("DEV-1", true).expect("bring back");
        assert!(notes.iter().any(|note| note.contains("Brought 1 commits back from dyson")), "{notes:?}");
        assert!(worktree.join("answered.txt").is_file());
        assert!(!fixture.root.join("host/wt").exists());
    }

    #[test]
    fn a_new_job_after_a_waiting_thread_takes_the_hosts_worktree_over() {
        let fixture = fixture();
        let (mut hosted, _) = set_up(&fixture).expect("set up");
        commit_on_host(&fixture, "asked.txt");
        hosted.bring_back_so_far("DEV-1").expect("so far");
        drop(hosted);
        let (_, notes) = set_up(&fixture).expect("its commits are on the Mac");
        assert!(notes.iter().any(|note| note.starts_with("Removed the worktree of an earlier job")), "{notes:?}");
    }

    #[test]
    fn a_host_with_another_t3_keeps_the_job_on_the_mac() {
        let fixture = fixture();
        std::fs::write(fixture.root.join("host-version"), "0.1.12\n").expect("version");
        let Err(err) = set_up(&fixture) else { panic!("refused") };
        assert_eq!(err, "it runs t3 0.1.12, this Mac 0.1.13");
        assert!(!fixture.root.join("host/wt").exists(), "nothing is set up there");
    }

    #[test]
    fn commits_that_cant_come_back_keep_the_hosts_worktree() {
        let fixture = fixture();
        let (mut hosted, _) = set_up(&fixture).expect("set up");
        commit_on_host(&fixture, "work.txt");
        let mac = fixture.root.join("mac/wt");
        std::fs::write(mac.join("mac.txt"), "here\n").expect("write");
        run_git(&mac, &["add", "mac.txt"]);
        run_git(&mac, &["commit", "-q", "-m", "on the Mac meanwhile"]);
        let err = hosted.bring_back("DEV-1", true).expect_err("can't fast-forward");
        assert!(err.contains("can't fast-forward"), "{err}");
        assert!(fixture.root.join("host/wt/work.txt").is_file(), "the host's worktree stays");
    }

    #[test]
    fn an_earlier_jobs_worktree_with_work_the_mac_lacks_keeps_the_job_here() {
        let fixture = fixture();
        let (hosted, _) = set_up(&fixture).expect("set up");
        drop(hosted);
        std::fs::write(fixture.root.join("host/wt/README"), "changed there\n").expect("write");
        let Err(err) = set_up(&fixture) else { panic!("refused") };
        assert!(err.contains("uncommitted work of an earlier job"), "{err}");
        run_git(&fixture.root.join("host/wt"), &["checkout", "--", "README"]);
        let (_, notes) = set_up(&fixture).expect("an earlier clean worktree goes");
        assert!(notes.iter().any(|note| note.starts_with("Removed the worktree of an earlier job")), "{notes:?}");
    }

    /// The fixture's host job with lotar's MCP connection: the forward's
    /// socket on this Mac and on the host.
    fn set_up_with_mcp(fixture: &Fixture, job_id: &str) -> (Hosted, PathBuf, PathBuf) {
        // The test runner's temp folder is too deep for a socket's path.
        // SAFETY: nextest runs each test in its own process.
        unsafe { std::env::set_var("TMPDIR", "/tmp") };
        let (repo, worktree) = (fixture.root.join("mac/repo"), fixture.root.join("mac/wt"));
        let settings = ThreadSettings { deny: Vec::new(), env: BTreeMap::new(), lotar_mcp: true };
        let checkout = Checkout { repo_root: &repo, worktree: &worktree, branch: "agent/DEV-1" };
        let server = McpServer { program: PathBuf::from("true"), env: BTreeMap::new(), on_lost: Box::new(|_| {}) };
        let (hosted, _) = Hosted::set_up(job_id, "dyson", &dyson(), &checkout, &settings, Some(server), Ok(Check::T3(fixture.local.clone()))).expect("set up");
        let bridge = hosted.bridge.as_ref().expect("an MCP connection");
        let (local, remote) = (bridge.local.clone(), PathBuf::from(&bridge.remote));
        assert!(local.exists() && remote.exists(), "the forward is open");
        (hosted, local, remote)
    }

    /// The tasks directory with the ticket DEV-1.
    fn tasks(fixture: &Fixture) -> PathBuf {
        let tasks_dir = fixture.root.join(".tasks");
        std::fs::create_dir_all(&tasks_dir).expect("tasks");
        std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: DEV\n").expect("config");
        let task = crate::api_types::TaskCreate { title: "Waits on dyson".to_string(), project: Some("DEV".to_string()), ..Default::default() };
        crate::services::task_service::TaskService::create(&mut crate::storage::manager::Storage::new(&tasks_dir), task).expect("ticket");
        tasks_dir
    }

    /// T3 Code's server on the host: it answers one command with success
    /// and hands it to the test.
    fn t3_server_on_host(fixture: &Fixture) -> std::thread::JoinHandle<serde_json::Value> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listen");
        let origin = format!("http://{}", listener.local_addr().expect("address"));
        std::fs::create_dir_all(fixture.root.join("host-t3/userdata")).expect("t3 home");
        std::fs::write(fixture.root.join("host-t3/userdata/server-runtime.json"), serde_json::json!({ "origin": origin }).to_string()).expect("runtime");
        std::thread::spawn(move || {
            // The forward's own check connects first and sends nothing.
            let mut socket = loop {
                let (stream, _) = listener.accept().expect("accept");
                if let Ok(socket) = tungstenite::accept(stream) {
                    break socket;
                }
            };
            let request: serde_json::Value = loop {
                if let tungstenite::Message::Text(text) = socket.read().expect("read")
                    && text.contains("Request")
                {
                    break serde_json::from_str(text.as_str()).expect("json");
                }
            };
            let answer = serde_json::json!({"_tag": "Exit", "requestId": request["id"], "exit": {"_tag": "Success", "value": {}}});
            socket.send(tungstenite::Message::text(answer.to_string())).expect("answer");
            let _ = socket.read();
            request
        })
    }

    fn comments(tasks_dir: &Path) -> Vec<String> {
        let storage = crate::storage::manager::Storage::new(tasks_dir);
        let task = crate::services::task_service::TaskService::get(&storage, "DEV-1", None).expect("ticket");
        task.comments.into_iter().map(|comment| comment.text).collect()
    }

    /// DEV-929: a new job of the ticket brings the waiting thread's commits
    /// back, closes its MCP connection, then stops the thread on the host.
    #[test]
    fn a_new_job_stops_the_thread_that_waits_on_the_host() {
        use crate::services::agent_job_service::{WAITING_ON_HOST, collect_waiting_on_host, wait_on_host};
        let fixture = fixture();
        let tasks_dir = tasks(&fixture);
        let (mut hosted, local, remote) = set_up_with_mcp(&fixture, "job-dev929-asked");
        commit_on_host(&fixture, "asked.txt");
        hosted.bring_back_so_far("DEV-1").expect("so far");
        agent_hosts::route_thread("thread-asked", &hosted.ssh);
        wait_on_host("DEV-1", "job-dev929-asked", Some("thread-asked".to_string()), hosted);
        commit_on_host(&fixture, "after.txt");
        let server = t3_server_on_host(&fixture);

        collect_waiting_on_host("job-dev929-new", "DEV-1", &tasks_dir);
        let request = server.join().expect("the stop reached T3 Code on the host");
        assert_eq!((request["payload"]["type"].as_str(), request["payload"]["threadId"].as_str()), (Some("thread.stop"), Some("thread-asked")));
        assert!(fixture.root.join("mac/wt/after.txt").is_file(), "its commits came back");
        assert!(!fixture.root.join("host/wt").exists(), "the host's worktree went");
        assert!(!local.exists() && !remote.exists(), "the MCP connection closed");
        assert!(WAITING_ON_HOST.lock().expect("lock").is_empty());
        let log = std::fs::read_to_string(fixture.root.join("ssh.log")).expect("ssh log");
        let (fetched, stopped) = (log.rfind("git-upload-pack").expect("fetched"), log.find("server-runtime.json").expect("stopped"));
        assert!(fetched < stopped, "the commits come back before the thread stops:\n{log}");
        assert!(comments(&tasks_dir).is_empty(), "{:?}", comments(&tasks_dir));
    }

    /// DEV-929: a stop that fails leaves the new job to run, says why on the
    /// ticket, and the MCP connection still closes.
    #[test]
    fn a_thread_that_cant_be_stopped_is_named_on_the_ticket() {
        use crate::services::agent_job_service::{WAITING_ON_HOST, collect_waiting_on_host, wait_on_host};
        let fixture = fixture();
        let tasks_dir = tasks(&fixture);
        let (hosted, local, remote) = set_up_with_mcp(&fixture, "job-dev929-unstoppable");
        agent_hosts::route_thread("thread-unstoppable", &hosted.ssh);
        wait_on_host("DEV-1", "job-dev929-unstoppable", Some("thread-unstoppable".to_string()), hosted);

        collect_waiting_on_host("job-dev929-next", "DEV-1", &tasks_dir);
        let comments = comments(&tasks_dir);
        assert!(comments.iter().any(|text| text.starts_with("Could not stop the thread thread-unstoppable") && text.contains("isn't running")), "{comments:?}");
        assert!(!local.exists() && !remote.exists(), "the MCP connection closed");
        assert!(WAITING_ON_HOST.lock().expect("lock").is_empty());
    }

    /// DEV-929: a question the ticket drops while its thread waits on the
    /// host ends the wait at the next look: the commits come back, the MCP
    /// connection closes and the thread stops. A job that comes later finds
    /// nothing left to stop.
    fn a_dropped_question_ends_the_wait(patch: crate::api_types::TaskUpdate, job_id: &str, thread: &str) {
        use crate::services::agent_job_service::{WAITING_ON_HOST, close_dropped_waits_on_host, collect_waiting_on_host, note_question_on_host, wait_on_host};
        let fixture = fixture();
        let tasks_dir = tasks(&fixture);
        let (hosted, local, remote) = set_up_with_mcp(&fixture, job_id);
        agent_hosts::route_thread(thread, &hosted.ssh);
        wait_on_host("DEV-1", job_id, Some(thread.to_string()), hosted);
        close_dropped_waits_on_host(&tasks_dir);
        assert!(WAITING_ON_HOST.lock().expect("lock").contains_key("DEV-1"), "the job's end hasn't left the question yet");
        note_question_on_host(job_id, "DEV-1", &tasks_dir);
        close_dropped_waits_on_host(&tasks_dir);
        assert!(local.exists() && remote.exists(), "the question still waits");

        commit_on_host(&fixture, "after.txt");
        let server = t3_server_on_host(&fixture);
        crate::services::task_service::TaskService::update(&mut crate::storage::manager::Storage::new(&tasks_dir), "DEV-1", patch).expect("drop");
        close_dropped_waits_on_host(&tasks_dir);
        let request = server.join().expect("the stop reached T3 Code on the host");
        assert_eq!((request["payload"]["type"].as_str(), request["payload"]["threadId"].as_str()), (Some("thread.stop"), Some(thread)));
        assert!(!local.exists() && !remote.exists(), "the MCP connection closed");
        assert!(fixture.root.join("mac/wt/after.txt").is_file(), "its commits came back");
        assert!(WAITING_ON_HOST.lock().expect("lock").is_empty());

        let before = std::fs::read_to_string(fixture.root.join("ssh.log")).expect("ssh log");
        collect_waiting_on_host("job-dev929-later", "DEV-1", &tasks_dir);
        assert_eq!(std::fs::read_to_string(fixture.root.join("ssh.log")).expect("ssh log"), before, "the later job has nothing to do");
        assert!(comments(&tasks_dir).is_empty(), "{:?}", comments(&tasks_dir));
    }

    /// DEV-929: a takeover moves the ticket to its agent before its job
    /// takes the entry; the look leaves that job its thread and connection.
    #[test]
    fn a_thread_taken_over_keeps_its_wait_while_its_job_runs() {
        use crate::services::agent_job_service::{JOB_REGISTRY, WAITING_ON_HOST, close_dropped_waits_on_host, note_question_on_host, wait_on_host};
        let fixture = fixture();
        let tasks_dir = tasks(&fixture);
        let (hosted, local, remote) = set_up_with_mcp(&fixture, "job-dev929-answered");
        agent_hosts::route_thread("thread-answered", &hosted.ssh);
        wait_on_host("DEV-1", "job-dev929-answered", Some("thread-answered".to_string()), hosted);
        note_question_on_host("job-dev929-answered", "DEV-1", &tasks_dir);
        let patch = crate::api_types::TaskUpdate { assignee: Some("@claude".to_string()), ..Default::default() };
        crate::services::task_service::TaskService::update(&mut crate::storage::manager::Storage::new(&tasks_dir), "DEV-1", patch).expect("take over");
        JOB_REGISTRY.lock().expect("registry").active_by_ticket.insert("DEV-1".to_string(), "job-dev929-takeover".to_string());

        close_dropped_waits_on_host(&tasks_dir);
        assert!(WAITING_ON_HOST.lock().expect("lock").contains_key("DEV-1"), "the takeover's job takes the entry");
        assert!(local.exists() && remote.exists(), "the MCP connection stays open");
        let log = std::fs::read_to_string(fixture.root.join("ssh.log")).expect("ssh log");
        assert!(!log.contains("server-runtime.json"), "the thread isn't stopped:\n{log}");
        JOB_REGISTRY.lock().expect("registry").active_by_ticket.remove("DEV-1");
        let mut entry = super::super::take_waiting_on_host("DEV-1").expect("entry");
        entry.hosted.close();
    }

    #[test]
    fn a_ticket_closed_while_its_thread_waits_ends_the_wait() {
        let patch = crate::api_types::TaskUpdate { status: Some("Done".to_string()), ..Default::default() };
        a_dropped_question_ends_the_wait(patch, "job-dev929-closed", "thread-closed");
    }

    #[test]
    fn a_ticket_reassigned_while_its_thread_waits_ends_the_wait_before_a_later_job() {
        let patch = crate::api_types::TaskUpdate { assignee: Some("sam".to_string()), ..Default::default() };
        a_dropped_question_ends_the_wait(patch, "job-dev929-reassigned", "thread-reassigned");
    }

    /// DEV-931: a headless agent's job taken over after lotar restarted goes
    /// on over ssh in the host's worktree, and its commits come back.
    #[test]
    fn a_headless_job_taken_over_after_a_restart_runs_on_the_host_and_brings_its_commits_back() {
        use crate::services::agent_job_service::{JOB_REGISTRY, RunEnd, finish_on_host, host_agent_table, resumed_on_host, tests::unit_job_state};
        let fixture = fixture();
        let (repo, worktree) = (fixture.root.join("mac/repo"), fixture.root.join("mac/wt"));
        let settings = ThreadSettings { deny: Vec::new(), env: BTreeMap::new(), lotar_mcp: false };
        let checkout = Checkout { repo_root: &repo, worktree: &worktree, branch: "agent/DEV-1" };
        let (hosted, _) =
            Hosted::set_up("job-dev931-old", "dyson", &dyson(), &checkout, &settings, None, Ok(Check::Programs(vec!["sh".to_string()]))).expect("set up");
        // lotar restarts: the agent goes on there.
        drop(hosted);
        commit_on_host(&fixture, "work.txt");
        std::fs::write(fixture.root.join("host/wt/left.txt"), "uncommitted\n").expect("leftover");

        let job_id = "job-dev931-takeover";
        let mut state = unit_job_state(job_id);
        state.record.worktree_path = Some(worktree.to_string_lossy().into_owned());
        state.record.worktree_branch = Some("agent/DEV-1".to_string());
        JOB_REGISTRY.lock().expect("registry").jobs.insert(job_id.to_string(), state);
        let mut config = crate::config::types::ResolvedConfig::from_global(crate::config::types::GlobalConfig::default());
        config.agent_hosts.insert("dyson".to_string(), dyson());
        let hosted = resumed_on_host(job_id, "DEV-1", Some("dyson"), &repo.join(".tasks"), &config, false).expect("on dyson");
        assert!(!hosted.thread);

        let profile = crate::config::types::AgentProfileDetail { runner: "claude".to_string(), ..Default::default() };
        let table = host_agent_table(&hosted, crate::services::agent_runner::agent_table().expect("table"), "claude", &profile).expect("table");
        let line = table.get("claude").expect("claude").command.clone();
        assert!(line.contains(&"dyson".to_string()) && line.last().is_some_and(|script| script.contains("exec env")), "{line:?}");

        let end = finish_on_host(job_id, "DEV-1", hosted, RunEnd::Exited(Some(0)), "session");
        assert!(matches!(end, RunEnd::Exited(Some(0))));
        assert!(worktree.join("work.txt").is_file() && worktree.join("left.txt").is_file(), "its commits and leftovers came back");
        assert!(!fixture.root.join("host/wt").exists(), "the host's worktree went");
        JOB_REGISTRY.lock().expect("registry").jobs.remove(job_id);
    }

    /// An ACP agent on stdio: it logs where it runs and each method it gets
    /// to `$FAKE_LOG`, refuses `session/resume` with `$FAKE_NO_RESUME`, and
    /// answers a prompt after a commit of `resumed.txt` when it runs in the
    /// host's worktree.
    const FAKE_ACP_AGENT: &str = r#"#!/bin/sh
printf 'runs in %s\n' "$PWD" >> "$FAKE_LOG"
respond() { printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$1" "$2"; }
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -nE 's/.*"id":("[^"]*"|[0-9]+).*/\1/p')
  method=$(printf '%s' "$line" | sed -nE 's/.*"method":"([^"]*)".*/\1/p')
  printf '%s\n' "$method" >> "$FAKE_LOG"
  case "$method" in
    initialize) respond "$id" '{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}},"agentInfo":{"name":"fake-acp","version":"1.0.0"},"authMethods":[]}' ;;
    session/new) respond "$id" '{"sessionId":"fake-1"}' ;;
    session/resume|session/load)
      if [ -n "$FAKE_NO_RESUME" ]; then
        printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32002,"message":"Resource not found: sessionId fake-1"}}\n' "$id"
      else
        respond "$id" '{}'
      fi ;;
    session/prompt)
      case "$PWD" in */host/wt) echo resumed > resumed.txt && git add resumed.txt && git -c commit.gpgsign=false commit -q -m 'DEV-1 resumed' >/dev/null 2>&1 ;; esac
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Done"}}}}\n'
      respond "$id" '{"stopReason":"end_turn"}' ;;
    *) if [ -n "$id" ]; then respond "$id" '{}'; fi ;;
  esac
done
"#;

    /// DEV-933: a headless job on dyson whose lotar exited mid-turn, as
    /// `finish_orphaned_job` hands it to `resume_session_to_end`: its
    /// session started there (by the fake agent, `env` its profile's), the
    /// host's worktree holds a commit and a leftover, and the job's record
    /// names the Mac worktree. Returns the tasks directory, the session's
    /// name and the job's profile; `ssh.log` starts empty.
    fn headless_job_to_take_over(fixture: &Fixture, job_id: &str, env: &[(&str, &str)]) -> (PathBuf, String, crate::config::types::AgentProfileDetail) {
        use crate::services::agent_job_service::{JOB_REGISTRY, tests::unit_job_state};
        use crate::services::agent_runner::{block_on, open_session, registry, session_profile};
        let (repo, worktree) = (fixture.root.join("mac/repo"), fixture.root.join("mac/wt"));
        let agent = fixture.root.join("bin/fake-acp");
        script(&agent, FAKE_ACP_AGENT);
        // SAFETY: nextest runs each test in its own process.
        unsafe { std::env::set_var("HARNESSMUX_HOME", fixture.root.join("harnessmux")) };
        let log = fixture.root.join("agent.log");
        let mut profile_env = vec![("FAKE_LOG".to_string(), log.to_string_lossy().into_owned())];
        profile_env.extend(env.iter().map(|(key, value)| ((*key).to_string(), (*value).to_string())));
        let profile = crate::config::types::AgentProfileDetail {
            runner: "claude".to_string(),
            command: Some(agent.to_string_lossy().into_owned()),
            env: profile_env.into_iter().collect(),
            ..Default::default()
        };

        // The job before the restart: its session starts on dyson.
        let settings = ThreadSettings { deny: Vec::new(), env: BTreeMap::new(), lotar_mcp: false };
        let checkout = Checkout { repo_root: &repo, worktree: &worktree, branch: "agent/DEV-1" };
        let (hosted, _) = Hosted::set_up(job_id, "dyson", &dyson(), &checkout, &settings, None, Ok(Check::Programs(vec!["sh".to_string()]))).expect("set up");
        let table = crate::services::agent_job_service::host_agent_table(
            &hosted,
            crate::services::agent_runner::job_agent_table("claude", &profile).expect("table"),
            "claude",
            &profile,
        )
        .expect("host table");
        let name = format!("lotar-dev933-{job_id}");
        let session = open_session(&name, "claude", &worktree, &table, session_profile(&profile, std::collections::HashMap::new()).expect("profile"), false)
            .expect("session");
        drop(session);
        // lotar exits: its session closes, the agent's work stays on dyson.
        block_on(harnessmux::client::request(&registry(), &name, &harnessmux::ipc::Request::Close)).expect("close");
        // Its record stays resumable, as a process that exits leaves it.
        let mut record = registry().load(&name).expect("record");
        record.state = harnessmux::SessionState::Idle;
        registry().save(&record).expect("save");
        drop(hosted);
        crate::services::agent_hosts::release(job_id);
        commit_on_host(fixture, "work.txt");
        std::fs::write(fixture.root.join("host/wt/left.txt"), "uncommitted\n").expect("leftover");
        std::fs::write(fixture.root.join("ssh.log"), "").expect("clear the ssh log");
        std::fs::write(&log, "").expect("clear the agent log");

        // In the Mac's repository, as the takeover finds the checkout from it.
        let tasks_dir = repo.join(".tasks");
        std::fs::create_dir_all(&tasks_dir).expect("tasks");
        std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: DEV\n").expect("config");
        let task = crate::api_types::TaskCreate { title: "Runs on dyson".to_string(), project: Some("DEV".to_string()), ..Default::default() };
        crate::services::task_service::TaskService::create(&mut crate::storage::manager::Storage::new(&tasks_dir), task).expect("ticket");
        let mut state = unit_job_state(job_id);
        state.record.ticket_id = "DEV-1".to_string();
        state.record.tasks_dir = tasks_dir.clone();
        state.record.runner = "claude".to_string();
        state.record.worktree_path = Some(worktree.to_string_lossy().into_owned());
        state.record.worktree_branch = Some("agent/DEV-1".to_string());
        state.request = Some(crate::api_types::AgentJobCreateRequest {
            ticket_id: "DEV-1".to_string(),
            prompt: "Fix it".to_string(),
            agent: Some("fix".to_string()),
            ..Default::default()
        });
        JOB_REGISTRY.lock().expect("registry").jobs.insert(job_id.to_string(), state);
        (tasks_dir, name, profile)
    }

    fn config_with_dyson() -> crate::config::types::ResolvedConfig {
        let mut config = crate::config::types::ResolvedConfig::from_global(crate::config::types::GlobalConfig::default());
        config.agent_hosts.insert("dyson".to_string(), dyson());
        config
    }

    fn progress(job_id: &str) -> Vec<String> {
        crate::services::agent_job_service::AgentJobService::events_for(job_id)
            .into_iter()
            .filter(|event| event.kind == "agent_job_progress")
            .filter_map(|event| event.message)
            .collect()
    }

    /// DEV-933: the takeover resumes a headless job's session over dyson's
    /// ssh, in the worktree there, and its commits come back to the Mac.
    #[test]
    fn a_taken_over_headless_job_resumes_its_session_on_the_host_and_brings_its_commits_back() {
        use crate::config::types::AgentRunnerKind;
        use crate::services::agent_job_service::{AgentJobService, resume_session_to_end};
        let fixture = fixture();
        let job_id = "job-dev933-resumed";
        let (tasks_dir, name, profile) = headless_job_to_take_over(&fixture, job_id, &[]);

        resume_session_to_end(
            job_id,
            AgentRunnerKind::Claude,
            "claude",
            &name,
            &profile,
            &tasks_dir,
            "DEV-1",
            &config_with_dyson(),
            "lotar exited",
            Some("dyson"),
        );

        let host_tree = fixture.root.join("host/wt");
        let agent_log = std::fs::read_to_string(fixture.root.join("agent.log")).expect("agent log");
        assert!(agent_log.contains(&format!("runs in {}", host_tree.display())), "the agent ran in dyson's worktree: {agent_log}");
        assert!(agent_log.contains("session/resume") && agent_log.contains("session/prompt"), "{agent_log}");
        let worktree = fixture.root.join("mac/wt");
        let ssh_log = std::fs::read_to_string(fixture.root.join("ssh.log")).expect("ssh log");
        let launch = format!("sh -lc 'cd '\\''{}'\\'' && exec env", worktree.display());
        assert!(ssh_log.contains(&launch) && ssh_log.contains("bin/fake-acp"), "it started over dyson's ssh: {ssh_log}");
        for file in ["work.txt", "left.txt", "resumed.txt"] {
            assert!(worktree.join(file).is_file(), "{file} came back: {:?}", progress(job_id));
        }
        assert!(run_git(&worktree, &["cat-file", "commit", "HEAD"]).contains("gpgsig"), "signed here");
        assert!(!host_tree.exists(), "the host's worktree went");
        assert_eq!(agent_hosts::job_host(job_id), None, "its slot on dyson is free");
        let job = AgentJobService::get_job(job_id).expect("job");
        assert_eq!(job.status, "completed", "{job:?}");
    }

    /// DEV-933: a headless job on dyson whose session can't be resumed
    /// brings its commits and leftovers back and is queued again.
    #[test]
    fn a_taken_over_headless_job_whose_session_cant_be_resumed_brings_its_work_back_and_runs_again() {
        use crate::config::types::AgentRunnerKind;
        use crate::services::agent_job_service::{AgentJobService, resume_session_to_end};
        let fixture = fixture();
        let queue = fixture.root.join("queue");
        // SAFETY: nextest runs each test in its own process.
        unsafe {
            std::env::set_var("LOTAR_AGENT_QUEUE_DIR", &queue);
            std::env::set_var("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        }
        let job_id = "job-dev933-not-resumed";
        let (tasks_dir, name, profile) = headless_job_to_take_over(&fixture, job_id, &[("FAKE_NO_RESUME", "1")]);

        resume_session_to_end(
            job_id,
            AgentRunnerKind::Claude,
            "claude",
            &name,
            &profile,
            &tasks_dir,
            "DEV-1",
            &config_with_dyson(),
            "lotar exited",
            Some("dyson"),
        );

        let job = AgentJobService::get_job(job_id).expect("job");
        assert_eq!(job.status, "paused", "{job:?}");
        let reason = job.summary.unwrap_or_default();
        assert!(reason.contains("could not be resumed") && reason.contains("runs again in a new session"), "{reason}");
        let worktree = fixture.root.join("mac/wt");
        assert!(worktree.join("work.txt").is_file() && worktree.join("left.txt").is_file(), "its commits and leftovers came back: {:?}", progress(job_id));
        let agent_log = std::fs::read_to_string(fixture.root.join("agent.log")).expect("agent log");
        assert!(agent_log.contains("session/resume") && !agent_log.contains("session/prompt"), "dyson's agent refused the resume: {agent_log}");
        assert!(!worktree.join("resumed.txt").exists(), "the agent got no turn");
        assert!(!fixture.root.join("host/wt").exists(), "the host's worktree went");
        assert_eq!(agent_hosts::job_host(job_id), None, "its slot on dyson is free");
        let pending = crate::services::agent_queue_service::AgentQueueService::list_pending(&tasks_dir).expect("queue");
        let retry = pending.iter().find(|entry| entry.ticket_id == "DEV-1").unwrap_or_else(|| panic!("the job is queued again: {pending:?}"));
        assert_eq!((retry.agent.as_str(), retry.prompt.as_str()), ("fix", "Fix it"), "{retry:?}");
        assert_eq!(retry.session, Some(crate::config::types::AgentSession::Fresh), "{retry:?}");
    }

    /// DEV-936: a headless job on dyson cancelled while the takeover resumes
    /// its session gets no turn; its commits come back, its slot frees, and
    /// what it left uncommitted stays in dyson's worktree, which is kept.
    #[test]
    fn a_headless_job_cancelled_during_its_takeover_brings_its_commits_back_and_frees_the_host() {
        use crate::config::types::AgentRunnerKind;
        use crate::services::agent_job_service::{AgentJobStatus, JOB_REGISTRY, resume_session_to_end};
        let fixture = fixture();
        let job_id = "job-dev936-cancelled";
        let (tasks_dir, name, profile) = headless_job_to_take_over(&fixture, job_id, &[]);
        JOB_REGISTRY.lock().expect("registry").jobs.get_mut(job_id).expect("job").record.status = AgentJobStatus::Cancelled;

        resume_session_to_end(
            job_id,
            AgentRunnerKind::Claude,
            "claude",
            &name,
            &profile,
            &tasks_dir,
            "DEV-1",
            &config_with_dyson(),
            "lotar exited",
            Some("dyson"),
        );

        let agent_log = std::fs::read_to_string(fixture.root.join("agent.log")).expect("agent log");
        assert!(agent_log.contains("session/resume") && !agent_log.contains("session/prompt"), "its session resumed, the agent got no turn: {agent_log}");
        let worktree = fixture.root.join("mac/wt");
        assert!(worktree.join("work.txt").is_file(), "its commits came back: {:?}", progress(job_id));
        assert!(run_git(&worktree, &["cat-file", "commit", "HEAD"]).contains("gpgsig"), "signed here");
        let host_tree = fixture.root.join("host/wt");
        assert!(!worktree.join("left.txt").exists() && host_tree.join("left.txt").is_file(), "the leftover stays on dyson");
        assert_eq!(run_git(&host_tree, &["status", "--porcelain"]), "?? left.txt", "uncommitted there");
        assert!(progress(job_id).iter().any(|note| note.starts_with("Kept the worktree")), "{:?}", progress(job_id));
        assert_eq!(agent_hosts::job_host(job_id), None, "its slot on dyson is free");
        assert_eq!(JOB_REGISTRY.lock().expect("registry").jobs.get(job_id).expect("job").record.status, AgentJobStatus::Cancelled);
    }

    /// DEV-931: a headless agent that asks a question has ended its turn:
    /// its leftovers are committed there, all comes back, nothing waits.
    #[test]
    fn a_headless_agent_that_asks_brings_everything_back() {
        use crate::services::agent_job_service::{RunEnd, WAITING_ON_HOST, finish_on_host};
        let fixture = fixture();
        let (repo, worktree) = (fixture.root.join("mac/repo"), fixture.root.join("mac/wt"));
        let settings = ThreadSettings { deny: Vec::new(), env: BTreeMap::new(), lotar_mcp: false };
        let checkout = Checkout { repo_root: &repo, worktree: &worktree, branch: "agent/DEV-1" };
        let (hosted, _) =
            Hosted::set_up("job-dev931-asks", "dyson", &dyson(), &checkout, &settings, None, Ok(Check::Programs(vec!["sh".to_string()]))).expect("set up");
        commit_on_host(&fixture, "asked.txt");
        std::fs::write(fixture.root.join("host/wt/draft.txt"), "uncommitted\n").expect("draft");

        let end = finish_on_host("job-dev931-asks", "DEV-1", hosted, RunEnd::WaitingForAnswer("Which one?".to_string()), "session");
        let RunEnd::WaitingForAnswer(message) = end else { panic!("it waits for the answer") };
        assert!(message.starts_with("Which one?") && message.contains("its commits, with what it left uncommitted there, are on this Mac"), "{message}");
        assert!(worktree.join("asked.txt").is_file() && worktree.join("draft.txt").is_file());
        assert!(run_git(&worktree, &["cat-file", "commit", "HEAD"]).contains("gpgsig"), "signed here");
        assert!(!fixture.root.join("host/wt").exists(), "the host's worktree went");
        assert!(!WAITING_ON_HOST.lock().expect("lock").contains_key("DEV-1"), "nothing waits on the host");

        let (_, notes) = Hosted::set_up("job-dev931-answered", "dyson", &dyson(), &checkout, &settings, None, Ok(Check::Programs(vec!["sh".to_string()])))
            .expect("the answered job may run there");
        assert!(!notes.iter().any(|note| note.starts_with("Removed")), "{notes:?}");
    }

    /// DEV-931: a profile the host can't take says what is missing.
    #[test]
    fn a_host_check_names_what_is_missing() {
        use crate::services::agent_job_service::host_check;
        let profile = |runner: &str| crate::config::types::AgentProfileDetail { runner: runner.to_string(), ..Default::default() };
        assert_eq!(host_check(&profile("command")).err().as_deref(), Some("a command job doesn't run on an agent host"));
        assert!(matches!(host_check(&profile("claude")), Ok(Check::Programs(_))));
        let refused = agent_hosts::cached_probe("dyson", &dyson(), Err("no codex agent command to run there".to_string()));
        assert_eq!(refused, Probe::Refused("no codex agent command to run there".to_string()));
    }
}
