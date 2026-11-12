use crate::api_types::TaskDTO;
use crate::services::automation_service::AutomationJobContext;
use std::collections::HashMap;

/// Context for expanding `${{...}}` template variables in automation config strings.
///
/// Scalars are looked up by key (`ticket.title`). Lists support a selector:
/// `ticket.tags[*]` joins all items with ", ", `ticket.comments[-1].text`
/// picks one item (negative indexes count from the end) and an optional
/// `.field` reads a field of each item (`ticket.references[*].link`).
#[derive(Default)]
pub struct TemplateContext {
    vars: HashMap<String, String>,
    lists: HashMap<String, Vec<ListItem>>,
}

/// One list item: its plain value plus named fields (references, comments).
#[derive(Default, Clone)]
struct ListItem {
    value: String,
    fields: HashMap<&'static str, String>,
}

impl ListItem {
    fn plain(value: impl Into<String>) -> Self {
        Self { value: value.into(), fields: HashMap::new() }
    }
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

        let plain = |values: &[String]| values.iter().cloned().map(ListItem::plain).collect();
        self.lists.insert(format!("{prefix}.tags"), plain(&task.tags));
        self.lists.insert(format!("{prefix}.acceptance_criteria"), plain(&task.acceptance_criteria));
        self.lists.insert(format!("{prefix}.sprints"), task.sprints.iter().map(|id| ListItem::plain(id.to_string())).collect());
        let relationships = &task.relationships;
        for (name, values) in [
            ("depends_on", &relationships.depends_on),
            ("blocks", &relationships.blocks),
            ("related", &relationships.related),
            ("children", &relationships.children),
            ("fixes", &relationships.fixes),
        ] {
            self.lists.insert(format!("{prefix}.relationships.{name}"), plain(values));
        }
        if let Some(ref parent) = relationships.parent {
            self.set(&format!("{prefix}.relationships.parent"), parent);
        }
        if let Some(ref duplicate) = relationships.duplicate_of {
            self.set(&format!("{prefix}.relationships.duplicate_of"), duplicate);
        }
        self.lists.insert(
            format!("{prefix}.comments"),
            task.comments
                .iter()
                .map(|comment| {
                    let mut fields = HashMap::from([("text", comment.text.clone()), ("date", comment.date.clone())]);
                    if let Some(ref author) = comment.author {
                        fields.insert("author", author.clone());
                    }
                    ListItem { value: comment.text.clone(), fields }
                })
                .collect(),
        );
        self.lists.insert(
            format!("{prefix}.references"),
            task.references
                .iter()
                .map(|reference| {
                    let fields: HashMap<&'static str, String> = [
                        ("code", &reference.code),
                        ("link", &reference.link),
                        ("file", &reference.file),
                        ("attachment", &reference.attachment),
                        ("jira", &reference.jira),
                        ("github", &reference.github),
                    ]
                    .into_iter()
                    .filter_map(|(name, value)| value.clone().map(|value| (name, value)))
                    .collect();
                    let value = ["code", "link", "file", "attachment", "jira", "github"].iter().find_map(|name| fields.get(name).cloned()).unwrap_or_default();
                    ListItem { value, fields }
                })
                .collect(),
        );
    }

    /// The value for `key`: a scalar, or a list selection such as
    /// `ticket.references[*].link` or `ticket.comments[-1].text`.
    fn lookup(&self, key: &str) -> Option<String> {
        if let Some(value) = self.vars.get(key) {
            return Some(value.clone());
        }
        let open = key.find('[')?;
        let close = open + key[open..].find(']')?;
        let list = self.lists.get(&key[..open])?;
        let selector = key[open + 1..close].trim();
        let rest = &key[close + 1..];
        let field = match rest.strip_prefix('.') {
            Some(field) => Some(field),
            None if rest.is_empty() => None,
            None => return None,
        };
        let read = |item: &ListItem| match field {
            None => Some(item.value.clone()),
            Some(name) => item.fields.get(name).cloned(),
        };
        if selector == "*" {
            let values: Vec<String> = list.iter().filter_map(read).collect();
            return Some(values.join(", "));
        }
        let index: i64 = selector.parse().ok()?;
        let position = if index < 0 { list.len().checked_sub(index.unsigned_abs() as usize)? } else { index as usize };
        list.get(position).and_then(read)
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
        if let Some(ref usage) = job.usage {
            self.set("agent.usage", &usage.describe());
            if let Some(cost) = usage.cost_usd {
                self.set("agent.cost_usd", &format!("{cost:.4}"));
            }
            if let Some(tokens) = usage.input_tokens {
                self.set("agent.input_tokens", &tokens.to_string());
            }
            if let Some(tokens) = usage.output_tokens {
                self.set("agent.output_tokens", &tokens.to_string());
            }
            if let Some(turns) = usage.turns {
                self.set("agent.turns", &turns.to_string());
            }
        }
        self
    }

    /// Add the comment that triggered a `commented` event.
    pub fn with_comment(mut self, comment: Option<&crate::types::TaskComment>) -> Self {
        let Some(comment) = comment else {
            return self;
        };
        self.set("comment.text", &comment.text);
        if let Some(ref author) = comment.author {
            self.set("comment.author", author);
        }
        self
    }

    /// Add one variable (for example `lotar.bin` in agent instructions).
    pub fn with_var(mut self, key: &str, value: &str) -> Self {
        self.set(key, value);
        self
    }

    fn set(&mut self, key: &str, value: &str) {
        self.vars.insert(key.to_string(), value.to_string());
    }

    /// Expand all `${{key}}` placeholders in `input`. Unknown keys expand to empty string.
    pub fn expand(&self, input: &str) -> String {
        self.expand_inner(input)
    }

    /// Expand a short-form `run` script without putting values into the
    /// script text (DEV-258). Each placeholder becomes a reference to an
    /// environment variable (`LOTAR_TEMPLATE_1`, ...) returned alongside the
    /// script, so the shell only ever sees values as data, never as code.
    ///
    /// The reference depends on the quoting at the placeholder:
    /// unquoted `"${V}"` (one word), inside double quotes `${V}`, and inside
    /// single quotes `${V}` left literal for a nested shell (`sh -c '...'`)
    /// to expand. Quoting values instead could not work: a value spliced
    /// into a quoted or nested script is parsed again as code.
    pub fn expand_for_shell(&self, input: &str) -> ShellExpansion {
        #[derive(Clone, Copy, PartialEq)]
        enum Quote {
            None,
            Single,
            Double,
        }
        let mut script = String::with_capacity(input.len());
        let mut env: Vec<(String, String)> = Vec::new();
        let mut keys: Vec<String> = Vec::new();
        let mut quote = Quote::None;
        let mut rest = input;
        while let Some(ch) = rest.chars().next() {
            if rest.starts_with("${{")
                && let Some(end) = rest[3..].find("}}")
            {
                let key = rest[3..3 + end].trim().to_string();
                rest = &rest[3 + end + 2..];
                let Some(value) = self.lookup(&key) else {
                    continue; // Unknown key → empty string
                };
                let index = match keys.iter().position(|k| *k == key) {
                    Some(index) => index,
                    None => {
                        keys.push(key);
                        env.push((format!("LOTAR_TEMPLATE_{}", keys.len()), value));
                        keys.len() - 1
                    }
                };
                let name = &env[index].0;
                match quote {
                    Quote::None => {
                        script.push_str("\"${");
                        script.push_str(name);
                        script.push_str("}\"");
                    }
                    Quote::Single | Quote::Double => {
                        script.push_str("${");
                        script.push_str(name);
                        script.push('}');
                    }
                }
                continue;
            }
            script.push(ch);
            rest = &rest[ch.len_utf8()..];
            match (quote, ch) {
                (Quote::None, '\'') => quote = Quote::Single,
                (Quote::None, '"') => quote = Quote::Double,
                (Quote::Single, '\'') | (Quote::Double, '"') => quote = Quote::None,
                (Quote::None | Quote::Double, '\\') => {
                    // A backslash escapes the next character.
                    if let Some(next) = rest.chars().next() {
                        script.push(next);
                        rest = &rest[next.len_utf8()..];
                    }
                }
                _ => {}
            }
        }
        ShellExpansion { script, env }
    }

    /// Preview expansion (DEV-118): like `expand`, but placeholders that only
    /// exist during a real event (`previous.*`, `comment.*`, `agent.*`) and
    /// are not in this context stay literal and are reported in `unresolved`
    /// instead of silently expanding to an empty string.
    pub fn expand_preview(&self, input: &str, unresolved: &mut Vec<String>) -> String {
        let mut result = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find("${{") {
            result.push_str(&rest[..start]);
            let after_open = &rest[start + 3..];
            let Some(end) = after_open.find("}}") else {
                result.push_str("${{");
                rest = after_open;
                continue;
            };
            let key = after_open[..end].trim();
            if let Some(value) = self.lookup(key) {
                result.push_str(&value);
            } else if is_event_only_key(key) {
                result.push_str(&rest[start..start + 3 + end + 2]);
                if !unresolved.iter().any(|k| k == key) {
                    unresolved.push(key.to_string());
                }
            }
            rest = &after_open[end + 2..];
        }
        result.push_str(rest);
        result
    }

    fn expand_inner(&self, input: &str) -> String {
        let mut result = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find("${{") {
            result.push_str(&rest[..start]);
            let after_open = &rest[start + 3..];
            if let Some(end) = after_open.find("}}") {
                let key = after_open[..end].trim();
                if let Some(value) = self.lookup(key) {
                    result.push_str(&value);
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

/// A short-form `run` script plus the environment variables its template
/// placeholders refer to (see [`TemplateContext::expand_for_shell`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellExpansion {
    pub script: String,
    pub env: Vec<(String, String)>,
}

#[cfg(not(feature = "schema"))]
fn custom_field_to_string(value: &crate::types::CustomFieldValue) -> String {
    match value {
        serde_yaml_ng::Value::String(v) => v.clone(),
        serde_yaml_ng::Value::Bool(v) => v.to_string(),
        serde_yaml_ng::Value::Number(v) => v.to_string(),
        serde_yaml_ng::Value::Null => String::new(),
        other => serde_yaml_ng::to_string(other).unwrap_or_default().trim().to_string(),
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

/// Template keys provided by the triggering event rather than the task.
fn is_event_only_key(key: &str) -> bool {
    ["previous.", "comment.", "agent."].iter().any(|prefix| key.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_with_lists() -> TaskDTO {
        serde_json::from_value(serde_json::json!({
            "id": "WEB-7",
            "title": "Lists",
            "status": "Todo",
            "priority": "Medium",
            "task_type": "Feature",
            "created": "now",
            "modified": "now",
            "tags": ["ui", "agents"],
            "acceptance_criteria": ["Shows the cost", "Works offline"],
            "relationships": { "depends_on": ["WEB-1", "WEB-2"], "parent": "WEB-0" },
            "comments": [
                { "date": "2026-10-01", "text": "First" },
                { "date": "2026-10-02", "author": "mallox", "text": "Latest note" }
            ],
            "references": [
                { "link": "https://example.com/spec" },
                { "code": "src/app.rs#L10" },
                { "link": "https://example.com/design" }
            ],
            "sprints": [3]
        }))
        .expect("task json")
    }

    #[test]
    fn lists_expand_with_selectors() {
        let ctx = TemplateContext::from_task(&task_with_lists());
        assert_eq!(ctx.expand("${{ticket.tags[*]}}"), "ui, agents");
        assert_eq!(ctx.expand("${{ticket.tags[0]}}"), "ui");
        assert_eq!(ctx.expand("${{ticket.tags}}"), "ui,agents", "scalar form unchanged");
        assert_eq!(ctx.expand("${{ticket.references[*].link}}"), "https://example.com/spec, https://example.com/design");
        assert_eq!(ctx.expand("${{ticket.references[*]}}"), "https://example.com/spec, src/app.rs#L10, https://example.com/design");
        assert_eq!(ctx.expand("${{ticket.references[1].code}}"), "src/app.rs#L10");
        assert_eq!(ctx.expand("${{ticket.comments[-1].text}}"), "Latest note");
        assert_eq!(ctx.expand("${{ticket.comments[-1].date}}"), "2026-10-02");
        assert_eq!(ctx.expand("${{ticket.comments[-1].author}}"), "mallox");
        assert_eq!(ctx.expand("${{ticket.comments[0].author}}"), "");
        assert_eq!(ctx.expand("${{ticket.relationships.depends_on[*]}}"), "WEB-1, WEB-2");
        assert_eq!(ctx.expand("${{ticket.relationships.parent}}"), "WEB-0");
        assert_eq!(ctx.expand("${{ticket.acceptance_criteria[*]}}"), "Shows the cost, Works offline");
        assert_eq!(ctx.expand("${{ticket.sprints[*]}}"), "3");
        // Out of range, unknown fields and malformed selectors expand to nothing.
        assert_eq!(ctx.expand("[${{ticket.tags[5]}}]"), "[]");
        assert_eq!(ctx.expand("[${{ticket.comments[-9].text}}]"), "[]");
        assert_eq!(ctx.expand("[${{ticket.references[0].nope}}]"), "[]");
        assert_eq!(ctx.expand("[${{ticket.tags[x]}}]"), "[]");
        // Shell expansion passes the joined value as one variable.
        let shell = ctx.expand_for_shell("echo ${{ticket.tags[*]}}");
        assert_eq!(shell.script, "echo \"${LOTAR_TEMPLATE_1}\"");
        assert_eq!(shell.env, vec![("LOTAR_TEMPLATE_1".to_string(), "ui, agents".to_string())]);
    }

    #[test]
    fn job_usage_is_available_to_templates() {
        let job = AutomationJobContext {
            job_id: "job-1".to_string(),
            runner: "claude".to_string(),
            agent: Some("implement".to_string()),
            worktree_path: None,
            worktree_branch: None,
            usage: Some(crate::api_types::AgentJobUsage {
                input_tokens: Some(1200),
                output_tokens: Some(300),
                cost_usd: Some(0.0123),
                turns: Some(3),
                ..Default::default()
            }),
        };
        let ctx = TemplateContext::new().with_job(Some(&job));
        assert_eq!(ctx.expand("${{agent.cost_usd}} / ${{agent.input_tokens}} / ${{agent.output_tokens}} / ${{agent.turns}}"), "0.0123 / 1200 / 300 / 3");
        assert_eq!(ctx.expand("${{agent.usage}}"), "1,200 tokens in, 300 out · $0.0123 · 3 turns");
    }

    #[test]
    fn expand_basic_variables() {
        let mut ctx = TemplateContext::new();
        ctx.set("ticket.id", "PROJ-1");
        ctx.set("ticket.title", "Fix bug");
        assert_eq!(ctx.expand("Working on ${{ticket.id}}: ${{ticket.title}}"), "Working on PROJ-1: Fix bug");
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
        assert_eq!(ctx.expand("${{ticket.id}} and ${{ticket.id}}"), "X-1 and X-1");
    }

    #[test]
    fn shell_expansion_never_puts_values_in_the_script() {
        let mut ctx = TemplateContext::new();
        ctx.set("ticket.title", "Hello $(touch INJECTED) '; echo x #");
        ctx.set("ticket.id", "X-1");
        let unquoted = ctx.expand_for_shell("echo ${{ticket.title}} ${{ticket.id}} ${{ticket.title}}");
        assert_eq!(unquoted.script, "echo \"${LOTAR_TEMPLATE_1}\" \"${LOTAR_TEMPLATE_2}\" \"${LOTAR_TEMPLATE_1}\"");
        assert_eq!(unquoted.env.len(), 2, "one variable per distinct key");
        assert!(!unquoted.script.contains("touch"));

        let double = ctx.expand_for_shell("echo \"New: ${{ticket.title}}\"");
        assert_eq!(double.script, "echo \"New: ${LOTAR_TEMPLATE_1}\"");

        let nested = ctx.expand_for_shell("sh -c 'echo New task: ${{ticket.title}} >> log'");
        assert_eq!(nested.script, "sh -c 'echo New task: ${LOTAR_TEMPLATE_1} >> log'");

        let escaped = ctx.expand_for_shell("echo \\' ${{ticket.id}}");
        assert_eq!(escaped.script, "echo \\' \"${LOTAR_TEMPLATE_1}\"");
    }

    #[test]
    fn shell_expansion_unknown_keys_stay_empty() {
        let ctx = TemplateContext::new();
        let shell = ctx.expand_for_shell("echo ${{unknown}}");
        assert_eq!(shell.script, "echo ");
        assert!(shell.env.is_empty());
    }
}
