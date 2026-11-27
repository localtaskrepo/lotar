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
//! worktree and MCP connection there for the answer.

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
        check: Option<Check>,
    ) -> Result<(Self, Vec<String>), String> {
        let Checkout { repo_root, worktree, branch } = *checkout;
        let check = check.ok_or("no t3 CLI on this Mac to compare versions with")?;
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

    /// A thread that already runs on the host in a worktree an earlier job
    /// set up, taken over after lotar restarted: no MCP connection, and its
    /// commits come back when it ends.
    pub fn attach(name: &str, host: &AgentHostConfig, checkout: &Checkout<'_>) -> Result<Self, String> {
        let Checkout { repo_root, worktree, branch } = *checkout;
        Ok(Self {
            name: name.to_string(),
            host: host.clone(),
            ssh: Ssh::new(&host.ssh),
            thread: true,
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
    /// command here, with `<root>/mac` paths rewritten to `<root>/host`, and
    /// a `t3` whose version is in `<root>/host-version`.
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
while [ $# -gt 0 ]; do
  case "$1" in
    -o|-R|-L) shift 2 ;;
    -N|-T) shift ;;
    *) break ;;
  esac
done
shift
[ "$1" = "--" ] && shift
cmd="$*"
case "$cmd" in "sh -lc "*) cmd="sh -c ${{cmd#sh -lc }}" ;; esac
cmd=$(printf '%s' "$cmd" | sed "s#{root}/mac#{root}/host#g")
PATH="{root}/host-bin:$PATH" exec sh -c "$cmd"
"#,
                root = root.display()
            ),
        );
        std::fs::write(root.join("host-version"), "0.1.13\n").expect("version");
        script(
            &root.join("host-bin/t3"),
            &format!("#!/bin/sh\ncase \"$1\" in --version) cat {}/host-version ;; project) echo added ;; esac\n", root.display()),
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
        Hosted::set_up("job-1", "dyson", &dyson(), &checkout, &settings, None, Some(Check::T3(fixture.local.clone())))
    }

    #[test]
    fn a_headless_agents_host_job_gets_the_worktree_without_thread_settings() {
        let fixture = fixture();
        let (repo, worktree) = (fixture.root.join("mac/repo"), fixture.root.join("mac/wt"));
        let settings = ThreadSettings { deny: vec!["mcp__claude_ai_Gmail".to_string()], env: BTreeMap::new(), lotar_mcp: false };
        let checkout = Checkout { repo_root: &repo, worktree: &worktree, branch: "agent/DEV-1" };
        script(&fixture.root.join("host-bin/t3"), &format!("#!/bin/sh\ntouch {}/t3-called\n", fixture.root.display()));
        let (hosted, _) =
            Hosted::set_up("job-1", "dyson", &dyson(), &checkout, &settings, None, Some(Check::Programs(vec!["sh".to_string()]))).expect("set up");
        assert!(!hosted.thread);
        let host_tree = fixture.root.join("host/wt");
        assert_eq!(run_git(&host_tree, &["rev-parse", "HEAD"]), run_git(&worktree, &["rev-parse", "HEAD"]));
        assert!(!host_tree.join(SETTINGS_FILE).exists() && !host_tree.join(".mcp.json").exists());
        assert!(!fixture.root.join("t3-called").exists(), "no `t3 project add` for a headless agent");

        let refused = Hosted::set_up("job-2", "dyson", &dyson(), &checkout, &settings, None, Some(Check::Programs(vec!["no-such-agent".to_string()])));
        assert_eq!(refused.err().as_deref(), Some("no-such-agent is not on its PATH"));
    }

    fn headless(fixture: &Fixture, command: &[&str], env: &[(&str, &str)]) -> Vec<String> {
        let (repo, worktree) = (fixture.root.join("mac/repo"), fixture.root.join("mac/wt"));
        let settings = ThreadSettings { deny: Vec::new(), env: BTreeMap::new(), lotar_mcp: false };
        let checkout = Checkout { repo_root: &repo, worktree: &worktree, branch: "agent/DEV-1" };
        let (hosted, _) =
            Hosted::set_up("job-1", "dyson", &dyson(), &checkout, &settings, None, Some(Check::Programs(vec!["sh".to_string()]))).expect("set up");
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
        let mut attached = Hosted::attach("dyson", &dyson(), &checkout).expect("attach");
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
}
