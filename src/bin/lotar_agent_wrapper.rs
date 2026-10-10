use std::env;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Default)]
struct WrapperConfig {
    job_id: Option<String>,
    ticket_id: Option<String>,
    runner: Option<String>,
    child_program: String,
    child_args: Vec<String>,
}

fn parse_args(args: &[String]) -> Result<WrapperConfig, String> {
    let mut cfg = WrapperConfig::default();
    let mut idx = 0;
    let mut seen_separator = false;
    while idx < args.len() {
        let arg = &args[idx];
        if arg == "--" {
            seen_separator = true;
            idx += 1;
            break;
        }
        match arg.as_str() {
            "--job-id" => {
                let value = args.get(idx + 1).ok_or("Missing value for --job-id")?;
                cfg.job_id = Some(value.to_string());
                idx += 2;
            }
            "--ticket-id" => {
                let value = args.get(idx + 1).ok_or("Missing value for --ticket-id")?;
                cfg.ticket_id = Some(value.to_string());
                idx += 2;
            }
            "--runner" => {
                let value = args.get(idx + 1).ok_or("Missing value for --runner")?;
                cfg.runner = Some(value.to_string());
                idx += 2;
            }
            _ => {
                return Err(format!("Unexpected argument '{arg}'"));
            }
        }
    }

    if !seen_separator {
        return Err("Missing '--' separator before child command".to_string());
    }

    if idx >= args.len() {
        return Err("Missing child command after '--'".to_string());
    }

    cfg.child_program = args[idx].clone();
    cfg.child_args = args[idx + 1..].to_vec();
    Ok(cfg)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShimDispatch {
    program: String,
    leading_args: Vec<String>,
    trailing_args: Vec<String>,
}

const SHIM_MAX_BYTES: u64 = 64 * 1024;

const NPM_CMD_SHIM_HEAD: [&str; 8] = [
    "@ECHO off",
    "GOTO start",
    ":find_dp0",
    "SET dp0=%~dp0",
    "EXIT /b",
    ":start",
    "SETLOCAL",
    "CALL :find_dp0",
];

const NPM_CMD_SHIM_PROG_BLOCK: [&str; 6] = [
    "IF EXIST \"%dp0%\\node.exe\" (",
    "  SET \"_prog=%dp0%\\node.exe\"",
    ") ELSE (",
    "  SET \"_prog=node\"",
    "  SET PATHEXT=%PATHEXT:;.JS;=;%",
    ")",
];

fn build_child_command(cfg: &WrapperConfig) -> Command {
    match resolve_batch_shim(&cfg.child_program) {
        Some(dispatch) => {
            let mut cmd = Command::new(&dispatch.program);
            cmd.args(&dispatch.leading_args)
                .args(&cfg.child_args)
                .args(&dispatch.trailing_args);
            cmd
        }
        None => {
            let mut cmd = Command::new(&cfg.child_program);
            cmd.args(&cfg.child_args);
            cmd
        }
    }
}

fn resolve_batch_shim(program: &str) -> Option<ShimDispatch> {
    use std::io::Read;

    let path = Path::new(program);
    let extension = path.extension().and_then(|ext| ext.to_str())?;
    if !extension.eq_ignore_ascii_case("cmd") && !extension.eq_ignore_ascii_case("bat") {
        return None;
    }
    let mut reader = std::fs::File::open(path).ok()?.take(SHIM_MAX_BYTES + 1);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > SHIM_MAX_BYTES {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    resolve_lotar_forwarder(&lines).or_else(|| resolve_npm_cmd_shim(path, &lines))
}

fn resolve_lotar_forwarder(lines: &[&str]) -> Option<ShimDispatch> {
    let mut content_lines = lines.iter().copied().filter(|line| !line.trim().is_empty());
    let line = content_lines.next()?;
    if content_lines.next().is_some() {
        return None;
    }
    let rest = line.strip_prefix('@')?;
    let (program, rest) = take_quoted(rest)?;
    let rest = rest.strip_prefix(' ')?;
    let (script, rest) = take_quoted(rest)?;
    let mut rest = rest.strip_prefix(" %*")?;
    let mut trailing_args = Vec::new();
    while !rest.is_empty() {
        rest = rest.strip_prefix(' ')?;
        let (arg, after) = take_quoted(rest)?;
        trailing_args.push(arg.to_string());
        rest = after;
    }
    if [program, script]
        .into_iter()
        .chain(trailing_args.iter().map(String::as_str))
        .any(contains_cmd_expansion)
    {
        return None;
    }
    Some(ShimDispatch {
        program: program.to_string(),
        leading_args: vec![script.to_string()],
        trailing_args,
    })
}

fn contains_cmd_expansion(value: &str) -> bool {
    value.contains(['%', '^', '!'])
}

fn resolve_npm_cmd_shim(shim_path: &Path, lines: &[&str]) -> Option<ShimDispatch> {
    let mut idx = 0;
    for expected in NPM_CMD_SHIM_HEAD {
        if lines.get(idx) != Some(&expected) {
            return None;
        }
        idx += 1;
    }
    idx = skip_blank_lines(lines, idx);
    for expected in NPM_CMD_SHIM_PROG_BLOCK {
        if lines.get(idx) != Some(&expected) {
            return None;
        }
        idx += 1;
    }
    idx = skip_blank_lines(lines, idx);
    let invocation = lines.get(idx)?;
    let target = parse_npm_invocation(invocation)?;
    idx += 1;
    if lines.get(idx) == Some(&"ENDLOCAL") {
        idx += 1;
    }
    if lines[idx..].iter().any(|line| !line.trim().is_empty()) {
        return None;
    }

    let dir = shim_path.parent()?;
    let script = dir.join(target.replace('\\', "/"));
    let node_exe = dir.join("node.exe");
    let program = if node_exe.is_file() {
        node_exe.to_string_lossy().to_string()
    } else {
        "node".to_string()
    };
    Some(ShimDispatch {
        program,
        leading_args: vec![script.to_string_lossy().to_string()],
        trailing_args: Vec::new(),
    })
}

fn parse_npm_invocation(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("endLocal & goto #_undefined_# 2>NUL || ")
        .unwrap_or(line);
    let rest = rest.strip_prefix("title %COMSPEC% & ").unwrap_or(rest);
    let rest = rest.strip_prefix("\"%_prog%\"  ")?;
    let (target, rest) = take_quoted(rest)?;
    let rest = rest.strip_prefix(" %*")?;
    if !rest.is_empty() {
        return None;
    }
    let relative = target.strip_prefix("%dp0%\\")?;
    if relative.is_empty()
        || relative.contains('%')
        || relative
            .split('\\')
            .any(|component| component.is_empty() || component == "..")
    {
        return None;
    }
    Some(relative)
}

fn take_quoted(input: &str) -> Option<(&str, &str)> {
    let rest = input.strip_prefix('"')?;
    let end = rest.find('"')?;
    let (value, after) = rest.split_at(end);
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some((value, after.strip_prefix('"')?))
}

fn skip_blank_lines(lines: &[&str], mut idx: usize) -> usize {
    while lines.get(idx).is_some_and(|line| line.trim().is_empty()) {
        idx += 1;
    }
    idx
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let cfg = match parse_args(&args) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("lotar-agent-wrapper: {err}");
            std::process::exit(2);
        }
    };

    let mut cmd = build_child_command(&cfg);
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(job_id) = cfg.job_id.as_ref() {
        cmd.env("LOTAR_JOB_ID", job_id);
    }
    if let Some(ticket_id) = cfg.ticket_id.as_ref() {
        cmd.env("LOTAR_TICKET_ID", ticket_id);
    }
    if let Some(runner) = cfg.runner.as_ref() {
        cmd.env("LOTAR_RUNNER", runner);
    }
    cmd.env("LOTAR_WRAPPER", "1");

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            eprintln!("lotar-agent-wrapper: failed to spawn child: {err}");
            std::process::exit(1);
        }
    };

    let status = match child.wait() {
        Ok(status) => status,
        Err(err) => {
            eprintln!("lotar-agent-wrapper: failed to wait for child: {err}");
            std::process::exit(1);
        }
    };

    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crlf(lines: &[&str]) -> String {
        lines.iter().map(|line| format!("{line}\r\n")).collect()
    }

    fn crlf_owned(lines: &[String]) -> String {
        lines.iter().map(|line| format!("{line}\r\n")).collect()
    }

    fn npm_shim(invocation: &str, extra_tail: &[&str]) -> String {
        let mut lines = NPM_CMD_SHIM_HEAD.to_vec();
        lines.push("");
        lines.extend(NPM_CMD_SHIM_PROG_BLOCK);
        lines.push("");
        lines.push(invocation);
        lines.extend(extra_tail);
        crlf(&lines)
    }

    fn modern_invocation(target: &str) -> String {
        format!(
            "endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"{target}\" %*"
        )
    }

    fn write_shim(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn parse_invocation(text: &str) -> Option<&str> {
        parse_npm_invocation(
            text.lines()
                .find(|line| line.contains("%_prog%"))
                .unwrap_or(""),
        )
    }

    #[test]
    fn parse_wrapper_args() {
        let args = vec![
            "--job-id".to_string(),
            "job-1".to_string(),
            "--ticket-id".to_string(),
            "TEST-1".to_string(),
            "--runner".to_string(),
            "codex".to_string(),
            "--".to_string(),
            "codex".to_string(),
            "exec".to_string(),
            "--json".to_string(),
        ];
        let cfg = parse_args(&args).expect("parse args");
        assert_eq!(cfg.job_id.as_deref(), Some("job-1"));
        assert_eq!(cfg.ticket_id.as_deref(), Some("TEST-1"));
        assert_eq!(cfg.runner.as_deref(), Some("codex"));
        assert_eq!(cfg.child_program, "codex");
        assert_eq!(cfg.child_args, vec!["exec", "--json"]);
    }

    #[test]
    fn resolves_modern_npm_cmd_shim_with_spaced_dir() {
        let dir = tempfile::tempdir().unwrap();
        let target = "%dp0%\\node_modules\\@anthropic-ai\\claude-code\\cli.js";
        let shim = write_shim(
            dir.path(),
            "claude.cmd",
            &npm_shim(&modern_invocation(target), &[]),
        );
        let dispatch = resolve_batch_shim(&shim.to_string_lossy()).expect("resolves");
        assert_eq!(dispatch.program, "node");
        assert_eq!(
            dispatch.leading_args,
            vec![
                dir.path()
                    .join("node_modules/@anthropic-ai/claude-code/cli.js")
                    .to_string_lossy()
                    .to_string()
            ]
        );
        assert_eq!(dispatch.trailing_args, Vec::<String>::new());
    }

    #[test]
    fn resolves_legacy_npm_cmd_shim_with_endlocal_tail() {
        let dir = tempfile::tempdir().unwrap();
        let target = "%dp0%\\node_modules\\.bin\\_codex.cjs";
        let invocation = format!("\"%_prog%\"  \"{target}\" %*");
        let shim = write_shim(
            dir.path(),
            "codex.cmd",
            &npm_shim(&invocation, &["ENDLOCAL"]),
        );
        let dispatch = resolve_batch_shim(&shim.to_string_lossy()).expect("resolves");
        assert_eq!(dispatch.program, "node");
        assert_eq!(
            dispatch.leading_args,
            vec![
                dir.path()
                    .join("node_modules/.bin/_codex.cjs")
                    .to_string_lossy()
                    .to_string()
            ]
        );
        assert_eq!(dispatch.trailing_args, Vec::<String>::new());
    }

    #[test]
    fn resolves_npm_shim_to_bundled_node_exe_when_present() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("node.exe"), b"").unwrap();
        let shim = write_shim(
            dir.path(),
            "gemini.cmd",
            &npm_shim(&modern_invocation("%dp0%\\node_modules\\x\\cli.js"), &[]),
        );
        let dispatch = resolve_batch_shim(&shim.to_string_lossy()).expect("resolves");
        assert_eq!(
            dispatch.program,
            dir.path().join("node.exe").to_string_lossy().to_string()
        );
    }

    #[test]
    fn resolves_lotar_fixture_forwarder_with_baked_args() {
        let dir = tempfile::tempdir().unwrap();
        let content = "@\"C:\\Program Files\\nodejs\\node.exe\" \"C:\\tools dir\\agent.mjs\" %* \"--mode\" \"copilot\" \"--gate-dir\" \"C:\\temp dir\\gate\"\r\n";
        let shim = write_shim(dir.path(), "agent-launcher-1.cmd", content);
        let dispatch = resolve_batch_shim(&shim.to_string_lossy()).expect("resolves");
        assert_eq!(dispatch.program, "C:\\Program Files\\nodejs\\node.exe");
        assert_eq!(
            dispatch.leading_args,
            vec!["C:\\tools dir\\agent.mjs".to_string()]
        );
        assert_eq!(
            dispatch.trailing_args,
            vec![
                "--mode".to_string(),
                "copilot".to_string(),
                "--gate-dir".to_string(),
                "C:\\temp dir\\gate".to_string(),
            ]
        );
    }

    #[test]
    fn rejects_forwarder_values_with_cmd_expansion_syntax() {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in [
            (
                "percent-var.cmd",
                "@\"node\" \"a.mjs\" %* \"--gate-dir\" \"C:\\%TEMP%\\gate\"\r\n",
            ),
            ("caret.cmd", "@\"node\" \"a^b.mjs\" %*\r\n"),
            ("delayed.cmd", "@\"node\" \"a.mjs\" %* \"label!\"\r\n"),
        ] {
            let shim = write_shim(dir.path(), name, content);
            assert_eq!(
                resolve_batch_shim(&shim.to_string_lossy()),
                None,
                "{name} must stay unresolved"
            );
        }
    }

    #[test]
    fn rejects_unrecognized_or_tampered_batch_files() {
        let dir = tempfile::tempdir().unwrap();
        let pnpm_style = "@IF EXIST \"%~dp0\\node.exe\" (\r\n  \"%~dp0\\node.exe\"  \"%~dp0%\\node_modules\\x\\cli.js\" %*\r\n) ELSE (\r\n  node  \"%~dp0%\\node_modules\\x\\cli.js\" %*\r\n)\r\n";
        let shim = write_shim(dir.path(), "pnpm.cmd", pnpm_style);
        assert_eq!(resolve_batch_shim(&shim.to_string_lossy()), None);

        let mut tampered: Vec<String> = NPM_CMD_SHIM_HEAD.iter().map(|s| s.to_string()).collect();
        tampered[0] = "@echo off".to_string();
        tampered.push(String::new());
        tampered.extend(NPM_CMD_SHIM_PROG_BLOCK.iter().map(|s| s.to_string()));
        tampered.push(String::new());
        tampered.push(modern_invocation("%dp0%\\node_modules\\x\\cli.js"));
        let shim = write_shim(dir.path(), "lowercase.cmd", &crlf_owned(&tampered));
        assert_eq!(resolve_batch_shim(&shim.to_string_lossy()), None);

        let extra_command = npm_shim(
            &modern_invocation("%dp0%\\node_modules\\x\\cli.js"),
            &["ENDLOCAL", "echo injected"],
        );
        let shim = write_shim(dir.path(), "extra.cmd", &extra_command);
        assert_eq!(resolve_batch_shim(&shim.to_string_lossy()), None);
    }

    #[test]
    fn rejects_unsafe_npm_shim_targets_and_invocations() {
        assert_eq!(
            parse_invocation("\"%_prog%\"  \"node_modules\\x.js\" %*"),
            None
        );
        assert_eq!(
            parse_invocation(&modern_invocation("%dp0%\\..\\escape.js")),
            None
        );
        assert_eq!(
            parse_invocation(&modern_invocation("%dp0%\\node_modules\\%VAR%.js")),
            None
        );
        assert_eq!(
            parse_invocation("\"%_prog%\" --shebang-arg \"%dp0%\\x.js\" %*"),
            None
        );
        assert_eq!(
            parse_invocation(&modern_invocation("%dp0%\\x.js").replace(" %*", " %* extra")),
            None
        );
    }

    #[test]
    fn rejects_deviating_forwarders_and_non_batch_programs() {
        let dir = tempfile::tempdir().unwrap();
        let two_line = "@\"node\" \"a.mjs\" %*\r\n@echo second\r\n";
        let shim = write_shim(dir.path(), "two.cmd", two_line);
        assert_eq!(resolve_batch_shim(&shim.to_string_lossy()), None);

        let trailing_junk = "@\"node\" \"a.mjs\" %* bare-arg\r\n";
        let shim = write_shim(dir.path(), "junk.cmd", trailing_junk);
        assert_eq!(resolve_batch_shim(&shim.to_string_lossy()), None);

        let exe = dir.path().join("agent.exe");
        std::fs::write(&exe, b"binary").unwrap();
        assert_eq!(resolve_batch_shim(&exe.to_string_lossy()), None);

        let oversized = dir.path().join("big.cmd");
        std::fs::write(&oversized, vec![b'@'; (SHIM_MAX_BYTES + 1) as usize]).unwrap();
        assert_eq!(resolve_batch_shim(&oversized.to_string_lossy()), None);
    }

    #[test]
    fn missing_shim_file_stays_unresolved() {
        assert_eq!(resolve_batch_shim("/nonexistent/agent.cmd"), None);
    }
}
