use crate::storage::task::Task;
use crate::types::{Priority, TaskStatus, TaskType};
use clap::ValueEnum;
use comfy_table::{Cell, ContentArrangement, Table};
use console::style;
use serde::Serialize;

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
            self.assignee.as_deref().unwrap_or("-").to_string(),
            self.due_date.as_deref().unwrap_or("-").to_string(),
            self.effort.as_deref().unwrap_or("-").to_string(),
            if self.tags.is_empty() {
                "-".to_string()
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

    #[allow(dead_code)]
    pub fn render_single<T: Outputable + Serialize>(&self, item: &T) -> String {
        match self.format {
            OutputFormat::Text => self.render_text_single(item),
            OutputFormat::Table => self.render_table_single(item),
            OutputFormat::Json => self.render_json_single(item),
            OutputFormat::Markdown => self.render_markdown_single(item),
        }
    }

    #[allow(dead_code)]
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
            OutputFormat::Json => serde_json::json!({
                "status": "success",
                "message": message
            })
            .to_string(),
            _ => format!("✅ {}", style(message).green()),
        }
    }

    pub fn render_error(&self, message: &str) -> String {
        match self.format {
            OutputFormat::Json => serde_json::json!({
                "status": "error",
                "message": message
            })
            .to_string(),
            _ => format!("❌ {}", style(message).red()),
        }
    }

    pub fn render_warning(&self, message: &str) -> String {
        match self.format {
            OutputFormat::Json => serde_json::json!({
                "status": "warning",
                "message": message
            })
            .to_string(),
            _ => format!("⚠️  {}", style(message).yellow()),
        }
    }

    // Private implementation methods
    #[allow(dead_code)]
    fn render_text_single<T: Outputable>(&self, item: &T) -> String {
        item.to_text()
    }

    #[allow(dead_code)]
    fn render_text_list<T: Outputable>(&self, items: &[T], title: Option<&str>) -> String {
        let mut output = String::new();

        if let Some(title) = title {
            output.push_str(&format!("{}\n\n", style(title).bold().underlined()));
        }

        if items.is_empty() {
            output.push_str(&style("No items found.").dim().to_string());
        } else {
            for (index, item) in items.iter().enumerate() {
                if self.verbose {
                    output.push_str(&format!("{}. {}\n", index + 1, item.to_text()));
                } else {
                    output.push_str(&format!("{}\n", item.to_text()));
                }
            }
        }

        output
    }

    #[allow(dead_code)]
    fn render_table_single<T: Outputable>(&self, item: &T) -> String {
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);

        let headers = T::table_headers();
        let values = item.to_table_row();

        for (header, value) in headers.iter().zip(values.iter()) {
            table.add_row(vec![Cell::new(header), Cell::new(value)]);
        }

        table.to_string()
    }

    #[allow(dead_code)]
    fn render_table_list<T: Outputable>(&self, items: &[T], title: Option<&str>) -> String {
        let mut output = String::new();

        if let Some(title) = title {
            output.push_str(&format!("{}\n\n", style(title).bold().underlined()));
        }

        if items.is_empty() {
            return format!("{}No items found.", output);
        }

        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);

        // Add headers
        let headers = T::table_headers();
        table.set_header(headers);

        // Add rows
        for item in items {
            table.add_row(item.to_table_row());
        }

        output.push_str(&table.to_string());
        output
    }

    #[allow(dead_code)]
    fn render_json_single<T: Serialize>(&self, item: &T) -> String {
        if self.verbose {
            serde_json::to_string_pretty(item).unwrap_or_else(|_| "{}".to_string())
        } else {
            serde_json::to_string(item).unwrap_or_else(|_| "{}".to_string())
        }
    }

    #[allow(dead_code)]
    fn render_json_list<T: Serialize>(&self, items: &[T]) -> String {
        if self.verbose {
            serde_json::to_string_pretty(items).unwrap_or_else(|_| "[]".to_string())
        } else {
            serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
        }
    }

    #[allow(dead_code)]
    fn render_markdown_single<T: Outputable>(&self, item: &T) -> String {
        let headers = T::table_headers();
        let values = item.to_table_row();

        let mut output = String::new();
        for (header, value) in headers.iter().zip(values.iter()) {
            output.push_str(&format!("**{}:** {}\n", header, value));
        }
        output
    }

    #[allow(dead_code)]
    fn render_markdown_list<T: Outputable>(&self, items: &[T], title: Option<&str>) -> String {
        let mut output = String::new();

        if let Some(title) = title {
            output.push_str(&format!("# {}\n\n", title));
        }

        if items.is_empty() {
            output.push_str("No items found.\n");
            return output;
        }

        // Markdown table
        let headers = T::table_headers();
        output.push_str("| ");
        for header in &headers {
            output.push_str(&format!("{} | ", header));
        }
        output.push('\n');

        output.push_str("| ");
        for _ in &headers {
            output.push_str("--- | ");
        }
        output.push('\n');

        for item in items {
            let values = item.to_table_row();
            output.push_str("| ");
            for value in values {
                output.push_str(&format!("{} | ", value));
            }
            output.push('\n');
        }

        output
    }
}

// Progress indicators for different formats
#[allow(dead_code)]
pub struct ProgressIndicator {
    format: OutputFormat,
}

impl ProgressIndicator {
    #[allow(dead_code)]
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    #[allow(dead_code)]
    pub fn start(&self, message: &str) {
        match self.format {
            OutputFormat::Json => {
                // JSON mode is typically for scripts, so no progress indicators
            }
            _ => {
                print!("🔄 {}...", message);
                use std::io::Write;
                std::io::stdout().flush().unwrap();
            }
        }
    }

    #[allow(dead_code)]
    pub fn finish(&self, success: bool) {
        match self.format {
            OutputFormat::Json => {
                // No output for JSON mode
            }
            _ => {
                if success {
                    println!(" ✅");
                } else {
                    println!(" ❌");
                }
            }
        }
    }
}
