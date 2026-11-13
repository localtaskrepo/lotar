use crate::cli::args::task::CommentAskArgs;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::task_questions::{AnswerInput, OPTION_SET_FIELDS};
use crate::services::task_service::TaskService;
use crate::types::{CommentQuestion, OptionSet, QuestionOption, TaskComment};
use crate::workspace::TasksDirectoryResolver;
use serde_json::{Map, Value};

const COMMENT_PREVIEW_EXPLANATION: &str = "comments append to the task log with the current UTC timestamp; dry-run skips persistence";
const COMMENT_LIST_EXPLANATION: &str = "Displays stored comments in chronological order; use --dry-run with text to preview an addition.";

/// Handler for adding or listing task comments
pub struct CommentHandler;

pub struct CommentArgs {
    pub task_id: String,
    pub text: Option<String>,
    pub explicit_project: Option<String>,
    pub dry_run: bool,
    pub explain: bool,
    pub ask: CommentAskArgs,
}

impl CommentArgs {
    pub fn new(task_id: String, text: Option<String>, explicit_project: Option<String>, dry_run: bool, explain: bool) -> Self {
        Self { task_id, text, explicit_project, dry_run, explain, ask: CommentAskArgs::default() }
    }

    pub fn with_ask(mut self, ask: CommentAskArgs) -> Self {
        self.ask = ask;
        self
    }
}

/// What `lotar comment` writes: plain text, a question or an answer.
enum CommentKind {
    Plain(String),
    Question { prompt: String, question: CommentQuestion },
    Answer(AnswerInput),
}

impl CommentKind {
    /// `None` lists the comments.
    fn from_args(text: Option<String>, ask: CommentAskArgs) -> Result<Option<Self>, String> {
        if let Some(prompt) = ask.question {
            if text.is_some() {
                return Err("--question takes the question; leave out the comment text".to_string());
            }
            return Ok(Some(Self::Question { prompt, question: question_from_args(&ask.options, &ask.sets, ask.recommend, ask.multiple, !ask.no_other)? }));
        }
        if let Some(keys) = ask.answer {
            let choice = keys.split(',').map(str::trim).filter(|key| !key.is_empty()).map(str::to_string).collect();
            return Ok(Some(Self::Answer(AnswerInput { to: ask.to, choice, note: text })));
        }
        Ok(text.map(Self::Plain))
    }
}

/// The question that `--option` and `--set` describe.
fn question_from_args(options: &[String], sets: &[String], recommended: Option<String>, multiple: bool, other: bool) -> Result<CommentQuestion, String> {
    let mut parsed: Vec<QuestionOption> = Vec::new();
    for raw in options {
        let (key, label) = raw.split_once(':').ok_or_else(|| format!("--option '{raw}' needs the form \"<key>: <label>\""))?;
        parsed.push(QuestionOption { key: key.trim().to_string(), label: label.trim().to_string(), set: None, extra: Default::default() });
    }
    for raw in sets {
        let invalid = || format!("--set '{raw}' needs the form \"<key>:<field>=<value>\"");
        let (key, assignment) = raw.split_once(':').ok_or_else(invalid)?;
        let (field, value) = assignment.split_once('=').ok_or_else(invalid)?;
        let (key, field, value) = (key.trim(), field.trim(), value.trim().to_string());
        let option = parsed.iter_mut().find(|option| option.key == key).ok_or_else(|| format!("--set '{raw}': no --option has the key '{key}'"))?;
        let set = option.set.get_or_insert_with(OptionSet::default);
        match field {
            "status" => set.status = Some(value),
            "assignee" => set.assignee = Some(value),
            "reporter" => set.reporter = Some(value),
            "priority" => set.priority = Some(value),
            "type" => set.task_type = Some(value),
            "due_date" | "due" => set.due_date = Some(value),
            "effort" => set.effort = Some(value),
            "tags" | "tag" => set.tags = Some(value.split(',').map(str::trim).filter(|tag| !tag.is_empty()).map(str::to_string).collect()),
            other => return Err(format!("--set '{raw}': an option cannot set '{other}' (fields: {})", OPTION_SET_FIELDS.join(", "))),
        }
    }
    Ok(CommentQuestion { options: parsed, recommended, multiple, other, extra: Default::default() })
}

impl CommandHandler for CommentHandler {
    type Args = CommentArgs;
    type Result = Result<(), String>;

    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver, renderer: &OutputRenderer) -> Self::Result {
        let CommentArgs { task_id, text, explicit_project, dry_run, explain, ask } = args;

        let project_hint = explicit_project.as_deref().or(project);
        renderer.log_info(format_args!("comment: begin task_id={} explicit_project={:?}", task_id, project_hint));

        let comment = CommentKind::from_args(text.filter(|candidate| !candidate.trim().is_empty()), ask)?;

        let mut ctx = if comment.is_some() {
            TaskCommandContext::new(resolver, project_hint, Some(task_id.as_str()))?
        } else {
            TaskCommandContext::new_read_only(resolver, project_hint, Some(task_id.as_str()))?
        };

        let LoadedTask { full_id, project_prefix, task } = load_task(&mut ctx, &task_id, project_hint)?;

        renderer.log_debug(format_args!(
            "comment: resolved task={} project={}",
            full_id,
            if project_prefix.trim().is_empty() { "-" } else { project_prefix.as_str() }
        ));

        let resolved_project = ctx
            .resolved_project_name()
            .map(|value| value.to_string())
            .or_else(|| if project_prefix.trim().is_empty() { None } else { Some(project_prefix.clone()) });

        match comment {
            Some(CommentKind::Plain(body)) => {
                let add_inputs = CommentAddInputs { project: resolved_project.clone(), task, text: body, dry_run, explain };
                handle_add_comment(&mut ctx, renderer, &full_id, add_inputs)
            }
            Some(CommentKind::Question { .. } | CommentKind::Answer(_)) if dry_run => Err("--dry-run does not apply to --question or --answer".to_string()),
            Some(CommentKind::Question { prompt, question }) => {
                let dto = TaskService::ask_question(&mut ctx.storage, &full_id, &prompt, question).map_err(|e| e.to_string())?;
                render_last_comment(renderer, &full_id, resolved_project.as_deref(), &dto, |comment| comment.question.is_some())
            }
            Some(CommentKind::Answer(answer)) => {
                let dto = TaskService::answer_question(&mut ctx.storage, &full_id, &answer).map_err(|e| e.to_string())?;
                render_last_comment(renderer, &full_id, resolved_project.as_deref(), &dto, |comment| comment.answer.is_some())
            }
            None => {
                render_comment_list(renderer, &full_id, resolved_project.as_deref(), &task, explain);
                Ok(())
            }
        }
    }
}

struct CommentAddInputs {
    project: Option<String>,
    task: crate::storage::task::Task,
    text: String,
    dry_run: bool,
    explain: bool,
}

#[expect(clippy::needless_pass_by_value, reason = "the caller hands this value over and does not use it afterwards")]
fn handle_add_comment(ctx: &mut TaskCommandContext, renderer: &OutputRenderer, full_id: &str, inputs: CommentAddInputs) -> Result<(), String> {
    renderer.log_debug("comment: preparing new comment entry");
    if inputs.dry_run {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let prefix = crate::storage::TaskId::parse(full_id).ok().map(|parsed| parsed.project);
        let author = crate::utils::identity::resolve_current_user_for_project(Some(ctx.storage.root_path.as_path()), prefix.as_deref());
        let text = crate::services::task_service::strip_author_prefix(&inputs.text, author.as_deref()).map_err(|e| e.to_string())?.to_string();
        let entry = TaskComment::new(timestamp, author, text);
        render_comment_preview(renderer, full_id, inputs.project.as_deref(), &entry, inputs.task.comments.len() + 1, inputs.explain);
        return Ok(());
    }

    let dto = crate::services::task_service::TaskService::add_comment(&mut ctx.storage, full_id, &inputs.text).map_err(|e| e.to_string())?;
    renderer.log_info("comment: comment persisted");

    let entry = dto.comments.last().ok_or("comment was not stored")?;
    render_comment_success(renderer, full_id, inputs.project.as_deref(), entry, dto.comments.len());
    Ok(())
}

/// Report the newest comment of the kind just added: rules the answer's
/// field changes fire may have commented after it.
fn render_last_comment(
    renderer: &OutputRenderer,
    task_id: &str,
    project: Option<&str>,
    dto: &crate::api_types::TaskDTO,
    kind: impl Fn(&TaskComment) -> bool,
) -> Result<(), String> {
    let entry = dto.comments.iter().rev().find(|comment| kind(comment)).ok_or("comment was not stored")?;
    render_comment_success(renderer, task_id, project, entry, dto.comments.len());
    Ok(())
}

fn render_comment_preview(renderer: &OutputRenderer, task_id: &str, project: Option<&str>, comment: &TaskComment, total_after: usize, explain: bool) {
    match renderer.format {
        OutputFormat::Json => {
            let mut payload = build_comment_payload("preview", "task.comment", task_id, total_after);
            payload.insert("added_comment".to_string(), comment_to_json(comment));
            insert_project(&mut payload, project);
            insert_explain(&mut payload, explain.then_some(COMMENT_PREVIEW_EXPLANATION));
            renderer.emit_json(&Value::Object(payload));
        }
        _ => {
            renderer.emit_info(format_args!("DRY RUN: Would add comment to {} ({} total)", task_id, total_after));
            if explain {
                renderer.emit_info(COMMENT_PREVIEW_EXPLANATION);
            }
        }
    }
}

fn render_comment_success(renderer: &OutputRenderer, task_id: &str, project: Option<&str>, comment: &TaskComment, total: usize) {
    match renderer.format {
        OutputFormat::Json => {
            let mut payload = build_comment_payload("success", "task.comment", task_id, total);
            payload.insert("added_comment".to_string(), comment_to_json(comment));
            insert_project(&mut payload, project);
            renderer.emit_json(&Value::Object(payload));
        }
        _ => {
            renderer.emit_success(format_args!("Comment added to {} ({} total)", task_id, total));
        }
    }
}

fn render_comment_list(renderer: &OutputRenderer, task_id: &str, project: Option<&str>, task: &crate::storage::task::Task, explain: bool) {
    match renderer.format {
        OutputFormat::Json => {
            let items: Vec<_> = task.comments.iter().map(comment_to_json).collect();
            let mut payload = build_comment_payload("ok", "task.comment.list", task_id, items.len());
            payload.insert("items".to_string(), Value::Array(items));
            insert_project(&mut payload, project);
            insert_explain(&mut payload, explain.then_some(COMMENT_LIST_EXPLANATION));
            renderer.emit_json(&Value::Object(payload));
        }
        _ => {
            if task.comments.is_empty() {
                renderer.emit_success(format_args!("No comments for {}.", task_id));
            } else {
                let open = crate::types::open_questions(&task.comments);
                for (index, c) in task.comments.iter().enumerate() {
                    let author = c.author.as_deref().map(|author| format!("{author}: ")).unwrap_or_default();
                    let marker = if open.contains(&index) { "[open question] " } else { "" };
                    renderer.emit_raw_stdout(format_args!("{}  {marker}{author}{}", c.date, c.text));
                }
            }
            if explain {
                renderer.emit_info(COMMENT_LIST_EXPLANATION);
            }
        }
    }
}

fn build_comment_payload(status: &str, action: &str, task_id: &str, count: usize) -> Map<String, Value> {
    let mut payload = Map::with_capacity(4);
    payload.insert("status".to_string(), Value::String(status.to_string()));
    payload.insert("action".to_string(), Value::String(action.to_string()));
    payload.insert("task_id".to_string(), Value::String(task_id.to_string()));
    payload.insert("comments".to_string(), Value::from(count));
    payload
}

fn comment_to_json(comment: &TaskComment) -> Value {
    serde_json::to_value(comment).unwrap_or_else(|_| serde_json::json!({ "date": comment.date, "text": comment.text }))
}

fn insert_project(payload: &mut Map<String, Value>, project: Option<&str>) {
    let trimmed = project.and_then(|candidate| {
        let value = candidate.trim();
        if value.is_empty() { None } else { Some(value) }
    });
    insert_optional_string(payload, "project", trimmed);
}

fn insert_explain(payload: &mut Map<String, Value>, explain: Option<&str>) {
    insert_optional_string(payload, "explain", explain);
}

fn insert_optional_string(payload: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(content) = value {
        payload.insert(key.to_string(), Value::String(content.to_string()));
    }
}

/// The comment text from `-F <file>` (`-F -` reads stdin), `-m`, the
/// positional text, or piped stdin, in that order. An unreadable file is an
/// error with its path (DEV-282: it used to fall back to listing comments
/// with exit 0). Piped stdin is only read when data shows up promptly, so
/// listing comments never hangs on a pipe that stays open with nothing in
/// it (agents, CI, `ssh -T`).
pub fn resolve_comment_text(file: Option<String>, message: Option<String>, text: Option<String>) -> Result<String, String> {
    use std::io::Read;
    let trim = |s: String| s.trim_end_matches(['\n', '\r']).to_string();
    if let Some(path) = file {
        if path == "-" {
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer).map_err(|e| format!("Failed to read the comment from stdin: {e}"))?;
            return Ok(trim(buffer));
        }
        return std::fs::read_to_string(&path).map(trim).map_err(|e| format!("Failed to read --file '{path}': {e}"));
    }
    if let Some(message) = message {
        return Ok(message);
    }
    if let Some(text) = text {
        return Ok(text);
    }
    Ok(read_piped_stdin().map(trim).unwrap_or_default())
}

/// Stdin when it is a file or a pipe that delivers data within a moment;
/// `None` for a terminal or a silent pipe.
fn read_piped_stdin() -> Option<String> {
    use std::io::{IsTerminal, Read};
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fd = stdin.as_raw_fd();
        let ready = |timeout_ms: i32| {
            let mut poll = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            // SAFETY: poll on our own stdin descriptor.
            unsafe { libc::poll(&mut poll, 1, timeout_ms) > 0 }
        };
        if !ready(200) {
            return None;
        }
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        let mut handle = stdin.lock();
        loop {
            match handle.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
            }
            // Stop when the writer goes quiet without closing the pipe.
            if !ready(500) {
                break;
            }
        }
        Some(String::from_utf8_lossy(&out).into_owned())
    }
    #[cfg(not(unix))]
    {
        let mut buffer = String::new();
        stdin.lock().read_to_string(&mut buffer).ok()?;
        Some(buffer)
    }
}
