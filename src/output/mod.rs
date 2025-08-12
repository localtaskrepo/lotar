use crate::storage::task::Task;
use crate::types::{Priority, TaskStatus, TaskType};
use clap::ValueEnum;
use serde::Serialize;
use std::io::{self, Write};

mod json;
mod text;

#[derive(Debug, Clone, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

// Custom parser for clap to provide clearer error messages for invalid format values
pub fn parse_output_format(s: &str) -> Result<OutputFormat, String> {
    match s.to_ascii_lowercase().as_str() {
        "text" => Ok(OutputFormat::Text),
        // Backward-compatible aliases; render as plain text
        "table" | "markdown" | "md" => Ok(OutputFormat::Text),
        "json" => Ok(OutputFormat::Json),
        other => Err(format!(
            "invalid format: '{}' . Supported formats: text, json",
            other
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn allows(self, level: LogLevel) -> bool {
        use LogLevel::*;
        fn rank(l: LogLevel) -> u8 {
            match l {
                Off => 0,
                Error => 1,
                Warn => 2,
                Info => 3,
                Debug => 4,
                Trace => 5,
            }
        }
        rank(self) >= rank(level) && !matches!(self, Off)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDisplayInfo {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub task_type: String,
    pub description: Option<String>,
    pub assignee: Option<String>,
    pub project: Option<String>,
    pub due_date: Option<String>,
    pub effort: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub created: String,
    pub modified: String,
    pub custom_fields: crate::types::CustomFields,
}

pub trait Outputable {
    fn to_text(&self) -> String;
    fn to_table_row(&self) -> Vec<String>;
    fn table_headers() -> Vec<&'static str>;
}

// Implement Outputable for Task
impl Outputable for Task {
    fn to_text(&self) -> String {
        let status_emoji = match self.status {
            TaskStatus::Todo => "📋",
            TaskStatus::InProgress => "🚧",
            TaskStatus::Verify => "🔍",
            TaskStatus::Blocked => "🚫",
            TaskStatus::Done => "✅",
        };

        let priority_str = self.priority.to_string();
        let priority_color = priority_str;

        let type_str = self.task_type.to_string();
        let type_styled = type_str;

        let mut output = format!(
            "{} {} [{}] - {} ({})",
            status_emoji,
            self.title.clone(),
            type_styled,
            priority_color,
            self.status
        );

        if let Some(assignee) = &self.assignee {
            output.push_str(&format!(" - 👤 {}", assignee));
        }

        if let Some(due_date) = &self.due_date {
            output.push_str(&format!(" - 📅 {}", due_date));
        }

        if !self.tags.is_empty() {
            output.push_str(&format!(" - 🏷️  {}", self.tags.join(", ")));
        }

        output
    }

    fn to_table_row(&self) -> Vec<String> {
        vec![
            self.title.clone(),
            self.status.to_string(),
            self.priority.to_string(),
            self.task_type.to_string(),
            self.assignee.as_deref().unwrap_or("-").to_owned(),
            self.due_date.as_deref().unwrap_or("-").to_owned(),
            self.effort.as_deref().unwrap_or("-").to_owned(),
            if self.tags.is_empty() {
                String::from("-")
            } else {
                self.tags.join(", ")
            },
        ]
    }

    fn table_headers() -> Vec<&'static str> {
        vec![
            "Title", "Status", "Priority", "Type", "Assignee", "Due Date", "Effort", "Tags",
        ]
    }
}

// Implement Outputable for a simple task summary struct
#[derive(Debug, Serialize)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub priority: Priority,
    pub task_type: TaskType,
}

impl Outputable for TaskSummary {
    fn to_text(&self) -> String {
        format!(
            "[{}] {} - {} ({}, {})",
            self.id, self.title, self.status, self.priority, self.task_type
        )
    }

    fn to_table_row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.title.clone(),
            self.status.to_string(),
            self.priority.to_string(),
            self.task_type.to_string(),
        ]
    }

    fn table_headers() -> Vec<&'static str> {
        vec!["ID", "Title", "Status", "Priority", "Type"]
    }
}

pub struct OutputRenderer {
    pub format: OutputFormat,
    log_level: LogLevel,
    // pretty_json controls pretty JSON printing; we derive it from log level (Debug+)
    pretty_json: bool,
}

impl OutputRenderer {
    pub fn new(format: OutputFormat, log_level: LogLevel) -> Self {
        let pretty_json = matches!(log_level, LogLevel::Debug | LogLevel::Trace);
        Self {
            format,
            log_level,
            pretty_json,
        }
    }

    // Small helpers to reduce duplication
    fn json_status_message(status: &str, message: &str) -> String {
        serde_json::json!({
            "status": status,
            "message": message
        })
        .to_string()
    }

    // removed unused transitional helper to satisfy clippy dead_code

    pub fn render_single<T: Outputable + Serialize>(&self, item: &T) -> String {
        match self.format {
            OutputFormat::Text => self.render_text_single(item),
            OutputFormat::Json => self.render_json_single(item),
        }
    }

    pub fn render_list<T: Outputable + Serialize>(
        &self,
        items: &[T],
        title: Option<&str>,
    ) -> String {
        match self.format {
            OutputFormat::Text => self.render_text_list(items, title),
            OutputFormat::Json => self.render_json_list(items),
        }
    }

    pub fn render_success(&self, message: &str) -> String {
        match self.format {
            OutputFormat::Json => Self::json_status_message("success", message),
            _ => format!("✅ {}", message),
        }
    }

    pub fn render_error(&self, message: &str) -> String {
        match self.format {
            OutputFormat::Json => Self::json_status_message("error", message),
            _ => format!("❌ {}", message),
        }
    }

    pub fn render_warning(&self, message: &str) -> String {
        match self.format {
            OutputFormat::Json => Self::json_status_message("warning", message),
            _ => format!("⚠️  {}", message),
        }
    }

    pub fn render_info(&self, message: &str) -> String {
        match self.format {
            OutputFormat::Json => Self::json_status_message("info", message),
            _ => format!("ℹ️  {}", message),
        }
    }

    // Emitters: user-facing output, not gated by log level.
    // Respect stream hygiene: info/success -> stdout (except JSON mode where info headers are unwanted),
    // warnings/errors -> stderr.
    pub fn emit_success(&self, message: &str) {
        let out = self.render_success(message);
        let _ = writeln!(io::stdout(), "{}", out);
    }

    pub fn emit_error(&self, message: &str) {
        let out = self.render_error(message);
        let _ = writeln!(io::stderr(), "{}", out);
    }

    pub fn emit_warning(&self, message: &str) {
        let out = self.render_warning(message);
        let _ = writeln!(io::stderr(), "{}", out);
    }

    pub fn emit_info(&self, message: &str) {
        // Suppress info banners in JSON format to keep stdout pure JSON
        if matches!(self.format, OutputFormat::Json) {
            return;
        }
        let out = self.render_info(message);
        let _ = writeln!(io::stdout(), "{}", out);
    }

    // Emit info even in JSON mode (used when a command would otherwise emit nothing)
    pub fn emit_notice(&self, message: &str) {
        let out = self.render_info(message);
        let _ = writeln!(io::stdout(), "{}", out);
    }

    pub fn emit_raw_stdout(&self, message: &str) {
        let _ = writeln!(io::stdout(), "{}", message);
    }

    pub fn emit_raw_stderr(&self, message: &str) {
        let _ = writeln!(io::stderr(), "{}", message);
    }

    // Private implementation methods
    fn render_text_single<T: Outputable>(&self, item: &T) -> String {
        text::render_text_single(item)
    }

    fn render_text_list<T: Outputable>(&self, items: &[T], title: Option<&str>) -> String {
        // use pretty_json as a proxy for verbosity in text mode as well for now
        text::render_text_list(items, title, self.pretty_json)
    }

    fn render_json_single<T: Serialize>(&self, item: &T) -> String {
        json::render_json_single(item, self.pretty_json)
    }

    fn render_json_list<T: Serialize>(&self, items: &[T]) -> String {
        json::render_json_list(items, self.pretty_json)
    }

    // Markdown/Table output removed
}

// ProgressIndicator removed as unused; reintroduce if interactive progress becomes necessary.

// Thin logging helpers: gated by log level and routed to stderr to avoid corrupting stdout payloads.
impl OutputRenderer {
    pub fn log_error(&self, message: &str) {
        if self.log_level.allows(LogLevel::Error) {
            let _ = writeln!(io::stderr(), "{}", self.render_error(message));
        }
    }

    pub fn log_warn(&self, message: &str) {
        if self.log_level.allows(LogLevel::Warn) {
            let _ = writeln!(io::stderr(), "{}", self.render_warning(message));
        }
    }

    pub fn log_info(&self, message: &str) {
        if self.log_level.allows(LogLevel::Info) {
            // Always route to stderr to keep stdout pure in all formats
            let _ = writeln!(io::stderr(), "{}", self.render_info(message));
        }
    }

    pub fn log_debug(&self, message: &str) {
        if self.log_level.allows(LogLevel::Debug) {
            let _ = writeln!(io::stderr(), "🐞 {}", message);
        }
    }

    pub fn log_trace(&self, message: &str) {
        if self.log_level.allows(LogLevel::Trace) {
            let _ = writeln!(io::stderr(), "🔎 {}", message);
        }
    }
}

// Styling removed to keep output plain and test-friendly
