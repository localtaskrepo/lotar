//! `lotar task timeline <id>` (DEV-518): what happened on a task, oldest first.

use crate::api_types::{AgentJob, TaskTimeline, TaskTimelineEntry};
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::task_timeline::TaskTimelineService;
use crate::workspace::TasksDirectoryResolver;
use chrono::{DateTime, Utc};

pub fn handle_timeline(id: &str, threads: bool, project: Option<&str>, resolver: &TasksDirectoryResolver, renderer: &OutputRenderer) -> Result<(), String> {
    let project = project.map(|p| crate::utils::project::resolve_project_input(p, resolver.path.as_path()));
    // A bare number means a task of the current project, as everywhere else (DEV-283).
    let id = if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
        let default = project.clone().unwrap_or_else(|| crate::project::get_effective_project_name(resolver));
        format!("{default}-{id}")
    } else {
        id.to_string()
    };
    let timeline = TaskTimelineService::build(&resolver.path, &id, project.as_deref(), threads).map_err(|e| e.to_string())?;
    match renderer.format {
        OutputFormat::Json => renderer.emit_json(&timeline),
        OutputFormat::Text => renderer.emit_raw_stdout(render_timeline_text(&timeline)),
    }
    Ok(())
}

/// One block per entry: time, kind and who, then the text indented.
pub fn render_timeline_text(timeline: &TaskTimeline) -> String {
    if timeline.entries.is_empty() {
        return format!("{}: nothing happened yet\n", timeline.task_id);
    }
    let mut out = String::new();
    for entry in &timeline.entries {
        let (kind, head, body) = match entry {
            TaskTimelineEntry::Change { actor, changes, .. } => {
                let list = changes.iter().map(describe_change).collect::<Vec<_>>().join("; ");
                ("change", format!("{}: {list}", actor.as_deref().unwrap_or("unknown")), Vec::new())
            }
            TaskTimelineEntry::Comment { comment, .. } => ("comment", comment.author.clone().unwrap_or_else(|| "unknown".into()), vec![comment.text.clone()]),
            TaskTimelineEntry::Commit { commit, author, message, .. } => {
                let short = commit.get(..8).unwrap_or(commit);
                ("commit", format!("{author} {short}: {}", message.lines().next().unwrap_or_default()), Vec::new())
            }
            TaskTimelineEntry::Job { job, thread, .. } => {
                let mut body: Vec<String> = job.summary.iter().cloned().collect();
                for event in thread.iter().flatten() {
                    let Some(message) = event.message.as_deref().map(str::trim).filter(|m| !m.is_empty()) else {
                        continue;
                    };
                    body.push(format!("[{} {}] {message}", short_time(&event.at), event.kind.trim_start_matches("agent_job_")));
                }
                ("job", describe_job(job), body)
            }
        };
        out.push_str(&format!("{}  {kind:<7}  {head}\n", short_time(entry.at())));
        for text in body {
            for line in text.trim_end().lines() {
                out.push_str(&format!("    {line}\n"));
            }
        }
    }
    out
}

fn describe_change(change: &crate::types::TaskChange) -> String {
    match (change.old.as_deref(), change.new.as_deref()) {
        (Some(old), Some(new)) => format!("{} {old} → {new}", change.field),
        (None, Some(new)) => format!("{} → {new}", change.field),
        (Some(old), None) => format!("{} {old} → (cleared)", change.field),
        (None, None) => change.field.clone(),
    }
}

/// "fix (t3, claude-opus-5-5) completed in 3m 05s, $0.1234 [job-…]".
fn describe_job(job: &AgentJob) -> String {
    let settings = [Some(job.runner.as_str()), job.model.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(", ");
    let mut text = format!("{} ({settings}) {}", job.agent.as_deref().unwrap_or("agent"), job.status);
    if let (Some(start), Some(end)) = (job.started_at.as_deref().and_then(parse), job.finished_at.as_deref().and_then(parse)) {
        text.push_str(&format!(" in {}", format_duration((end - start).num_seconds())));
    }
    if let Some(cost) = job.usage.as_ref().and_then(|usage| usage.cost_usd) {
        text.push_str(&format!(", ${cost:.4}"));
    }
    text.push_str(&format!(" [{}]", job.id));
    text
}

fn parse(at: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(at).ok().map(|time| time.with_timezone(&Utc))
}

fn short_time(at: &str) -> String {
    parse(at).map_or_else(|| at.to_string(), |time| time.format("%Y-%m-%d %H:%MZ").to_string())
}

/// "45s", "3m 05s", "1h 02m", as the web UI writes durations.
fn format_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h {:02}m", seconds / 3600, (seconds / 60) % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_lists_each_entry_with_its_body_indented() {
        let job: AgentJob = serde_json::from_value(serde_json::json!({
            "id": "job-1", "ticket_id": "T-1", "runner": "claude", "agent": "fix", "status": "completed",
            "created_at": "2026-01-01T10:00:00Z", "started_at": "2026-01-01T10:00:00Z", "finished_at": "2026-01-01T10:03:05Z",
            "model": "sonnet", "summary": "Fixed it", "usage": {"cost_usd": 0.5}
        }))
        .unwrap();
        let thread = vec![crate::services::agent_job_service::AgentJobEvent {
            kind: "agent_job_message".into(),
            sequence: 1,
            at: "2026-01-01T10:01:00Z".into(),
            message: Some("Looking".into()),
        }];
        let timeline = TaskTimeline {
            task_id: "T-1".into(),
            entries: vec![
                TaskTimelineEntry::Change {
                    at: "2026-01-01T09:00:00Z".into(),
                    actor: Some("ann".into()),
                    changes: vec![crate::types::TaskChange { field: "status".into(), old: Some("Todo".into()), new: Some("Doing".into()) }],
                },
                TaskTimelineEntry::Job { at: "2026-01-01T10:00:00Z".into(), job, thread: Some(thread) },
            ],
        };
        let text = render_timeline_text(&timeline);
        assert_eq!(
            text,
            "2026-01-01 09:00Z  change   ann: status Todo → Doing\n\
             2026-01-01 10:00Z  job      fix (claude, sonnet) completed in 3m 05s, $0.5000 [job-1]\n    \
             Fixed it\n    \
             [2026-01-01 10:01Z message] Looking\n"
        );
    }
}
