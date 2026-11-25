//! `lotar show <id>` (DEV-18): one task with everything on it.

use crate::api_types::TaskDTO;
use crate::cli::args::TaskShowArgs;
use crate::cli::handlers::comment::numbered_comment_json;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::mutation::load_task_including_deleted;
use crate::output::{OutputFormat, OutputRenderer};
use crate::types::TaskComment;
use crate::workspace::TasksDirectoryResolver;

/// How long a comment's line in `--brief` may get (DEV-884).
const BRIEF_COMMENT_CHARS: usize = 200;

pub fn handle_show(args: &TaskShowArgs, project: Option<&str>, resolver: &TasksDirectoryResolver, renderer: &OutputRenderer) -> Result<(), String> {
    let mut ctx = TaskCommandContext::for_reading(resolver, project, Some(&args.id))?;
    let loaded = load_task_including_deleted(&mut ctx, &args.id, project)?;
    let mut task = loaded.task;
    if let Some(number) = args.comment {
        let open = crate::types::open_questions(&task.comments);
        let index = number.checked_sub(1).filter(|&index| task.comments.get(index).is_some_and(|comment| !comment.is_deleted()));
        let Some(index) = index else {
            let range = if task.comments.is_empty() { "it has no comments".to_string() } else { format!("comments are #1 to #{}", task.comments.len()) };
            return Err(format!("{} has no comment #{number}: {range}, deleted ones left out", task.id));
        };
        let comment = &task.comments[index];
        match renderer.format {
            OutputFormat::Json => renderer.emit_json(comment),
            OutputFormat::Text => renderer.emit_raw_stdout(comment_text(index, comment, open.contains(&index)).trim()),
        }
        return Ok(());
    }
    if matches!(renderer.format, OutputFormat::Text) {
        renderer.emit_raw_stdout(render_task_text(&task, &resolver.path, args.brief));
        return Ok(());
    }
    if args.brief {
        for comment in &mut task.comments {
            comment.text = brief_line(&comment.text);
            comment.report = None;
            comment.review = None;
        }
    }
    // Deleted comments are left out (DEV-809), so whatever reads this
    // (the inbox mirror) sees a withdrawn question as gone; the others
    // carry their stored `number` (DEV-828).
    let comments: Vec<_> =
        task.comments.iter().enumerate().filter(|(_, comment)| !comment.is_deleted()).map(|(index, comment)| numbered_comment_json(index, comment)).collect();
    let mut value = serde_json::to_value(&task).map_err(|e| format!("Failed to serialize {}: {e}", task.id))?;
    if let Some(object) = value.as_object_mut() {
        object.insert("comments".to_string(), serde_json::Value::Array(comments));
        if args.brief {
            object.remove("history");
        }
    }
    renderer.emit_json(&value);
    Ok(())
}

/// A comment's first line, cut at [`BRIEF_COMMENT_CHARS`] with `…`.
fn brief_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim_end();
    let cut: String = line.chars().take(BRIEF_COMMENT_CHARS).collect();
    let more = cut.len() < line.len() || text.trim_end().lines().nth(1).is_some();
    if more { format!("{cut}…") } else { cut }
}

/// One comment in full: `#N`, date, marker, author and its text, then the
/// report's lines.
fn comment_text(index: usize, comment: &TaskComment, open: bool) -> String {
    let mut lines = comment.text.lines();
    let author = comment.author.as_deref().map(|author| format!("{author}: ")).unwrap_or_default();
    let marker = crate::cli::handlers::comment::comment_marker(comment, open);
    let mut out = format!("  #{}  {}  {marker}{author}{}\n", index + 1, comment.date, lines.next().unwrap_or_default());
    for line in lines {
        out.push_str(&format!("    {line}\n"));
    }
    if let Some(report) = &comment.report {
        for line in crate::cli::handlers::comment::report_lines(report) {
            out.push_str(&format!("    {line}\n"));
        }
    }
    out
}

/// Plain-text view of a task: header, fields, relationships, description,
/// references and comments. Empty sections are left out. `brief` cuts each
/// comment to one line (DEV-884).
pub fn render_task_text(task: &TaskDTO, tasks_root: &std::path::Path, brief: bool) -> String {
    let mut out = format!("{}  {}\n", task.id, task.title);
    if let Some(url) = &task.url {
        out.push_str(&format!("{url}\n"));
    }
    let mut state = vec![format!("Status: {}", task.status), format!("Priority: {}", task.priority), format!("Type: {}", task.task_type)];
    if task.deleted_at.is_some() {
        state.push("Deleted".into());
    } else if task.archived {
        state.push("Archived".into());
    }
    out.push_str(&state.join("   "));
    out.push('\n');

    let mut fields: Vec<(&str, String)> = Vec::new();
    let mut push = |label: &'static str, value: Option<String>| {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            fields.push((label, value));
        }
    };
    push("Assignee", task.assignee.clone());
    push("Reporter", task.reporter.clone());
    push("Due", task.due_date.clone());
    push("Effort", task.effort.clone());
    push("Tags", (!task.tags.is_empty()).then(|| task.tags.join(", ")));
    push("Sprints", (!task.sprints.is_empty()).then(|| task.sprints.iter().map(|id| format!("#{id}")).collect::<Vec<_>>().join(", ")));
    push("Releases", (!task.releases.is_empty()).then(|| task.releases.iter().map(|id| format!("#{id}")).collect::<Vec<_>>().join(", ")));
    push("Created", Some(task.created.clone()));
    push("Modified", Some(task.modified.clone()));
    for (name, value) in &task.custom_fields {
        let text = crate::types::custom_value_items(value).join(", ");
        if !text.trim().is_empty() {
            fields.push(("", format!("{name}: {text}")));
        }
    }
    for (label, value) in fields {
        if label.is_empty() {
            out.push_str(&format!("{value}\n"));
        } else {
            out.push_str(&format!("{label}: {value}\n"));
        }
    }

    let rel = &task.relationships;
    let mut links: Vec<(&str, Vec<String>)> = vec![
        ("Depends on", rel.depends_on.clone()),
        ("Blocks", rel.blocks.clone()),
        ("Parent", rel.parent.iter().cloned().collect()),
        ("Children", rel.children.clone()),
        ("Related", rel.related.clone()),
        ("Fixes", rel.fixes.clone()),
        ("Duplicate of", rel.duplicate_of.iter().cloned().collect()),
    ];
    links.retain(|(_, ids)| !ids.is_empty());
    if !links.is_empty() {
        out.push_str("\nRelationships\n");
        for (label, ids) in links {
            let ids: Vec<_> = ids.iter().map(|id| crate::services::external_tasks::describe(tasks_root, id)).collect();
            out.push_str(&format!("  {label}: {}\n", ids.join(", ")));
        }
    }

    if let Some(description) = task.description.as_deref().filter(|d| !d.trim().is_empty()) {
        out.push_str("\nDescription\n");
        for line in description.lines() {
            out.push_str(&format!("  {line}\n"));
        }
    }

    if !task.acceptance_criteria.is_empty() {
        out.push_str("\nAcceptance criteria\n");
        for item in &task.acceptance_criteria {
            out.push_str(&format!("  - {item}\n"));
        }
    }

    let references: Vec<String> = task
        .references
        .iter()
        .filter_map(|reference| {
            [
                ("link", &reference.link),
                ("file", &reference.file),
                ("code", &reference.code),
                ("attachment", &reference.attachment),
                ("jira", &reference.jira),
                ("github", &reference.github),
            ]
            .into_iter()
            .find_map(|(kind, value)| value.as_ref().map(|value| format!("{kind}: {value}")))
        })
        .collect();
    if !references.is_empty() {
        out.push_str("\nReferences\n");
        for reference in references {
            out.push_str(&format!("  {reference}\n"));
        }
    }

    // Deleted comments are left out (DEV-809); `#N` is the stored position,
    // which `lotar comment --edit/--delete` take.
    let live: Vec<_> = crate::types::live_comments(&task.comments).collect();
    if !live.is_empty() {
        out.push_str(&format!("\nComments ({})\n", live.len()));
        let open = crate::types::open_questions(&task.comments);
        for (index, comment) in live {
            if brief {
                let author = comment.author.as_deref().map(|author| format!("{author}: ")).unwrap_or_default();
                let marker = crate::cli::handlers::comment::comment_marker(comment, open.contains(&index));
                out.push_str(&format!("  #{}  {}  {marker}{author}{}\n", index + 1, comment.date, brief_line(&comment.text)));
            } else {
                out.push_str(&comment_text(index, comment, open.contains(&index)));
            }
        }
        if brief {
            out.push_str(&format!("  One in full: lotar show {} --comment <N>\n", task.id));
        }
    }
    out.trim_end().to_string()
}
