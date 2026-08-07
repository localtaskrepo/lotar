use crate::workspace::TasksDirectoryResolver;
use notify::{Config as NotifyConfig, EventKind, RecursiveMode, Watcher, recommended_watcher};
use serde_json::{Value, json};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use super::hints::{EnumHints, gather_enum_hints};
use super::{session_initialized, write_json_message};

#[derive(Debug)]
pub(super) enum ServerEvent {
    ToolsChanged { hint_categories: Vec<String> },
}

pub(super) fn spawn_event_dispatcher(
    receiver: mpsc::Receiver<ServerEvent>,
    stdout: Arc<Mutex<io::Stdout>>,
) {
    std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            match event {
                ServerEvent::ToolsChanged { hint_categories } => {
                    if !session_initialized() {
                        continue;
                    }
                    let notification = build_tools_changed_notification(&hint_categories);
                    if let Ok(line) = serde_json::to_string(&notification) {
                        write_json_message(&stdout, &line);
                    }
                }
            }
        }
    });
}

pub(super) fn start_tools_change_notifier(sender: mpsc::Sender<ServerEvent>) {
    let resolver = match TasksDirectoryResolver::resolve(None, None) {
        Ok(resolver) => resolver,
        Err(_) => return,
    };

    spawn_tools_dir_watcher(resolver.path, sender);
}

fn spawn_tools_dir_watcher(tasks_dir: PathBuf, sender: mpsc::Sender<ServerEvent>) {
    if !tasks_dir.exists() {
        return;
    }

    std::thread::spawn(move || {
        let mut previous_hints = gather_enum_hints();
        let (tx, rx) = mpsc::channel();

        // Best-effort kernel watcher for instant updates. Some runtimes (e.g.
        // sandboxed agent shells) forbid the underlying file-watch syscalls, so
        // this may be `None`; the periodic poll below still detects config/
        // tooling changes so `tools/listChanged` fires reliably everywhere.
        let _watcher = (|| {
            let mut watcher = recommended_watcher({
                let tx = tx.clone();
                move |result| {
                    let _ = tx.send(result);
                }
            })
            .ok()?;
            let _ = watcher.configure(NotifyConfig::default());
            let _ = watcher.watch(&tasks_dir, RecursiveMode::Recursive);
            Some(watcher)
        })();
        drop(tx);

        let mut last_emit: Option<Instant> = None;
        let poll_interval = Duration::from_secs(2);
        loop {
            match rx.recv_timeout(poll_interval) {
                Ok(result) => {
                    let Ok(event) = result else {
                        continue;
                    };
                    if matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) && event_affects_tooling(&event.paths, &tasks_dir)
                    {
                        emit_tools_change_if_changed(&sender, &mut previous_hints, &mut last_emit);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Polling fallback: catches changes the kernel watcher
                    // missed or never delivered.
                    emit_tools_change_if_changed(&sender, &mut previous_hints, &mut last_emit);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // Kernel watcher unavailable; keep polling on a timer.
                    std::thread::sleep(poll_interval);
                    emit_tools_change_if_changed(&sender, &mut previous_hints, &mut last_emit);
                }
            }
        }
    });
}

/// Re-gather enum hints and emit a `tools/listChanged` notification when they
/// differ from the last snapshot, debounced to coalesce event bursts.
fn emit_tools_change_if_changed(
    sender: &mpsc::Sender<ServerEvent>,
    previous_hints: &mut Option<EnumHints>,
    last_emit: &mut Option<Instant>,
) {
    let now = Instant::now();
    if last_emit
        .map(|instant| now.duration_since(instant) < Duration::from_millis(500))
        .unwrap_or(false)
    {
        return;
    }

    let next_hints = gather_enum_hints();
    let hint_categories = diff_hint_categories(previous_hints.as_ref(), next_hints.as_ref());
    if hint_categories.is_empty() {
        return;
    }

    *previous_hints = next_hints;
    *last_emit = Some(now);
    let _ = sender.send(ServerEvent::ToolsChanged { hint_categories });
}

pub(crate) fn event_affects_tooling(paths: &[PathBuf], tasks_dir: &Path) -> bool {
    if paths.is_empty() {
        return true;
    }

    paths.iter().any(|path| {
        if path == tasks_dir {
            return true;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("config.yml"))
            .unwrap_or(false)
        {
            return true;
        }
        if path.parent().is_some_and(|parent| parent == tasks_dir) {
            return true;
        }
        false
    })
}

pub(super) fn build_tools_changed_notification(hint_categories: &[String]) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "tools/listChanged",
        "params": {
            "hintCategories": hint_categories
        }
    })
}

pub(super) fn diff_hint_categories(
    previous: Option<&EnumHints>,
    current: Option<&EnumHints>,
) -> Vec<String> {
    let mut changed = Vec::new();
    if hint_values_changed(
        previous.map(|h| h.projects.as_slice()),
        current.map(|h| h.projects.as_slice()),
    ) {
        changed.push("projects".to_string());
    }
    if hint_values_changed(
        previous.map(|h| h.statuses.as_slice()),
        current.map(|h| h.statuses.as_slice()),
    ) {
        changed.push("statuses".to_string());
    }
    if hint_values_changed(
        previous.map(|h| h.priorities.as_slice()),
        current.map(|h| h.priorities.as_slice()),
    ) {
        changed.push("priorities".to_string());
    }
    if hint_values_changed(
        previous.map(|h| h.types.as_slice()),
        current.map(|h| h.types.as_slice()),
    ) {
        changed.push("types".to_string());
    }
    if hint_values_changed(
        previous.map(|h| h.members.as_slice()),
        current.map(|h| h.members.as_slice()),
    ) {
        changed.push("members".to_string());
    }
    if hint_values_changed(
        previous.map(|h| h.tags.as_slice()),
        current.map(|h| h.tags.as_slice()),
    ) {
        changed.push("tags".to_string());
    }
    if hint_values_changed(
        previous.map(|h| h.custom_fields.as_slice()),
        current.map(|h| h.custom_fields.as_slice()),
    ) {
        changed.push("custom_fields".to_string());
    }

    changed
}

fn hint_values_changed(previous: Option<&[String]>, current: Option<&[String]>) -> bool {
    match (previous, current) {
        (None, None) => false,
        (Some(prev), Some(curr)) => !eq_ignore_order(prev, curr),
        (Some(prev), None) => !prev.is_empty(),
        (None, Some(curr)) => !curr.is_empty(),
    }
}

fn eq_ignore_order(left: &[String], right: &[String]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut left_refs: Vec<&str> = left.iter().map(|value| value.as_str()).collect();
    let mut right_refs: Vec<&str> = right.iter().map(|value| value.as_str()).collect();
    left_refs.sort_unstable();
    right_refs.sort_unstable();
    left_refs == right_refs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_hints() -> EnumHints {
        EnumHints {
            projects: vec!["MCP".to_string()],
            statuses: vec!["Todo".to_string()],
            priorities: vec!["Low".to_string()],
            types: vec!["Feature".to_string()],
            members: vec!["alice".to_string()],
            tags: vec!["cli".to_string()],
            custom_fields: vec!["severity".to_string()],
        }
    }

    #[test]
    fn diff_hint_categories_detects_field_changes() {
        let previous = base_hints();
        let mut current = base_hints();
        current.statuses = vec!["Done".to_string()];
        current.tags.push("infra".to_string());
        let diff = diff_hint_categories(Some(&previous), Some(&current));
        assert_eq!(diff, vec!["statuses".to_string(), "tags".to_string()]);
    }

    #[test]
    fn diff_hint_categories_handles_missing_snapshots() {
        let current = base_hints();
        let diff = diff_hint_categories(None, Some(&current));
        assert_eq!(
            diff,
            vec![
                "projects".to_string(),
                "statuses".to_string(),
                "priorities".to_string(),
                "types".to_string(),
                "members".to_string(),
                "tags".to_string(),
                "custom_fields".to_string(),
            ]
        );
    }

    #[test]
    fn build_notification_embeds_hint_categories() {
        let categories = vec!["statuses".to_string(), "tags".to_string()];
        let notification = build_tools_changed_notification(&categories);
        assert_eq!(
            notification.get("method").and_then(|v| v.as_str()),
            Some("tools/listChanged")
        );
        let params = notification
            .get("params")
            .and_then(|v| v.as_object())
            .unwrap();
        let hints = params
            .get("hintCategories")
            .and_then(|value| value.as_array())
            .unwrap();
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].as_str(), Some("statuses"));
        assert_eq!(hints[1].as_str(), Some("tags"));
    }
}
