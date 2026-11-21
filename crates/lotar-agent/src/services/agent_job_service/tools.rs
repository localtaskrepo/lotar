//! A job's tool calls as `agent_job_tool` log entries (DEV-766).
//!
//! harnessmux 0.2 gives a tool's id, title, kind and status; its input, paths and output are only in the event's
//! `raw` ACP update (`rawInput`, `locations`, `content`, `rawOutput`), read here.

use crate::api_types::AgentJobTool;
use serde_json::Value;
use std::collections::HashMap;

/// Bytes of a tool's output kept in the log.
pub(super) const OUTPUT_LIMIT: usize = 4 * 1024;
/// Bytes of a tool's input JSON kept in the log.
pub(super) const RAW_INPUT_LIMIT: usize = 1024;

/// The tool calls of a job's turns that started and haven't ended.
#[derive(Default)]
pub(super) struct ToolCalls {
    running: HashMap<String, AgentJobTool>,
}

impl ToolCalls {
    /// The running entry of a `tool.started` event.
    pub(super) fn started(&mut self, id: String, title: &str, kind: Option<String>, raw: Option<&Value>, at: String, prefixes: &[String]) -> AgentJobTool {
        let update = raw.and_then(acp_update);
        let mut tool =
            AgentJobTool { id: id.clone(), kind, title: clean(title, prefixes), status: "running".to_string(), started_at: Some(at), ..Default::default() };
        if let Some(update) = update {
            fill_input(&mut tool, update, prefixes);
        }
        self.running.insert(id, tool.clone());
        // Only the end entry carries the input JSON, so the log holds it once.
        AgentJobTool { raw_input: None, ..tool }
    }

    /// The end entry of a `tool.finished` event: what the start said, with what the end adds.
    pub(super) fn finished(&mut self, id: String, status: &str, title: Option<&str>, raw: Option<&Value>, at: String, prefixes: &[String]) -> AgentJobTool {
        let mut tool = self.running.remove(&id).unwrap_or_else(|| AgentJobTool { id, ..Default::default() });
        tool.status = if status == "failed" { "failed" } else { "completed" }.to_string();
        tool.finished_at = Some(at);
        if let Some(title) = title.map(|title| clean(title, prefixes)).filter(|title| !title.trim().is_empty()) {
            tool.title = title;
        }
        if let Some(update) = raw.and_then(acp_update) {
            if tool.kind.is_none() {
                tool.kind = update.get("kind").and_then(Value::as_str).map(str::to_string);
            }
            fill_input(&mut tool, update, prefixes);
            if let Some(output) = output_of(update) {
                let (output, full) = cap(&strip_fence(&strip_ansi(&clean(&output, prefixes))), OUTPUT_LIMIT);
                tool.output = Some(output);
                tool.output_bytes = full;
            }
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

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// The ACP `session/update` an event's `raw` holds.
fn acp_update(raw: &Value) -> Option<&Value> {
    raw.get("update").filter(|update| update.is_object())
}

/// Input and paths from an update, unless an earlier one gave them (the first non-empty input wins).
fn fill_input(tool: &mut AgentJobTool, update: &Value, prefixes: &[String]) {
    if tool.paths.is_empty() {
        tool.paths = update
            .get("locations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|location| location.get("path").and_then(Value::as_str))
            .map(|path| clean(path, prefixes))
            .collect();
    }
    let input = update.get("rawInput").filter(|input| !input.is_null() && input.as_object().is_none_or(|map| !map.is_empty()));
    if let Some(input) = input.filter(|_| tool.raw_input.is_none()) {
        tool.raw_input = Some(cap(&clean(&input.to_string(), prefixes), RAW_INPUT_LIMIT).0);
        // A path stood in for the input until the input came.
        if tool.input.is_none() || tool.input == tool.paths.first().cloned() {
            tool.input = summary(input, tool.kind.as_deref()).map(|text| clean(&text, prefixes)).or(tool.input.take());
        }
    }
    if tool.input.is_none() {
        tool.input = tool.paths.first().cloned();
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

/// A finished tool's output: its text `content` items, else `rawOutput` (a string, or other JSON as compact JSON).
fn output_of(update: &Value) -> Option<String> {
    let texts: Vec<&str> = update
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("content"))
        .filter_map(|item| item.get("content").filter(|block| block.get("type").and_then(Value::as_str) == Some("text")))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect();
    let output = if texts.is_empty() {
        match update.get("rawOutput") {
            Some(Value::String(text)) => text.clone(),
            None | Some(Value::Null) => String::new(),
            Some(other) => other.to_string(),
        }
    } else {
        texts.join("\n")
    };
    // Codex's last output chunk comes with the end (codex-acp `_meta.terminal_output_delta`).
    let output = if output.trim().is_empty() {
        update.get("_meta").and_then(|meta| meta.get("terminal_output_delta")).and_then(|delta| delta.get("data")).and_then(Value::as_str)?.to_string()
    } else {
        output
    };
    (!output.trim().is_empty()).then_some(output)
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

    #[test]
    fn a_claude_bash_call_keeps_its_output_without_the_fence_and_the_start_entry() {
        let mut calls = ToolCalls::default();
        let start = json!({"update": {"sessionUpdate": "tool_call", "toolCallId": "toolu_1", "title": "Terminal", "kind": "execute", "rawInput": {}}});
        let running = calls.started("toolu_1".into(), "Terminal", Some("execute".into()), Some(&start), AT.into(), &none());
        assert_eq!(running.status, "running");
        assert_eq!((running.input.as_deref(), running.raw_input.as_deref()), (None, None), "Claude's first rawInput is {{}}");
        let end = json!({"update": {"sessionUpdate": "tool_call_update", "toolCallId": "toolu_1", "status": "completed",
            "content": [{"type": "content", "content": {"type": "text", "text": "```console\nmain\n77e3175 LOCA-535\n```"}}],
            "rawOutput": "main\n77e3175 LOCA-535"}});
        let done = calls.finished("toolu_1".into(), "completed", None, Some(&end), "2026-11-21T21:13:25.983Z".into(), &none());
        assert_eq!(done.title, "Terminal");
        assert_eq!(done.kind.as_deref(), Some("execute"));
        assert_eq!(done.status, "completed");
        assert_eq!(done.output.as_deref(), Some("main\n77e3175 LOCA-535"));
        assert_eq!(done.output_bytes, None);
        assert_eq!(done.started_at.as_deref(), Some(AT));
        assert_eq!(done.finished_at.as_deref(), Some("2026-11-21T21:13:25.983Z"));
        assert_eq!(tool_line(&done), "Terminal");
    }

    #[test]
    fn a_codex_command_is_its_input_and_a_failed_end_names_it_with_its_output() {
        let mut calls = ToolCalls::default();
        let start = json!({"update": {"sessionUpdate": "tool_call", "toolCallId": "exec-1", "title": "cargo test", "kind": "execute",
            "status": "in_progress", "rawInput": {"command": "cargo test", "cwd": "/work/repo"}}});
        let running = calls.started("exec-1".into(), "cargo test", Some("execute".into()), Some(&start), AT.into(), &["/work/repo/".to_string()]);
        assert_eq!(running.input.as_deref(), Some("cargo test"));
        assert_eq!(running.raw_input, None, "the input JSON goes on the end entry only");
        let end = json!({"update": {"sessionUpdate": "tool_call_update", "toolCallId": "exec-1", "status": "failed",
            "_meta": {"terminal_output_delta": {"data": "test add ... \u{1b}[31mFAILED\u{1b}[0m\n", "terminal_id": "exec-1"}}}});
        let failed = calls.finished("exec-1".into(), "failed", None, Some(&end), AT.into(), &none());
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.raw_input.as_deref(), Some(r#"{"command":"cargo test","cwd":"/work/repo"}"#));
        assert_eq!(failed.output.as_deref(), Some("test add ... FAILED\n"));
        assert_eq!(tool_line(&failed), "Failed: cargo test", "the title is the command already");
    }

    #[test]
    fn read_and_edit_calls_show_their_path_from_input_or_locations() {
        let mut calls = ToolCalls::default();
        let read = json!({"update": {"toolCallId": "r", "kind": "read", "rawInput": {"file_path": "/repo/src/a.rs", "limit": 20}}});
        let tool = calls.started("r".into(), "Read File", Some("read".into()), Some(&read), AT.into(), &["/repo/".to_string()]);
        assert_eq!(tool.input.as_deref(), Some("src/a.rs"));
        assert_eq!(tool_line(&tool), "Read File: src/a.rs");

        let edit = json!({"update": {"toolCallId": "e", "kind": "edit", "locations": [{"path": "/repo/b.ts", "line": 3}]}});
        let tool = calls.started("e".into(), "Edit", Some("edit".into()), Some(&edit), AT.into(), &["/repo/".to_string()]);
        assert_eq!(tool.paths, vec!["b.ts"]);
        assert_eq!(tool.input.as_deref(), Some("b.ts"));

        let search = json!({"update": {"toolCallId": "s", "kind": "search", "rawInput": {"pattern": "fn main", "path": "src"}}});
        let tool = calls.started("s".into(), "grep", Some("search".into()), Some(&search), AT.into(), &none());
        assert_eq!(tool.input.as_deref(), Some("fn main"));
    }

    #[test]
    fn an_end_without_its_start_has_what_the_end_carries() {
        let mut calls = ToolCalls::default();
        let end = json!({"update": {"toolCallId": "x", "status": "failed", "kind": "execute", "rawOutput": {"exit_code": 2}}});
        let tool = calls.finished("x".into(), "failed", Some("ls /nope"), Some(&end), AT.into(), &none());
        assert_eq!((tool.title.as_str(), tool.kind.as_deref(), tool.started_at.as_deref()), ("ls /nope", Some("execute"), None));
        assert_eq!(tool.output.as_deref(), Some(r#"{"exit_code":2}"#), "JSON rawOutput as compact JSON");

        let bare = calls.finished("y".into(), "completed", None, None, AT.into(), &none());
        assert_eq!((bare.title.as_str(), bare.output.as_deref()), ("Tool", None), "no title, kind or output known");
    }

    #[test]
    fn output_is_cut_at_the_limit_on_a_character_boundary_with_the_full_size() {
        let mut calls = ToolCalls::default();
        let long = format!("{}é{}", "a".repeat(OUTPUT_LIMIT - 1), "b".repeat(5000));
        let end = json!({"update": {"toolCallId": "big", "rawOutput": long}});
        let tool = calls.finished("big".into(), "completed", Some("Terminal"), Some(&end), AT.into(), &none());
        let output = tool.output.unwrap();
        assert_eq!(output.len(), OUTPUT_LIMIT - 1, "é would cross the limit");
        assert_eq!(tool.output_bytes, Some(long.len() as u64));

        let input = json!({"update": {"toolCallId": "in", "rawInput": {"command": "x".repeat(3000)}}});
        let tool = calls.started("in".into(), "Terminal", Some("execute".into()), Some(&input), AT.into(), &none());
        let tool = calls.finished(tool.id, "completed", None, None, AT.into(), &none());
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
