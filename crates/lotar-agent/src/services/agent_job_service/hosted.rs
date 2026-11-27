//! A job on an agent host (DEV-901). Before its thread starts, the ticket's
//! branch is pushed over ssh to a clone at the main checkout's absolute path
//! on the host, and a worktree added there at the Mac worktree's path, with
//! the Claude settings and the lotar MCP server the thread reads. The MCP
//! server runs on this Mac: the host's `.mcp.json` starts a bridge to a
//! socket that an ssh connection lotar holds for the job forwards here. The
//! tickets stay on the Mac.
//!
//! When the thread ends, what it left uncommitted is committed there, the
//! branch fetched, fast-forwarded into the Mac worktree and its new commits
//! signed again here. Then the host's worktree goes. A thread that waits for
//! an answer brings its commits so far back the same way and keeps its
//! worktree and MCP connection there for the answer.

use crate::config::types::AgentHostConfig;
use crate::services::agent_hosts::{self, LocalT3, Probe, Ssh, quote, stop_child};
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

/// A job's run on a host, from its setup until its commits are back.
pub(super) struct Hosted {
    pub name: String,
    pub host: AgentHostConfig,
    pub ssh: Ssh,
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
    /// Ready the host for the job: it answers with this Mac's T3 Code, its
    /// clone has the branch, its worktree the settings. Any failure leaves
    /// the job to the Mac, with why.
    pub fn set_up(
        job_id: &str,
        name: &str,
        host: &AgentHostConfig,
        checkout: &Checkout<'_>,
        settings: &ThreadSettings,
        mcp: Option<McpServer>,
        local: Option<LocalT3>,
    ) -> Result<(Self, Vec<String>), String> {
        let Checkout { repo_root, worktree, branch } = *checkout;
        let local = local.ok_or("no t3 CLI on this Mac to compare versions with")?;
        match agent_hosts::probe(host, &local) {
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
        match hosted.ssh.run(&format!("t3 project add {repo}")) {
            Ok(_) => {}
            Err(err) if err.contains("already") || err.contains("exists") => {}
            Err(err) => return Err(format!("`t3 project add` failed there: {err}")),
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
        hosted.write_settings(settings, socket.as_deref())?;
        notes.push(format!("Runs on {name}: pushed {branch} to {} there, in the worktree {} at the same path.", repo_root.display(), worktree.display()));
        Ok((hosted, notes))
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
    -N) shift ;;
    *) break ;;
  esac
done
shift
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
        Hosted::set_up("job-1", "dyson", &dyson(), &checkout, &settings, None, Some(fixture.local.clone()))
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
