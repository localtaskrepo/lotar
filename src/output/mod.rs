use crate::storage::task::Task;
use crate::types::{Priority, TaskStatus, TaskType};
use clap::ValueEnum;
use console::style;
use serde::Serialize;

mod json;
mod markdown;
mod table;
mod text;

#[derive(Debug, Clone, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Table,
    Json,
    Markdown,
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
    pub custom_fields: std::collections::HashMap<String, serde_yaml::Value>,
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
        let priority_color = match self.priority {
            Priority::Critical => style(&priority_str).red().bright(),
            Priority::High => style(&priority_str).red(),
            Priority::Medium => style(&priority_str).yellow(),
            Priority::Low => style(&priority_str).green(),
        };

        let type_str = self.task_type.to_string();
        let type_styled = match self.task_type {
            TaskType::Bug => style(&type_str).red(),
            TaskType::Feature => style(&type_str).blue(),
            TaskType::Epic => style(&type_str).magenta(),
            TaskType::Spike => style(&type_str).cyan(),
            TaskType::Chore => style(&type_str).dim(),
        };

        let mut output = format!(
            "{} {} [{}] - {} ({})",
            status_emoji,
            style(&self.title).bold(),
            type_styled,
            priority_color,
            style(&self.status.to_string()).cyan()
        );

        if let Some(assignee) = &self.assignee {
            output.push_str(&format!(" - 👤 {}", style(assignee).dim()));
        }

        if let Some(due_date) = &self.due_date {
            output.push_str(&format!(" - 📅 {}", style(due_date).dim()));
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
    verbose: bool,
}

impl OutputRenderer {
    pub fn new(format: OutputFormat, verbose: bool) -> Self {
        Self { format, verbose }
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
            OutputFormat::Table => self.render_table_single(item),
            OutputFormat::Json => self.render_json_single(item),
            OutputFormat::Markdown => self.render_markdown_single(item),
        }
    }

    pub fn render_list<T: Outputable + Serialize>(
        &self,
        items: &[T],
        title: Option<&str>,
    ) -> String {
        match self.format {
            OutputFormat::Text => self.render_text_list(items, title),
            OutputFormat::Table => self.render_table_list(items, title),
            OutputFormat::Json => self.render_json_list(items),
            OutputFormat::Markdown => self.render_markdown_list(items, title),
        }
    }

    pub fn render_success(&self, message: &str) -> String {
        match self.format {
            OutputFormat::Json => Self::json_status_message("success", message),
            _ => format!("✅ {}", style(message).green()),
        }
    }

    pub fn render_error(&self, message: &str) -> String {
        match self.format {
            OutputFormat::Json => Self::json_status_message("error", message),
            _ => format!("❌ {}", style(message).red()),
        }
    }

    pub fn render_warning(&self, message: &str) -> String {
        match self.format {
            OutputFormat::Json => Self::json_status_message("warning", message),
            _ => format!("⚠️  {}", style(message).yellow()),
        }
    }

    pub fn render_info(&self, message: &str) -> String {
        match self.format {
            OutputFormat::Json => Self::json_status_message("info", message),
            _ => format!("ℹ️  {}", message),
        }
    }

    // Private implementation methods
    fn render_text_single<T: Outputable>(&self, item: &T) -> String {
        text::render_text_single(item)
    }

    fn render_text_list<T: Outputable>(&self, items: &[T], title: Option<&str>) -> String {
        text::render_text_list(items, title, self.verbose)
    }

    fn render_table_single<T: Outputable>(&self, item: &T) -> String {
        table::render_table_single(item)
    }

    fn render_table_list<T: Outputable>(&self, items: &[T], title: Option<&str>) -> String {
        table::render_table_list(items, title)
    }

    fn render_json_single<T: Serialize>(&self, item: &T) -> String {
        json::render_json_single(item, self.verbose)
    }

    fn render_json_list<T: Serialize>(&self, items: &[T]) -> String {
        json::render_json_list(items, self.verbose)
    }

    fn render_markdown_single<T: Outputable>(&self, item: &T) -> String {
        markdown::render_markdown_single(item)
    }

    fn render_markdown_list<T: Outputable>(&self, items: &[T], title: Option<&str>) -> String {
        markdown::render_markdown_list(items, title)
    }
}

// ProgressIndicator removed as unused; reintroduce if interactive progress becomes necessary.
