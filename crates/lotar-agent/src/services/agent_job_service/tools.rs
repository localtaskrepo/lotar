//! A job's tool calls as `agent_job_tool` log entries (DEV-766), from harnessmux's `tool.started`, `tool.updated` and
//! `tool.finished` fields: input, paths and output (DEV-774).

use crate::api_types::AgentJobTool;
use serde_json::Value;
use std::collections::HashMap;

/// Bytes of a tool's output kept in the log.
pub(super) const OUTPUT_LIMIT: usize = 4 * 1024;
/// Bytes of a tool's input JSON kept in the log.
pub(super) const RAW_INPUT_LIMIT: usize = 1024;

/// What a tool event says about the call; a field it doesn't carry is `None` or empty.
#[derive(Default)]
pub(super) struct ToolFields {
    pub title: Option<String>,
    pub kind: Option<String>,
    pub input: Option<Value>,
    pub locations: Vec<String>,
}

/// The tool calls of a job's turns that started and haven't ended.
#[derive(Default)]
pub(super) struct ToolCalls {
    running: HashMap<String, AgentJobTool>,
}

impl ToolCalls {
    /// The running entry of a `tool.started` event.
    pub(super) fn started(&mut self, id: String, fields: ToolFields, at: String, prefixes: &[String]) -> AgentJobTool {
        let mut tool = AgentJobTool { id: id.clone(), status: "running".to_string(), started_at: Some(at), ..Default::default() };
        apply(&mut tool, fields, prefixes);
        self.running.insert(id, tool.clone());
        // Only the end entry carries the input JSON, so the log holds it once.
        AgentJobTool { raw_input: None, ..tool }
    }

    /// A `tool.updated` event: the running call takes its title, kind, input and paths. Returns a new running entry
    /// when what a reader sees changed (title, input or paths), as Claude's command comes only here.
    pub(super) fn updated(&mut self, id: String, fields: ToolFields, prefixes: &[String]) -> Option<AgentJobTool> {
        let tool = self.running.entry(id.clone()).or_insert_with(|| AgentJobTool { id, status: "running".to_string(), ..Default::default() });
        let seen = (tool.title.clone(), tool.input.clone(), tool.paths.clone());
        apply(tool, fields, prefixes);
        (seen != (tool.title.clone(), tool.input.clone(), tool.paths.clone())).then(|| AgentJobTool { raw_input: None, ..tool.clone() })
    }

    /// The end entry of a `tool.finished` event: what the start and updates said, with the end's title and output.
    pub(super) fn finished(&mut self, id: String, status: &str, title: Option<String>, output: Option<&str>, at: String, prefixes: &[String]) -> AgentJobTool {
        let mut tool = self.running.remove(&id).unwrap_or_else(|| AgentJobTool { id, ..Default::default() });
        tool.status = if status == "failed" { "failed" } else { "completed" }.to_string();
        tool.finished_at = Some(at);
        apply(&mut tool, ToolFields { title, ..ToolFields::default() }, prefixes);
        if let Some(output) = output {
            let (output, dropped) = harnessmux_cut(output);
            let output = strip_ansi(&clean(output, prefixes));
            // A cut output lost its closing fence.
            let output = if dropped == 0 { strip_fence(&output) } else { strip_opening_fence(&output) };
            let (kept, full) = cap(&output, OUTPUT_LIMIT);
            tool.output = Some(kept).filter(|kept| !kept.trim().is_empty());
            tool.output_bytes = if dropped == 0 { full } else { Some(output.len() as u64 + dropped) };
        }
        if tool.title.trim().is_empty() {
            tool.title = tool.kind.clone().unwrap_or_else(|| "Tool".to_string());
        }
        tool
    }
}

/// The plain line of a tool entry for readers without `tool`: `<title>: <input>`, `Failed: ` in front of a failed one.
pub(super) fn tool_line(tool: &AgentJobTool) -> String {
    let what = match tool.input.as_deref().or(tool.paths.first().map(String::as_str)) {
        Some(input) if input != tool.title => format!("{}: {}", tool.title, first_line(input)),
        _ => tool.title.clone(),
    };
    if tool.status == "failed" { format!("Failed: {what}") } else { what }
}

/// The plain text of an `agent_job_plan` entry: a Markdown task list, the running step marked.
pub(super) fn plan_text(plan: &[crate::api_types::AgentJobPlanEntry]) -> String {
    if plan.is_empty() {
        return "Plan cleared".to_string();
    }
    let line = |entry: &crate::api_types::AgentJobPlanEntry| match entry.status.as_str() {
        "completed" => format!("- [x] {}", entry.content),
        "in_progress" => format!("- [ ] {} (in progress)", entry.content),
        _ => format!("- [ ] {}", entry.content),
    };
    plan.iter().map(line).collect::<Vec<_>>().join("\n")
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// What an event carries replaces what the call had: a non-empty title, a kind, paths, and the input (each harnessmux
/// update holds all input known so far). The input for people is summarised from the input JSON, else the first path.
fn apply(tool: &mut AgentJobTool, fields: ToolFields, prefixes: &[String]) {
    if let Some(title) = fields.title.map(|title| clean(&title, prefixes)).filter(|title| !title.trim().is_empty()) {
        tool.title = title;
    }
    if fields.kind.is_some() {
        tool.kind = fields.kind;
    }
    if !fields.locations.is_empty() {
        tool.paths = fields.locations.iter().map(|path| clean(path, prefixes)).collect();
    }
    if let Some(input) = fields.input {
        tool.raw_input = Some(cap(&clean(&input.to_string(), prefixes), RAW_INPUT_LIMIT).0);
        if let Some(summary) = summary(&input, tool.kind.as_deref()) {
            tool.input = Some(clean(&summary, prefixes));
        }
    }
    // A path stands in for the input until the input comes.
    if tool.input.is_none() || tool.raw_input.is_none() {
        tool.input = tool.paths.first().cloned().or(tool.input.take());
    }
}

/// What a person wants to see of a tool's input: the command, the path, the query.
fn summary(input: &Value, kind: Option<&str>) -> Option<String> {
    let text = |key: &str| input.get(key).and_then(Value::as_str).map(str::to_string);
    let command = || match input.get("command") {
        Some(Value::String(command)) => Some(command.clone()),
        // Codex sends some commands as argv.
        Some(Value::Array(argv)) => Some(argv.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")),
        _ => None,
    };
    let path = || ["file_path", "path", "notebook_path"].into_iter().find_map(text);
    let query = || ["query", "pattern", "url", "prompt", "description"].into_iter().find_map(text);
    match kind {
        Some("execute") => command().or_else(path),
        Some("read" | "edit" | "delete" | "move") => path().or_else(command),
        Some("search" | "fetch") => query().or_else(path),
        _ => command().or_else(path).or_else(query),
    }
    .filter(|text| !text.trim().is_empty())
}

/// harnessmux's output without the `… [N more bytes]` it ends a cut one with (its `OUTPUT_LIMIT`, 16 KB), and N.
fn harnessmux_cut(output: &str) -> (&str, u64) {
    output
        .strip_suffix(" more bytes]")
        .and_then(|rest| rest.rsplit_once("… ["))
        .and_then(|(text, more)| Some((text, more.parse().ok()?)))
        .unwrap_or((output, 0))
}

fn clean(text: &str, prefixes: &[String]) -> String {
    super::strip_prefixes(text, prefixes)
}

/// `text` without ANSI escape sequences (CSI such as colours, OSC such as links and titles, and two-byte escapes).
pub(super) fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('[') => {
                // Parameters and intermediates, then one final byte in @..~.
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                // Ends with BEL or ESC \.
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            // Character set designations (`ESC ( B`) take one more character.
            Some('(' | ')' | '*' | '+' | '-' | '.' | '/' | '#' | '%') => {
                chars.next();
            }
            _ => {}
        }
    }
    out
}

/// The text inside one Markdown code fence that is all of `text` (claude-agent-acp wraps output in ```` ```console ````).
fn strip_fence(text: &str) -> String {
    let trimmed = text.trim();
    let inner = trimmed
        .strip_prefix("```")
        .and_then(|rest| rest.strip_suffix("```"))
        .and_then(|inner| inner.split_once('\n'))
        .map(|(lang, body)| (lang, body.strip_suffix('\n').unwrap_or(body)))
        .filter(|(lang, body)| !lang.contains(' ') && !body.contains("\n```"));
    match inner {
        Some((_, body)) => body.to_string(),
        None => text.to_string(),
    }
}

/// `text` without the opening fence line of a fenced output whose end was cut off.
fn strip_opening_fence(text: &str) -> String {
    let body = text.trim_start().strip_prefix("```").and_then(|rest| rest.split_once('\n')).filter(|(lang, _)| !lang.contains(' ')).map(|(_, body)| body);
    body.unwrap_or(text).to_string()
}

/// `text` cut to `limit` bytes on a character boundary, and its full size when it was cut.
pub(super) fn cap(text: &str, limit: usize) -> (String, Option<u64>) {
    if text.len() <= limit {
        return (text.to_string(), None);
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), Some(text.len() as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const AT: &str = "2026-11-21T21:13:24.297Z";

    fn none() -> Vec<String> {
        Vec::new()
    }

    fn fields(title: Option<&str>, kind: Option<&str>, input: Option<Value>, locations: &[&str]) -> ToolFields {
        ToolFields {
            title: title.map(str::to_string),
            kind: kind.map(str::to_string),
            input,
            locations: locations.iter().map(|path| path.to_string()).collect(),
        }
    }

    /// claude-agent-acp's events through harnessmux 0.3, as its live test recorded them: `Terminal` with no input,
    /// then `tool.updated` with the command as title and input, then the input with its description.
    #[test]
    fn a_claude_bash_call_names_its_command_from_tool_updated() {
        let mut calls = ToolCalls::default();
        let running = calls.started("toolu_1".into(), fields(Some("Terminal"), Some("execute"), None, &[]), AT.into(), &none());
        assert_eq!((running.title.as_str(), running.input.as_deref()), ("Terminal", None));

        let named = calls.updated("toolu_1".into(), fields(Some("echo pong"), None, Some(json!({"command": "echo pong"})), &[]), &none()).unwrap();
        assert_eq!((named.status.as_str(), named.title.as_str(), named.input.as_deref()), ("running", "echo pong", Some("echo pong")));
        assert_eq!(named.started_at.as_deref(), Some(AT));
        assert_eq!(named.raw_input, None, "the input JSON goes on the end entry only");
        assert_eq!(tool_line(&named), "echo pong");
        let described = json!({"command": "echo pong", "description": "Print pong"});
        assert!(calls.updated("toolu_1".into(), fields(None, None, Some(described), &[]), &none()).is_none(), "nothing a reader sees changed");

        let done = calls.finished("toolu_1".into(), "completed", None, Some("```console\npong\n```"), "2026-11-21T21:13:25.983Z".into(), &none());
        assert_eq!((done.title.as_str(), done.kind.as_deref(), done.input.as_deref()), ("echo pong", Some("execute"), Some("echo pong")));
        assert_eq!(done.raw_input.as_deref(), Some(r#"{"command":"echo pong","description":"Print pong"}"#), "the newest input wins");
        assert_eq!((done.output.as_deref(), done.output_bytes), (Some("pong"), None));
        assert_eq!(done.started_at.as_deref(), Some(AT));
        assert_eq!(done.finished_at.as_deref(), Some("2026-11-21T21:13:25.983Z"));
    }

    #[test]
    fn a_claude_read_and_edit_show_their_path_from_the_update() {
        let mut calls = ToolCalls::default();
        let prefixes = ["/repo/".to_string()];
        calls.started("r".into(), fields(Some("Read File"), Some("read"), None, &[]), AT.into(), &prefixes);
        let read = json!({"file_path": "/repo/src/a.rs", "limit": 20});
        let tool = calls.updated("r".into(), fields(Some("Read /repo/src/a.rs"), None, Some(read), &["/repo/src/a.rs"]), &prefixes).unwrap();
        assert_eq!(
            (tool.title.as_str(), tool.input.as_deref(), tool.paths.as_slice()),
            ("Read src/a.rs", Some("src/a.rs"), ["src/a.rs".to_string()].as_slice())
        );
        assert_eq!(tool_line(&calls.finished("r".into(), "completed", None, Some("fn a() {}"), AT.into(), &prefixes)), "Read src/a.rs: src/a.rs");

        calls.started("e".into(), fields(Some("Edit"), Some("edit"), None, &[]), AT.into(), &prefixes);
        let tool = calls.updated("e".into(), fields(None, None, None, &["/repo/b.ts"]), &prefixes).unwrap();
        assert_eq!((tool.paths.as_slice(), tool.input.as_deref()), (["b.ts".to_string()].as_slice(), Some("b.ts")), "a path stands in for the input");
        let tool = calls.updated("e".into(), fields(None, None, Some(json!({"file_path": "/repo/b.ts", "old_string": "a"})), &[]), &prefixes);
        assert!(tool.is_none(), "the input names the same path");
        let failed = calls.finished("e".into(), "failed", None, Some("old_string not found"), AT.into(), &prefixes);
        assert_eq!((tool_line(&failed).as_str(), failed.output.as_deref()), ("Failed: Edit: b.ts", Some("old_string not found")));
    }

    /// codex-acp's command comes with its start; harnessmux joins its terminal output chunks into the end's `output`.
    #[test]
    fn a_codex_command_is_its_input_and_a_failed_end_has_the_joined_terminal_output() {
        let mut calls = ToolCalls::default();
        let input = json!({"command": ["cargo", "test"], "cwd": "/work/repo"});
        let running = calls.started("exec-1".into(), fields(Some("cargo test"), Some("execute"), Some(input), &[]), AT.into(), &["/work/repo/".to_string()]);
        assert_eq!((running.input.as_deref(), running.raw_input.as_deref()), (Some("cargo test"), None));
        let failed = calls.finished("exec-1".into(), "failed", None, Some("Compiling\ntest add ... \u{1b}[31mFAILED\u{1b}[0m\n"), AT.into(), &none());
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.raw_input.as_deref(), Some(r#"{"command":["cargo","test"],"cwd":"/work/repo"}"#));
        assert_eq!(failed.output.as_deref(), Some("Compiling\ntest add ... FAILED\n"));
        assert_eq!(tool_line(&failed), "Failed: cargo test", "the title is the command already");
    }

    #[test]
    fn a_search_shows_its_query() {
        let mut calls = ToolCalls::default();
        let tool = calls.started("s".into(), fields(Some("grep"), Some("search"), Some(json!({"pattern": "fn main", "path": "src"})), &[]), AT.into(), &none());
        assert_eq!(tool.input.as_deref(), Some("fn main"));
    }

    #[test]
    fn an_update_or_end_without_its_start_has_what_it_carries() {
        let mut calls = ToolCalls::default();
        let tool = calls.finished("x".into(), "failed", Some("ls /nope".into()), Some(r#"{"exit_code":2}"#), AT.into(), &none());
        assert_eq!((tool.title.as_str(), tool.kind, tool.started_at), ("ls /nope", None, None));
        assert_eq!(tool.output.as_deref(), Some(r#"{"exit_code":2}"#));

        let bare = calls.finished("y".into(), "completed", None, None, AT.into(), &none());
        assert_eq!((bare.title.as_str(), bare.output.as_deref()), ("Tool", None), "no title, kind or output known");

        let updated = calls.updated("z".into(), fields(Some("cat a"), Some("execute"), Some(json!({"command": "cat a"})), &[]), &none()).unwrap();
        assert_eq!((updated.status.as_str(), updated.started_at.as_deref()), ("running", None));
        let done = calls.finished("z".into(), "completed", None, Some("   \n"), AT.into(), &none());
        assert_eq!((done.input.as_deref(), done.output.as_deref()), (Some("cat a"), None), "blank output is left out");
    }

    #[test]
    fn output_is_cut_at_the_limit_on_a_character_boundary_with_the_full_size() {
        let mut calls = ToolCalls::default();
        let long = format!("{}é{}", "a".repeat(OUTPUT_LIMIT - 1), "b".repeat(5000));
        let tool = calls.finished("big".into(), "completed", Some("Terminal".into()), Some(&long), AT.into(), &none());
        assert_eq!(tool.output.unwrap().len(), OUTPUT_LIMIT - 1, "é would cross the limit");
        assert_eq!(tool.output_bytes, Some(long.len() as u64));

        // harnessmux cut it at 16 KB already: the full size counts the bytes it left out.
        let cut = format!("{}… [9000 more bytes]", "c".repeat(16 * 1024));
        let tool = calls.finished("huge".into(), "completed", None, Some(&cut), AT.into(), &none());
        assert_eq!((tool.output.map(|output| output.len()), tool.output_bytes), (Some(OUTPUT_LIMIT), Some(16 * 1024 + 9000)));
        let tool = calls.finished("small".into(), "completed", None, Some("```console\nok… [5 more bytes]"), AT.into(), &none());
        assert_eq!((tool.output.as_deref(), tool.output_bytes), (Some("ok"), Some(7)), "a cut fenced output without its opening fence");

        calls.started("in".into(), fields(Some("Terminal"), Some("execute"), Some(json!({"command": "x".repeat(3000)})), &[]), AT.into(), &none());
        let tool = calls.finished("in".into(), "completed", None, None, AT.into(), &none());
        assert_eq!(tool.raw_input.map(|raw| raw.len()), Some(RAW_INPUT_LIMIT));
    }

    #[test]
    fn ansi_codes_and_osc_links_are_removed() {
        assert_eq!(strip_ansi("\u{1b}[1;32mok\u{1b}[0m done\u{1b}]8;;https://x\u{7}link\u{1b}]8;;\u{1b}\\ \u{1b}(B."), "ok donelink .");
        assert_eq!(strip_ansi("plain ü"), "plain ü");
    }

    #[test]
    fn only_a_fence_around_the_whole_output_is_removed() {
        assert_eq!(strip_fence("```\n1\tline\n```"), "1\tline");
        assert_eq!(strip_fence("text\n```\ncode\n```"), "text\n```\ncode\n```");
        assert_eq!(strip_fence("```a\nx\n```\n```b\ny\n```"), "```a\nx\n```\n```b\ny\n```", "two fences stay");
    }
}
