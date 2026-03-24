use crate::api_types::TaskDTO;
use crate::services::automation_service::AutomationJobContext;
use std::collections::HashMap;

/// Context for expanding `${{...}}` template variables in automation config strings.
#[derive(Default)]
pub struct TemplateContext {
    vars: HashMap<String, String>,
}

impl TemplateContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build context from a task (ticket fields).
    pub fn from_task(task: &TaskDTO) -> Self {
        let mut ctx = Self::new();
        ctx.populate_task_fields("ticket", task);
        ctx
    }

    /// Add previous field values (for updated/assigned events).
    pub fn with_previous(mut self, previous: Option<&TaskDTO>) -> Self {
        let Some(prev) = previous else {
            return self;
        };
        self.populate_task_fields("previous", prev);
        self
    }

    /// Populate template variables for a task under the given prefix.
    fn populate_task_fields(&mut self, prefix: &str, task: &TaskDTO) {
        self.set(&format!("{prefix}.id"), &task.id);
        self.set(&format!("{prefix}.title"), &task.title);
        self.set(&format!("{prefix}.status"), &task.status.to_string());
        self.set(&format!("{prefix}.priority"), &task.priority.to_string());
        self.set(&format!("{prefix}.type"), &task.task_type.to_string());
        if let Some(ref v) = task.assignee {
            self.set(&format!("{prefix}.assignee"), v);
        }
        if let Some(ref v) = task.reporter {
            self.set(&format!("{prefix}.reporter"), v);
        }
        if let Some(ref v) = task.description {
            self.set(&format!("{prefix}.description"), v);
        }
        if let Some(ref v) = task.due_date {
            self.set(&format!("{prefix}.due_date"), v);
        }
        if let Some(ref v) = task.effort {
            self.set(&format!("{prefix}.effort"), v);
        }
        if !task.tags.is_empty() {
            self.set(&format!("{prefix}.tags"), &task.tags.join(","));
        }
        for (key, value) in &task.custom_fields {
            let stringified = custom_field_to_string(value);
            self.set(&format!("{prefix}.field:{key}"), &stringified);
        }
    }

    /// Add agent/job context variables.
    pub fn with_job(mut self, job: Option<&AutomationJobContext>) -> Self {
        let Some(job) = job else {
            return self;
        };
        self.set("agent.job_id", &job.job_id);
        self.set("agent.runner", &job.runner);
        if let Some(ref v) = job.agent {
            self.set("agent.profile", v);
        }
        if let Some(ref v) = job.worktree_path {
            self.set("agent.worktree_path", v);
        }
        if let Some(ref v) = job.worktree_branch {
            self.set("agent.worktree_branch", v);
        }
        self
    }

    /// Add the comment text that triggered a `commented` event.
    pub fn with_comment(mut self, text: Option<&str>) -> Self {
        let Some(text) = text else {
            return self;
        };
        self.set("comment.text", text);
        self
    }

    fn set(&mut self, key: &str, value: &str) {
        self.vars.insert(key.to_string(), value.to_string());
    }

    /// Expand all `${{key}}` placeholders in `input`. Unknown keys expand to empty string.
    pub fn expand(&self, input: &str) -> String {
        self.expand_inner(input, false)
    }

    /// Like `expand` but wraps substituted values in shell-safe single quotes.
    /// Use this for strings that will be passed to `sh -c` to prevent injection.
    pub fn expand_shell_safe(&self, input: &str) -> String {
        self.expand_inner(input, true)
    }

    fn expand_inner(&self, input: &str, shell_escape: bool) -> String {
        let mut result = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find("${{") {
            result.push_str(&rest[..start]);
            let after_open = &rest[start + 3..];
            if let Some(end) = after_open.find("}}") {
                let key = after_open[..end].trim();
                if let Some(value) = self.vars.get(key) {
                    if shell_escape {
                        shell_quote_into(&mut result, value);
                    } else {
                        result.push_str(value);
                    }
                }
                // Unknown key → empty string (no output)
                rest = &after_open[end + 2..];
            } else {
                // No closing `}}` — output the literal `${{` and move on
                result.push_str("${{");
                rest = after_open;
            }
        }
        result.push_str(rest);
        result
    }
}

/// Write `value` into `out` as a POSIX single-quoted string.
/// Single quotes inside the value are escaped as `'\''`.
fn shell_quote_into(out: &mut String, value: &str) {
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
}

#[cfg(not(feature = "schema"))]
fn custom_field_to_string(value: &crate::types::CustomFieldValue) -> String {
    match value {
        serde_yaml::Value::String(v) => v.clone(),
        serde_yaml::Value::Bool(v) => v.to_string(),
        serde_yaml::Value::Number(v) => v.to_string(),
        serde_yaml::Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

#[cfg(feature = "schema")]
fn custom_field_to_string(value: &crate::types::CustomFieldValue) -> String {
    match value {
        serde_json::Value::String(v) => v.clone(),
        serde_json::Value::Bool(v) => v.to_string(),
        serde_json::Value::Number(v) => v.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_basic_variables() {
        let mut ctx = TemplateContext::new();
        ctx.set("ticket.id", "PROJ-1");
        ctx.set("ticket.title", "Fix bug");
        assert_eq!(
            ctx.expand("Working on ${{ticket.id}}: ${{ticket.title}}"),
            "Working on PROJ-1: Fix bug"
        );
    }

    #[test]
    fn expand_unknown_variable_to_empty() {
        let ctx = TemplateContext::new();
        assert_eq!(ctx.expand("before ${{unknown.var}} after"), "before  after");
    }

    #[test]
    fn expand_no_placeholders() {
        let ctx = TemplateContext::new();
        assert_eq!(ctx.expand("plain text"), "plain text");
    }

    #[test]
    fn expand_unclosed_placeholder() {
        let ctx = TemplateContext::new();
        assert_eq!(ctx.expand("text ${{ no close"), "text ${{ no close");
    }

    #[test]
    fn expand_whitespace_in_key() {
        let mut ctx = TemplateContext::new();
        ctx.set("ticket.id", "PROJ-1");
        assert_eq!(ctx.expand("${{ ticket.id }}"), "PROJ-1");
    }

    #[test]
    fn expand_multiple_same_variable() {
        let mut ctx = TemplateContext::new();
        ctx.set("ticket.id", "X-1");
        assert_eq!(
            ctx.expand("${{ticket.id}} and ${{ticket.id}}"),
            "X-1 and X-1"
        );
    }

    #[test]
    fn expand_shell_safe_quotes_values() {
        let mut ctx = TemplateContext::new();
        ctx.set("ticket.title", "Fix the bug");
        assert_eq!(
            ctx.expand_shell_safe("echo ${{ticket.title}}"),
            "echo 'Fix the bug'"
        );
    }

    #[test]
    fn expand_shell_safe_escapes_single_quotes() {
        let mut ctx = TemplateContext::new();
        ctx.set("ticket.title", "it's broken");
        assert_eq!(
            ctx.expand_shell_safe("echo ${{ticket.title}}"),
            "echo 'it'\\''s broken'"
        );
    }

    #[test]
    fn expand_shell_safe_prevents_injection() {
        let mut ctx = TemplateContext::new();
        ctx.set("ticket.title", "'; echo injected #");
        let result = ctx.expand_shell_safe("echo ${{ticket.title}}");
        assert_eq!(result, "echo ''\\''; echo injected #'");
        // The value is safely wrapped — the shell interprets it as a literal string
    }

    #[test]
    fn expand_shell_safe_unknown_stays_empty() {
        let ctx = TemplateContext::new();
        assert_eq!(ctx.expand_shell_safe("echo ${{unknown}}"), "echo ");
    }
}
