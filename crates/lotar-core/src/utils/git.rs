use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// Anchor Git to the command's working directory, not the invoking process's repo.
/// Keep SSH/askpass and author/committer identity variables; injected Git config
/// is removed because it can override core.worktree and repository discovery.
const REPOSITORY_ROUTING_KEYS: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_GRAFT_FILE",
    "GIT_SHALLOW_FILE",
    "GIT_REPLACE_REF_BASE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_PREFIX",
    "GIT_INTERNAL_SUPER_PREFIX",
    "GIT_SUPER_PREFIX",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
];

pub fn clear_repository_env(command: &mut Command) {
    let config_keys: Vec<_> = std::env::vars_os()
        .map(|(key, _)| key)
        .chain(command.get_envs().map(|(key, _)| key.to_os_string()))
        .filter(|key| key.to_str().is_some_and(|key| key.to_ascii_uppercase().starts_with("GIT_CONFIG")))
        .collect();
    for key in REPOSITORY_ROUTING_KEYS {
        command.env_remove(key);
    }
    for key in config_keys {
        command.env_remove(key);
    }
}

/// A git child in `root`. Its [`GitCommand::output`] and
/// [`GitCommand::status`] stop it after [`GIT_TIMEOUT`], or
/// [`GIT_LONG_TIMEOUT`] for the subcommands in [`LONG_SUBCOMMANDS`], unless
/// [`GitCommand::timeout`] says otherwise (DEV-812).
pub fn git_command(root: &Path) -> GitCommand {
    let mut command = Command::new("git");
    command.current_dir(root);
    clear_repository_env(&mut command);
    GitCommand { command, timeout: None }
}

/// How long a read-only git call may take (DEV-466). A git that blocks, for
/// example on a config file macOS holds behind a permission prompt, must not
/// hold up a request forever or pile up processes.
pub const GIT_TIMEOUT: Duration = Duration::from_secs(20);

/// How long a git call that can run long by design may take: a commit that
/// runs hooks, network transfers, history rewrites, `add` or `worktree add`
/// in a large checkout (DEV-812).
pub const GIT_LONG_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// The subcommands that get [`GIT_LONG_TIMEOUT`].
const LONG_SUBCOMMANDS: &[&str] = &["add", "am", "cherry-pick", "clone", "commit", "fetch", "gc", "merge", "pull", "push", "rebase", "worktree"];

/// A git command that never waits on its child without a timeout: there is
/// no untimed `output` or `status` to call (DEV-812).
pub struct GitCommand {
    command: Command,
    timeout: Option<Duration>,
}

impl GitCommand {
    pub fn arg(&mut self, arg: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        self.command.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.command.args(args);
        self
    }

    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.command.current_dir(dir);
        self
    }

    pub fn env(&mut self, key: impl AsRef<std::ffi::OsStr>, value: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        self.command.env(key, value);
        self
    }

    /// How long [`Self::output`] and [`Self::status`] wait before they stop
    /// the command.
    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = Some(timeout);
        self
    }

    /// [`output_within`] the command's timeout.
    pub fn output(&mut self) -> std::io::Result<Output> {
        let timeout = self.effective_timeout();
        output_within(&mut self.command, timeout)
    }

    fn effective_timeout(&self) -> Duration {
        self.timeout.unwrap_or_else(|| {
            let subcommand = self.command.get_args().next().and_then(|arg| arg.to_str());
            if subcommand.is_some_and(|subcommand| LONG_SUBCOMMANDS.contains(&subcommand)) { GIT_LONG_TIMEOUT } else { GIT_TIMEOUT }
        })
    }

    /// The exit status of [`Self::output`]; the output is discarded.
    pub fn status(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.output().map(|output| output.status)
    }

    /// The command itself, for a caller that runs it as a long-lived
    /// process and times each exchange (`git cat-file --batch`).
    pub fn command_mut(&mut self) -> &mut Command {
        &mut self.command
    }

    /// The command itself, for a runner that applies a time limit of its
    /// own (the merge job's `git push`).
    pub fn into_command(self) -> Command {
        self.command
    }
}

/// [`Command::output`] that stops the command and its children after
/// `timeout` and answers with a `TimedOut` error. Stdin is empty. The timeout
/// covers the output as well: a command that exits while a process it started
/// keeps stdout or stderr open has not finished.
pub fn output_within(command: &mut Command, timeout: Duration) -> std::io::Result<Output> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn_in_process_group(command)?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let result = collect_until(&mut child, &stdout, &stderr, deadline);
    if !matches!(result, Ok(Some(_))) {
        kill_process_group(&mut child);
    }
    result?.ok_or_else(|| {
        let program = command.get_program().to_string_lossy().into_owned();
        std::io::Error::new(std::io::ErrorKind::TimedOut, format!("{program} did not finish within {}s", timeout.as_secs()))
    })
}

/// Reads a pipe to its end on a thread of its own. The thread is never
/// joined: it ends when the last process holding the pipe open is gone.
fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> Receiver<Vec<u8>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        let _ = sender.send(bytes);
    });
    receiver
}

/// The child's exit status and complete output, or `None` when `deadline`
/// passes first.
fn collect_until(child: &mut Child, stdout: &Receiver<Vec<u8>>, stderr: &Receiver<Vec<u8>>, deadline: Instant) -> std::io::Result<Option<Output>> {
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let until_deadline = |pipe: &Receiver<Vec<u8>>| pipe.recv_timeout(deadline.saturating_duration_since(Instant::now())).ok();
    let (Some(stdout), Some(stderr)) = (until_deadline(stdout), until_deadline(stderr)) else {
        return Ok(None);
    };
    Ok(Some(Output { status, stdout, stderr }))
}

/// Starts `command` as the leader of its own process group, so that
/// [`kill_process_group`] also reaches the processes it starts.
pub fn spawn_in_process_group(command: &mut Command) -> std::io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn()
}

/// Kills a child started by [`spawn_in_process_group`] together with its
/// children, and reaps it. Children that started a process group of their
/// own, as a nested `lotar` call's run command does, are killed with their
/// group too (DEV-808).
pub fn kill_process_group(child: &mut Child) {
    #[cfg(unix)]
    kill_tree(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

/// How long a directory walk waits for git to name the global excludes file.
pub const EXCLUDES_TIMEOUT: Duration = Duration::from_secs(5);

/// Makes `builder` honour git's global excludes file without reading git's
/// config in this process (DEV-812): the `ignore` crate's own lookup opens
/// `~/.gitconfig`, and an open macOS holds behind a permission check blocks
/// the walk with no way to time it out. Git names the file instead, within
/// [`EXCLUDES_TIMEOUT`]; a `TimedOut` error means git did not answer and the
/// walk has no global excludes.
pub fn apply_global_excludes(builder: &mut ignore::WalkBuilder, root: &Path) -> std::io::Result<()> {
    builder.git_global(false);
    if let Some(path) = excludes_file(root)?.filter(|path| path.is_file()) {
        // A broken pattern drops that pattern only, as git does.
        let _ = builder.add_ignore(path);
    }
    Ok(())
}

/// Git's `core.excludesFile` as seen from `root`, else git's default
/// (`$XDG_CONFIG_HOME/git/ignore`, `~/.config/git/ignore`). Missing git or a
/// git error count as unset; only a git that does not answer is an error.
fn excludes_file(root: &Path) -> std::io::Result<Option<PathBuf>> {
    match git_command(root).args(["config", "--path", "--get", "core.excludesFile"]).timeout(EXCLUDES_TIMEOUT).output() {
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => Err(error),
        Ok(output) if output.status.success() => {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(if value.is_empty() { default_excludes_file() } else { Some(root.join(value)) })
        }
        _ => Ok(default_excludes_file()),
    }
}

fn default_excludes_file() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").filter(|dir| !dir.is_empty()).map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("git").join("ignore"))
}

/// Stops `root`'s process group and every process below `root` with its
/// group. The processes are stopped before they are listed and killed, so
/// none can start a new one in between or lose its parent link to `root`.
#[cfg(unix)]
fn kill_tree(root: u32) {
    let Ok(root) = i32::try_from(root) else { return };
    let signal = |pid: i32, signal: i32| {
        // SAFETY: plain kill(2) on a pid or, when negative, a process group.
        unsafe { libc::kill(pid, signal) };
    };
    signal(-root, libc::SIGSTOP);
    let mut tree = descendants(root);
    tree.iter().for_each(|&pid| signal(pid, libc::SIGSTOP));
    for pid in descendants(root) {
        if !tree.contains(&pid) {
            signal(pid, libc::SIGSTOP);
            tree.push(pid);
        }
    }
    // SAFETY: getpgrp(2) and getpgid(2) only read process groups.
    let own_group = unsafe { libc::getpgrp() };
    for &pid in &tree {
        let group = unsafe { libc::getpgid(pid) };
        if group > 1 && group != own_group && group != root {
            signal(-group, libc::SIGKILL);
        }
        signal(pid, libc::SIGKILL);
    }
    signal(-root, libc::SIGKILL);
}

/// Every process below `root`, from the process table `ps` lists.
#[cfg(unix)]
fn descendants(root: i32) -> Vec<i32> {
    let Ok(ps) = Command::new("ps").args(["-A", "-o", "pid=", "-o", "ppid="]).stdin(Stdio::null()).stderr(Stdio::null()).output() else {
        return Vec::new();
    };
    let pairs: Vec<(i32, i32)> = String::from_utf8_lossy(&ps.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace().map(str::parse::<i32>);
            Some((fields.next()?.ok()?, fields.next()?.ok()?))
        })
        .collect();
    let mut found = Vec::new();
    let mut parents = vec![root];
    while let Some(parent) = parents.pop() {
        let children = pairs.iter().filter(|&&(pid, ppid)| ppid == parent && pid != root).map(|&(pid, _)| pid);
        let before = found.len();
        found.extend(children);
        parents.extend_from_slice(&found[before..]);
    }
    found
}

/// Repository routing keys removed by [`clear_repository_env`]: presence in
/// an environment can direct Git discovery and writes outside the command's
/// working directory, and injected `GIT_CONFIG*` keys can override
/// `core.worktree`. Transport/auth/identity variables (SSH, askpass,
/// author/committer) are deliberately NOT routing keys. Matching is
/// case-insensitive so Windows env folds cannot slip a variant through.
pub fn is_repository_routing_key(key: &str) -> bool {
    REPOSITORY_ROUTING_KEYS.iter().any(|routing| routing.eq_ignore_ascii_case(key)) || key.to_ascii_uppercase().starts_with("GIT_CONFIG")
}

/// Verify discovery did not select a parent/foreign checkout and identify its repo.
pub fn verified_common_dir(root: &Path) -> std::io::Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let output = git_command(&root).args(["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-common-dir"]).output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("Cannot verify Git working directory"));
    }
    let text = String::from_utf8(output.stdout).map_err(std::io::Error::other)?;
    let mut lines = text.lines();
    let top = lines.next().ok_or_else(|| std::io::Error::other("Missing Git top-level"))?;
    let common = lines.next().ok_or_else(|| std::io::Error::other("Missing Git common directory"))?;
    if lines.next().is_some() || fs::canonicalize(top)? != root {
        return Err(std::io::Error::other("Git top-level differs from intended checkout"));
    }
    fs::canonicalize(common)
}

/// Find the git repo root by walking up from start until a .git directory or file is found.
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    // A relative start such as `.tasks` would otherwise walk up to the empty
    // path and return that as the root, which no command can run in.
    let absolute;
    let mut cur = if start.is_absolute() {
        start
    } else {
        absolute = std::env::current_dir().ok()?.join(start);
        absolute.as_path()
    };
    loop {
        let candidate = cur.join(".git");
        if candidate.is_dir() || candidate.is_file() {
            return Some(cur.to_path_buf());
        }
        cur = cur.parent()?;
    }
}

/// Express `path` relative to `repo_root`, the form Git pathspecs expect.
///
/// The direct prefix is tried first. When either side goes through a symlink
/// (for example macOS `/tmp` -> `/private/tmp`, or a symlinked workspace),
/// both sides are canonicalized and compared again. Returns `None` when the
/// path is not inside the repository.
pub fn repo_relative_path(repo_root: &Path, path: &Path) -> Option<PathBuf> {
    if let Ok(rel) = path.strip_prefix(repo_root) {
        return Some(rel.to_path_buf());
    }
    let root_real = fs::canonicalize(repo_root).ok()?;
    let path_real = fs::canonicalize(path).ok()?;
    path_real.strip_prefix(&root_real).ok().map(Path::to_path_buf)
}

/// Read the checkout's own HEAD, including gitdir-file linked worktrees.
pub fn read_current_branch(repo_root: &Path) -> Option<String> {
    read_branch_at(&repo_root.join(".git"))
}

fn read_branch_at(git_entry: &Path) -> Option<String> {
    let (git_dir, _) = metadata_dirs(git_entry)?;
    let head = git_dir.join("HEAD");
    let line = metadata_line(&head)?;
    let branch = line.strip_prefix("ref: refs/heads/")?;
    if branch.is_empty() || branch.chars().any(|c| c.is_ascii_whitespace() || c.is_ascii_control()) {
        return None;
    }
    Some(branch.to_string())
}

/// Parse local remotes from the shared Git config, not the worktree admin directory.
pub fn read_remotes(repo_root: &Path) -> Vec<String> {
    read_remotes_at(&repo_root.join(".git"))
}

fn read_remotes_at(git_entry: &Path) -> Vec<String> {
    let Some((_, common_dir)) = metadata_dirs(git_entry) else {
        return vec![];
    };
    let config_path = common_dir.join("config");
    let contents = match fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut remotes = Vec::new();
    let mut in_remote = false;
    for line in contents.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_remote = t.starts_with("[remote ");
            continue;
        }
        if in_remote && t.starts_with("url = ") {
            remotes.push(t.trim_start_matches("url = ").trim().to_string());
        }
    }
    remotes
}

/// The web page of a commit for a remote on a known host, without the hash
/// (e.g. `https://github.com/o/r/commit/`); `None` for other hosts (DEV-490).
pub fn commit_url_prefix(remote: &str) -> Option<String> {
    let remote = remote.trim();
    // https://host/o/r(.git), ssh://git@host/o/r.git or git@host:o/r.git
    let rest = match remote.split_once("://") {
        Some((_, rest)) => rest,
        None => remote,
    };
    let rest = rest.rsplit_once('@').map_or(rest, |(_, rest)| rest);
    let (host, path) = match rest.split_once('/') {
        Some((host, path)) if !host.contains(':') || remote.contains("://") => (host.split(':').next()?, path),
        _ => rest.split_once(':')?,
    };
    let path = path.trim_matches('/').trim_end_matches(".git");
    if path.is_empty() || path.split('/').count() < 2 {
        return None;
    }
    let commit = match host.to_ascii_lowercase().as_str() {
        "github.com" | "codeberg.org" => "commit",
        "gitlab.com" => "-/commit",
        "bitbucket.org" => "commits",
        _ => return None,
    };
    Some(format!("https://{host}/{path}/{commit}/"))
}

/// The top folder of every checkout of the repository `repo_root` belongs
/// to: the main checkout and each linked worktree, `repo_root` included.
/// Read from Git's metadata files, so no `git` process runs; missing or
/// pruned worktrees are left out. A relative `gitdir` line resolves against
/// its admin folder, not the process's working directory.
pub fn checkout_tops(repo_root: &Path) -> Vec<PathBuf> {
    let Some((_, common_dir)) = metadata_dirs(&repo_root.join(".git")) else {
        return vec![];
    };
    let mut tops = Vec::new();
    if common_dir.file_name().is_some_and(|name| name == ".git")
        && let Some(main) = common_dir.parent()
    {
        tops.push(main.to_path_buf());
    }
    let admins = fs::read_dir(common_dir.join("worktrees")).into_iter().flatten().flatten();
    for admin in admins {
        // Git 2.48+ writes the line relative to the admin folder for
        // worktrees added with --relative-paths (DEV-825).
        let admin = admin.path();
        let top = metadata_line(&admin.join("gitdir")).filter(|line| !line.is_empty()).and_then(|line| fs::canonicalize(admin.join(line).parent()?).ok());
        if let Some(top) = top.filter(|top| top.is_dir() && !tops.contains(top)) {
            tops.push(top);
        }
    }
    tops
}

/// Read-only metadata location, not the ownership verification used for Git writes.
pub fn metadata_dirs(git_entry: &Path) -> Option<(PathBuf, PathBuf)> {
    let git_dir = if git_entry.is_dir() {
        fs::canonicalize(git_entry).ok()?
    } else {
        let marker = metadata_line(git_entry)?;
        metadata_directory(marker.strip_prefix("gitdir: ")?, git_entry.parent()?)?
    };
    let common_file = git_dir.join("commondir");
    let common_dir = match fs::symlink_metadata(&common_file) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => git_dir.clone(),
        Ok(_) => metadata_directory(&metadata_line(&common_file)?, &git_dir)?,
        Err(_) => return None,
    };
    Some((git_dir, common_dir))
}

fn metadata_line(path: &Path) -> Option<String> {
    if !fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let contents = fs::read_to_string(path).ok()?;
    let mut lines = contents.lines();
    let line = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    Some(line.to_string())
}

fn metadata_directory(value: &str, base: &Path) -> Option<PathBuf> {
    if value.is_empty() || value.contains('\0') {
        return None;
    }
    let path = Path::new(value);
    let directory = fs::canonicalize(if path.is_absolute() { path.to_path_buf() } else { base.join(path) }).ok()?;
    directory.is_dir().then_some(directory)
}

#[cfg(test)]
#[path = "../../tests/common/dev73_git_metadata_cases.rs"]
mod metadata_tests;

#[cfg(test)]
mod commit_url_tests {
    use super::commit_url_prefix;

    #[test]
    fn known_hosts_give_a_commit_page() {
        let github = Some("https://github.com/localtaskrepo/lotar/commit/".to_string());
        assert_eq!(commit_url_prefix("git@github.com:localtaskrepo/lotar.git"), github);
        assert_eq!(commit_url_prefix("https://github.com/localtaskrepo/lotar.git"), github);
        assert_eq!(commit_url_prefix("https://token@github.com/localtaskrepo/lotar"), github);
        assert_eq!(commit_url_prefix("ssh://git@github.com:22/localtaskrepo/lotar.git"), github);
        assert_eq!(commit_url_prefix("git@gitlab.com:group/sub/app.git"), Some("https://gitlab.com/group/sub/app/-/commit/".to_string()));
        assert_eq!(commit_url_prefix("https://bitbucket.org/team/app"), Some("https://bitbucket.org/team/app/commits/".to_string()));
    }

    #[test]
    fn other_remotes_give_none() {
        assert_eq!(commit_url_prefix("nyx:repos/app.git"), None);
        assert_eq!(commit_url_prefix("/srv/git/app.git"), None);
        assert_eq!(commit_url_prefix("https://git.example.com/o/r.git"), None);
        assert_eq!(commit_url_prefix("git@github.com:lotar"), None);
    }
}

#[cfg(test)]
mod routing_tests {
    use super::is_repository_routing_key;

    /// The lib-test ctor scrubs routing variables once per process; nothing
    /// that matches the shared inventory may be present afterwards.
    #[test]
    fn lib_test_baseline_has_no_repository_routing_env() {
        for (key, _) in std::env::vars() {
            assert!(!is_repository_routing_key(&key), "routing variable {key} survived the lib-test baseline");
        }
    }
}

#[cfg(all(test, unix))]
mod timeout_tests {
    use super::output_within;
    use std::process::Command;
    use std::time::{Duration, Instant};

    /// A command that hangs is stopped with its children and answers with an
    /// error instead of blocking the caller (DEV-466).
    #[test]
    fn hanging_command_is_killed_at_the_timeout() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pid_file = dir.path().join("pid");
        let mut command = Command::new("sh");
        command.arg("-c").arg(format!("sleep 600 & echo $! > {}; wait", pid_file.display()));
        let started = Instant::now();
        let error = output_within(&mut command, Duration::from_millis(300)).expect_err("hanging command must time out");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
        let pid = std::fs::read_to_string(&pid_file).expect("pid file");
        assert!(stops_running(pid.trim()), "the hanging child {pid} is still running");
    }

    /// A command that exits while a process it started still holds its output
    /// open: collecting the output must stop at the timeout too (DEV-466).
    #[test]
    fn child_holding_the_output_open_is_killed_at_the_timeout() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pid_file = dir.path().join("pid");
        let mut command = Command::new("sh");
        command.arg("-c").arg(format!("sleep 30 & echo $! > {}; exit 0", pid_file.display()));
        let started = Instant::now();
        let error = output_within(&mut command, Duration::from_millis(300)).expect_err("open output must time out");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2), "took {:?}", started.elapsed());
        let pid = std::fs::read_to_string(&pid_file).expect("pid file");
        assert!(stops_running(pid.trim()), "the child {pid} holding the output is still running");
    }

    /// Whether `pid` is gone or a zombie within a few seconds. A killed child
    /// stays in the process table until its new parent reaps it (DEV-654).
    fn stops_running(pid: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let ps = Command::new("ps").args(["-o", "stat=", "-p", pid]).output().expect("ps");
            if !ps.status.success() || String::from_utf8_lossy(&ps.stdout).trim_start().starts_with('Z') {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A git child blocked opening its global config (macOS holding
    /// `~/.gitconfig` behind a permission check, here a FIFO nobody writes)
    /// is stopped at its timeout (DEV-812).
    #[test]
    fn git_blocked_on_its_config_times_out() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(Command::new("mkfifo").arg(dir.path().join(".gitconfig")).status().expect("mkfifo").success());
        let started = Instant::now();
        let error = super::git_command(dir.path())
            .env("HOME", dir.path())
            .env("XDG_CONFIG_HOME", dir.path().join("xdg"))
            .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .timeout(Duration::from_millis(500))
            .output()
            .expect_err("git must time out");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
    }

    /// Subcommands that run hooks or transfers get the long limit; the rest
    /// the read-only one (DEV-812).
    #[test]
    fn long_subcommands_get_the_long_timeout() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(super::git_command(dir.path()).args(["commit", "-m", "x"]).effective_timeout(), super::GIT_LONG_TIMEOUT);
        assert_eq!(super::git_command(dir.path()).args(["worktree", "add", "x"]).effective_timeout(), super::GIT_LONG_TIMEOUT);
        assert_eq!(super::git_command(dir.path()).args(["rev-parse", "HEAD"]).effective_timeout(), super::GIT_TIMEOUT);
        assert_eq!(super::git_command(dir.path()).arg("push").timeout(Duration::from_secs(1)).effective_timeout(), Duration::from_secs(1));
    }

    #[test]
    fn finished_command_returns_its_output() {
        let output = output_within(Command::new("sh").arg("-c").arg("echo out; echo err >&2; exit 3"), Duration::from_secs(10)).expect("output");
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(output.stdout, b"out\n");
        assert_eq!(output.stderr, b"err\n");
    }
}
