use crate::cli::TaskSearchArgs;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::validation::CliValidator;
use crate::config::types::ResolvedConfig;
use crate::storage::{TaskFilter, task::Task};
use crate::workspace::TasksDirectoryResolver;

/// Handler for searching tasks
pub struct SearchHandler;

impl CommandHandler for SearchHandler {
    type Args = TaskSearchArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
    ) -> Self::Result {
        renderer.log_info("list: begin");
        let ctx = TaskCommandContext::new(resolver, project, None)?;
        let validator = CliValidator::new(&ctx.config);
        let task_filter = Self::build_task_filter(&args, &validator, project, &ctx)?;

        #[allow(clippy::drop_non_drop)]
        drop(validator);

        renderer.log_debug("list: executing search");
        let mut tasks: Vec<(String, Task)> = ctx.storage.search(&task_filter).into_iter().collect();

        TaskPostFilters::new(&args, &ctx.config, resolver).apply(&mut tasks)?;
        Self::apply_sort_and_limit(&mut tasks, &args, &ctx.config);
        Self::render_results(renderer, tasks);

        Ok(())
    }
}

impl SearchHandler {
    fn build_task_filter(
        args: &TaskSearchArgs,
        validator: &CliValidator,
        project: Option<&str>,
        ctx: &TaskCommandContext,
    ) -> Result<TaskFilter, String> {
        let mut task_filter = TaskFilter::default();

        if let Some(query) = args.query.as_ref() {
            if !query.is_empty() {
                task_filter.text_query = Some(query.clone());
            }
        }

        for status in &args.status {
            let validated_status = validator
                .validate_status(status)
                .map_err(|e| format!("Status validation failed: {}", e))?;
            task_filter.status.push(validated_status);
        }

        for priority in &args.priority {
            let validated_priority = validator
                .validate_priority(priority)
                .map_err(|e| format!("Priority validation failed: {}", e))?;
            task_filter.priority.push(validated_priority);
        }

        for task_type in &args.task_type {
            let validated_type = validator
                .validate_task_type(task_type)
                .map_err(|e| format!("Task type validation failed: {}", e))?;
            task_filter.task_type.push(validated_type);
        }

        task_filter.tags = args.tag.clone();

        if let Some(project_arg) = project {
            let project_prefix = ctx.project_prefix_for(Some(project_arg));
            task_filter.project = Some(project_prefix);
        }

        Ok(task_filter)
    }

    fn apply_sort_and_limit(
        tasks: &mut Vec<(String, Task)>,
        args: &TaskSearchArgs,
        config: &ResolvedConfig,
    ) {
        if let Some(sort_key) = args.sort_by.as_deref() {
            let key_raw = sort_key.trim();
            let key = key_raw.to_lowercase();
            tasks.sort_by(|(id_a, task_a), (id_b, task_b)| {
                use std::cmp::Ordering::*;

                let ordering = match key.as_str() {
                    "priority" => task_a.priority.as_str().cmp(task_b.priority.as_str()),
                    "status" => task_a.status.as_str().cmp(task_b.status.as_str()),
                    "effort" => {
                        let effort_a = task_a
                            .effort
                            .as_deref()
                            .and_then(|s| crate::utils::effort::parse_effort(s).ok());
                        let effort_b = task_b
                            .effort
                            .as_deref()
                            .and_then(|s| crate::utils::effort::parse_effort(s).ok());

                        match (effort_a, effort_b) {
                            (Some(x), Some(y)) => match (x.kind, y.kind) {
                                (
                                    crate::utils::effort::EffortKind::TimeHours(ax),
                                    crate::utils::effort::EffortKind::TimeHours(by),
                                ) => ax.partial_cmp(&by).unwrap_or(Equal),
                                (
                                    crate::utils::effort::EffortKind::Points(ax),
                                    crate::utils::effort::EffortKind::Points(by),
                                ) => ax.partial_cmp(&by).unwrap_or(Equal),
                                _ => x.canonical.cmp(&y.canonical),
                            },
                            (Some(_), None) => Less,
                            (None, Some(_)) => Greater,
                            (None, None) => Equal,
                        }
                    }
                    "due-date" | "due" => match (&task_a.due_date, &task_b.due_date) {
                        (Some(x), Some(y)) => x.cmp(y),
                        (Some(_), None) => Less,
                        (None, Some(_)) => Greater,
                        (None, None) => Equal,
                    },
                    "created" => task_a.created.cmp(&task_b.created),
                    "modified" => task_a.modified.cmp(&task_b.modified),
                    "assignee" => task_a.assignee.cmp(&task_b.assignee),
                    "type" => task_a
                        .task_type
                        .to_string()
                        .cmp(&task_b.task_type.to_string()),
                    "project" => id_a.split('-').next().cmp(&id_b.split('-').next()),
                    "id" => id_a.cmp(id_b),
                    other => {
                        let mut name_opt: Option<&str> = None;
                        if let Some(rest) = other.strip_prefix("field:") {
                            name_opt = Some(rest.trim());
                        } else if config.custom_fields.has_wildcard()
                            || config
                                .custom_fields
                                .values
                                .iter()
                                .any(|v| v.eq_ignore_ascii_case(key_raw))
                        {
                            name_opt = Some(key_raw);
                        }

                        if let Some(name) = name_opt {
                            let pick = |task: &Task| -> String {
                                if let Some(value) = task.custom_fields.get(name) {
                                    return crate::types::custom_value_to_string(value);
                                }
                                let lower = name.to_lowercase();
                                if let Some((_, value)) = task
                                    .custom_fields
                                    .iter()
                                    .find(|(k, _)| k.to_lowercase() == lower)
                                {
                                    return crate::types::custom_value_to_string(value);
                                }
                                String::new()
                            };

                            pick(task_a).cmp(&pick(task_b))
                        } else {
                            Equal
                        }
                    }
                };

                if args.reverse {
                    ordering.reverse()
                } else {
                    ordering
                }
            });
        }

        tasks.truncate(args.limit);
    }

    fn render_results(renderer: &crate::output::OutputRenderer, tasks: Vec<(String, Task)>) {
        if tasks.is_empty() {
            renderer.log_info("list: no results");
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    renderer.emit_raw_stdout(
                        &serde_json::json!({
                            "status": "success",
                            "message": "No tasks found",
                            "tasks": []
                        })
                        .to_string(),
                    );
                }
                _ => {
                    renderer.emit_warning("No tasks found matching the search criteria.");
                }
            }
            return;
        }

        renderer.log_info(&format!("list: {} result(s)", tasks.len()));

        let display_tasks: Vec<crate::output::TaskDisplayInfo> = tasks
            .into_iter()
            .map(|(task_id, task)| {
                let project = task_id
                    .find('-')
                    .map(|dash_pos| task_id[..dash_pos].to_string());

                crate::output::TaskDisplayInfo {
                    id: task_id,
                    title: task.title,
                    status: task.status.to_string(),
                    priority: task.priority.to_string(),
                    task_type: task.task_type.to_string(),
                    description: task.description,
                    assignee: task.assignee,
                    project,
                    due_date: task.due_date,
                    effort: task.effort,
                    tags: task.tags,
                    created: task.created,
                    modified: task.modified,
                    custom_fields: task.custom_fields,
                }
            })
            .collect();

        match renderer.format {
            crate::output::OutputFormat::Json => {
                renderer.emit_raw_stdout(
                    &serde_json::json!({
                        "status": "success",
                        "message": format!("Found {} task(s)", display_tasks.len()),
                        "tasks": display_tasks
                    })
                    .to_string(),
                );
            }
            _ => {
                renderer.emit_success(&format!("Found {} task(s):", display_tasks.len()));
                for task in display_tasks {
                    renderer.emit_raw_stdout(&format!(
                        "  {} - {} [{}] ({})",
                        task.id, task.title, task.status, task.priority
                    ));
                    if let Some(description) = &task.description {
                        if !description.is_empty() {
                            renderer.emit_raw_stdout(&format!("    {}", description));
                        }
                    }
                }
            }
        }
    }
}

struct TaskPostFilters<'a> {
    args: &'a TaskSearchArgs,
    config: &'a ResolvedConfig,
    resolver: &'a TasksDirectoryResolver,
}

impl<'a> TaskPostFilters<'a> {
    fn new(
        args: &'a TaskSearchArgs,
        config: &'a ResolvedConfig,
        resolver: &'a TasksDirectoryResolver,
    ) -> Self {
        Self {
            args,
            config,
            resolver,
        }
    }

    fn apply(&self, tasks: &mut Vec<(String, Task)>) -> Result<(), String> {
        self.apply_assignee_filter(tasks);
        self.apply_mine_filter(tasks);
        self.apply_priority_flags(tasks);
        self.apply_due_filters(tasks);
        self.apply_where_filters(tasks);
        self.apply_effort_filters(tasks)?;
        Ok(())
    }

    fn apply_assignee_filter(&self, tasks: &mut Vec<(String, Task)>) {
        if let Some(assignee) = self.args.assignee.as_ref() {
            let target = if assignee == "@me" {
                crate::utils::identity::resolve_current_user(Some(self.resolver.path.as_path()))
            } else {
                Some(assignee.clone())
            };

            if let Some(user) = target {
                tasks.retain(|(_, task)| task.assignee.as_ref() == Some(&user));
            } else {
                tasks.clear();
            }
        }
    }

    fn apply_mine_filter(&self, tasks: &mut Vec<(String, Task)>) {
        if self.args.mine {
            if let Some(me) =
                crate::utils::identity::resolve_current_user(Some(self.resolver.path.as_path()))
            {
                tasks.retain(|(_, task)| task.assignee.as_ref() == Some(&me));
            } else {
                tasks.clear();
            }
        }
    }

    fn apply_priority_flags(&self, tasks: &mut Vec<(String, Task)>) {
        if self.args.high {
            tasks.retain(|(_, task)| task.priority.eq_ignore_case("high"));
        }

        if self.args.critical {
            tasks.retain(|(_, task)| task.priority.eq_ignore_case("critical"));
        }
    }

    fn apply_due_filters(&self, tasks: &mut Vec<(String, Task)>) {
        if self.args.overdue {
            let now = chrono::Utc::now();
            tasks.retain(|(_, task)| {
                if let Some(ref due) = task.due_date {
                    if let Some(dt) = crate::cli::validation::parse_due_string_to_utc(due) {
                        return dt < now;
                    }
                }
                false
            });
        }

        if let Some(due_soon_arg) = self.args.due_soon {
            let days = match due_soon_arg {
                Some(n) => n as i64,
                None => 7,
            };
            let now = chrono::Utc::now();
            let cutoff = now + chrono::Duration::days(days);
            tasks.retain(|(_, task)| {
                if let Some(ref due) = task.due_date {
                    if let Some(dt) = crate::cli::validation::parse_due_string_to_utc(due) {
                        return dt >= now && dt <= cutoff;
                    }
                }
                false
            });
        }
    }

    fn apply_where_filters(&self, tasks: &mut Vec<(String, Task)>) {
        if self.args.r#where.is_empty() {
            return;
        }

        use std::collections::{HashMap, HashSet};

        let mut filters: HashMap<String, HashSet<String>> = HashMap::new();
        for (key, value) in &self.args.r#where {
            filters
                .entry(key.clone())
                .or_default()
                .insert(value.clone());
        }

        let resolve_vals = |id: &str, task: &Task, key: &str| -> Option<Vec<String>> {
            let raw = key.trim();
            let normalized = raw.to_lowercase();

            if let Some(canonical) = crate::utils::fields::is_reserved_field(raw) {
                match canonical {
                    "assignee" => return Some(vec![task.assignee.clone().unwrap_or_default()]),
                    "reporter" => return Some(vec![task.reporter.clone().unwrap_or_default()]),
                    "type" => return Some(vec![task.task_type.to_string()]),
                    "status" => return Some(vec![task.status.to_string()]),
                    "priority" => return Some(vec![task.priority.to_string()]),
                    "project" => {
                        return Some(vec![id.split('-').next().unwrap_or("").to_string()]);
                    }
                    "tags" => return Some(task.tags.clone()),
                    _ => {}
                }
            }

            let mut field_name: Option<&str> = None;
            if let Some(rest) = normalized.strip_prefix("field:") {
                field_name = Some(rest.trim());
            } else if self
                .args
                .r#where
                .iter()
                .any(|(k, _)| k.trim().eq_ignore_ascii_case(raw))
                && (self.config.custom_fields.has_wildcard()
                    || self.config.custom_fields.values.iter().any(|v| v == raw))
            {
                field_name = Some(raw);
            }

            if let Some(name) = field_name {
                if let Some(value) = task.custom_fields.get(name) {
                    return Some(vec![crate::types::custom_value_to_string(value)]);
                }

                let lower = name.to_lowercase();
                if let Some((_, value)) = task
                    .custom_fields
                    .iter()
                    .find(|(k, _)| k.to_lowercase() == lower)
                {
                    return Some(vec![crate::types::custom_value_to_string(value)]);
                }
            }

            None
        };

        tasks.retain(|(id, task)| {
            for (key, allowed) in &filters {
                let values = match resolve_vals(id, task, key) {
                    Some(v) => v.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>(),
                    None => return false,
                };
                if values.is_empty() {
                    return false;
                }

                let allowed_vec: Vec<String> = allowed.iter().cloned().collect();
                if !crate::utils::fuzzy_match::fuzzy_set_match(&values, &allowed_vec) {
                    return false;
                }
            }
            true
        });
    }

    fn apply_effort_filters(&self, tasks: &mut Vec<(String, Task)>) -> Result<(), String> {
        if self.args.effort_min.is_none() && self.args.effort_max.is_none() {
            return Ok(());
        }

        let min_parsed = self
            .args
            .effort_min
            .as_ref()
            .map(|value| crate::utils::effort::parse_effort(value));
        let max_parsed = self
            .args
            .effort_max
            .as_ref()
            .map(|value| crate::utils::effort::parse_effort(value));

        let min = match min_parsed.transpose() {
            Ok(value) => value,
            Err(e) => return Err(format!("Invalid --effort-min: {}", e)),
        };
        let max = match max_parsed.transpose() {
            Ok(value) => value,
            Err(e) => return Err(format!("Invalid --effort-max: {}", e)),
        };

        tasks.retain(|(_, task)| {
            let Some(effort) = task.effort.as_deref() else {
                return false;
            };

            let parsed = match crate::utils::effort::parse_effort(effort) {
                Ok(p) => p,
                Err(_) => return false,
            };

            let mut keep = true;
            if let Some(min_value) = min.as_ref() {
                keep &= parsed.total_cmp_ge(min_value);
            }
            if let Some(max_value) = max.as_ref() {
                keep &= parsed.total_cmp_le(max_value);
            }
            keep
        });

        Ok(())
    }
}
