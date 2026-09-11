use crate::cli::TaskSearchArgs;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::validation::CliValidator;
use crate::config::types::ResolvedConfig;
use crate::services::task_query::{SortOrder, SortSpec};
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
        let build = Self::build_task_filter(&args, &validator, project, &ctx)?;

        #[allow(clippy::drop_non_drop)]
        drop(validator);

        renderer.log_debug("list: executing search");
        let mut tasks: Vec<(String, Task)> =
            ctx.storage.search(&build.task_filter).into_iter().collect();

        TaskPostFilters::new(&args, &ctx.config, resolver, build.where_filters)
            .apply(&mut tasks)?;

        let total_matching = tasks.len();
        Self::apply_sort(tasks.as_mut_slice(), &args, &ctx.config)?;

        let (offset, limit) = Self::resolve_pagination(&args)?;
        Self::apply_offset_and_limit(&mut tasks, offset, limit);

        Self::render_results(renderer, tasks, total_matching, offset, limit, args.details);

        Ok(())
    }
}

impl SearchHandler {
    fn build_task_filter(
        args: &TaskSearchArgs,
        validator: &CliValidator,
        project: Option<&str>,
        ctx: &TaskCommandContext,
    ) -> Result<BuiltTaskFilter, String> {
        let mut task_filter = TaskFilter::default();

        if let Some(query) = args.query.as_ref()
            && !query.is_empty()
        {
            task_filter.text_query = Some(query.clone());
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

        let (custom_where, remaining_where, _) =
            crate::utils::custom_fields::partition_where_filters(&args.r#where, &ctx.config);
        for (name, values) in custom_where {
            let entry = task_filter.custom_fields.entry(name).or_default();
            entry.extend(values);
        }

        Ok(BuiltTaskFilter {
            task_filter,
            where_filters: remaining_where,
        })
    }

    /// Resolve the CLI sort key into the shared executor spec. CLI keeps one
    /// extra grammar form over strict REST/MCP (DEV-57): bare declared or
    /// wildcard custom-field names are accepted. Any other unknown key is an
    /// explicit error, matching the shared strict-query contract.
    fn resolve_sort_spec(sort_key: &str, config: &ResolvedConfig) -> Result<SortSpec, String> {
        if let Ok(spec) = crate::services::task_query::parse_sort_by(sort_key) {
            return Ok(spec);
        }
        let key_raw = sort_key.trim();
        if !key_raw.is_empty()
            && (config.custom_fields.has_wildcard()
                || config
                    .custom_fields
                    .values
                    .iter()
                    .any(|v| v.eq_ignore_ascii_case(key_raw)))
        {
            return Ok(SortSpec::Custom(key_raw.to_string()));
        }
        Err(format!(
            "Invalid --sort-by '{key_raw}': not a builtin key or configured custom field"
        ))
    }

    /// Shared-executor sorting (DEV-57): without `--sort-by` the CLI now uses
    /// the shared default order (modified desc, canonical-ID asc tie) like
    /// REST and MCP; an invalid explicit key is an error instead of a silent
    /// no-op; `--reverse` maps to descending order (the tiebreak never
    /// flips).
    fn apply_sort(
        tasks: &mut [(String, Task)],
        args: &TaskSearchArgs,
        config: &ResolvedConfig,
    ) -> Result<(), String> {
        let (spec, order) = match args.sort_by.as_deref() {
            Some(sort_key) => {
                let spec = Self::resolve_sort_spec(sort_key, config)?;
                let order = if args.reverse {
                    SortOrder::Desc
                } else {
                    SortOrder::Asc
                };
                (spec, order)
            }
            None => (
                SortSpec::Builtin(crate::services::task_query::SortKey::Modified),
                SortOrder::Desc,
            ),
        };
        crate::services::task_query::sort_tasks(tasks, &spec, order);
        Ok(())
    }

    fn resolve_pagination(args: &TaskSearchArgs) -> Result<(usize, usize), String> {
        let page_size = args.page_size;
        let offset = if let Some(page) = args.page {
            let page_index = page
                .checked_sub(1)
                .ok_or_else(|| "--page must be >= 1".to_string())?;
            page_index
                .checked_mul(page_size)
                .ok_or_else(|| "--page is too large".to_string())?
        } else {
            args.offset.unwrap_or(0)
        };

        Ok((offset, page_size))
    }

    fn apply_offset_and_limit(tasks: &mut Vec<(String, Task)>, offset: usize, limit: usize) {
        if offset > 0 {
            if offset >= tasks.len() {
                tasks.clear();
                return;
            }
            tasks.drain(..offset);
        }

        tasks.truncate(limit);
    }

    fn render_results(
        renderer: &crate::output::OutputRenderer,
        tasks: Vec<(String, Task)>,
        total_matching: usize,
        offset: usize,
        limit: usize,
        show_details: bool,
    ) {
        if tasks.is_empty() {
            renderer.log_info("list: no results");
            let current_page = offset.checked_div(limit).map(|q| q + 1).unwrap_or(1);
            let total_pages = if limit == 0 {
                1
            } else {
                total_matching.div_ceil(limit).max(1)
            };
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    renderer.emit_raw_stdout(
                        serde_json::json!({
                            "status": "success",
                            "message": if total_matching == 0 {
                                "No tasks found".to_string()
                            } else {
                                format!(
                                    "Found {} task(s) matching filters, showing 0 (offset {}, limit {})",
                                    total_matching, offset, limit
                                )
                            },
                            "tasks": [],
                            "total": total_matching,
                            "limit": limit,
                            "offset": offset,
                            "page": current_page,
                            "total_pages": total_pages,
                            "has_more": false,
                            "has_previous": offset > 0,
                            "next_offset": serde_json::Value::Null,
                            "next_page": serde_json::Value::Null,
                        })
                        .to_string(),
                    );
                }
                _ => {
                    if total_matching == 0 {
                        renderer.emit_warning("No tasks found matching the search criteria.");
                    } else {
                        renderer.emit_warning(format_args!(
                            "No tasks on this page (offset {}, limit {}, total {}).",
                            offset, limit, total_matching
                        ));
                        renderer.emit_raw_stdout(
                            "  Try --page 1 or --offset 0 to return to the first page, or increase --page-size."
                                .to_string(),
                        );
                    }
                }
            }
            return;
        }

        renderer.log_info(format_args!("list: {} result(s)", tasks.len()));

        let display_tasks: Vec<crate::output::TaskDisplayInfo> = tasks
            .into_iter()
            .map(|(task_id, task)| {
                let project = crate::storage::TaskId::parse(&task_id)
                    .ok()
                    .map(|parsed| parsed.project);

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

        let shown = display_tasks.len();
        let start = if shown == 0 { 0 } else { offset + 1 };
        let end = offset + shown;
        let has_more = end < total_matching;
        let has_previous = offset > 0;
        let current_page = offset.checked_div(limit).map(|q| q + 1).unwrap_or(1);
        let total_pages = if limit == 0 {
            1
        } else {
            total_matching.div_ceil(limit).max(1)
        };
        let next_offset = if has_more { Some(end) } else { None };
        let next_page = if has_more {
            Some(current_page + 1)
        } else {
            None
        };

        match renderer.format {
            crate::output::OutputFormat::Json => {
                renderer.emit_raw_stdout(
                    serde_json::json!({
                        "status": "success",
                        "message": format!("Found {} task(s)", display_tasks.len()),
                        "tasks": display_tasks,
                        "total": total_matching,
                        "limit": limit,
                        "offset": offset,
                        "page": current_page,
                        "total_pages": total_pages,
                        "has_more": has_more,
                        "has_previous": has_previous,
                        "next_offset": next_offset,
                        "next_page": next_page,
                    })
                    .to_string(),
                );
            }
            _ => {
                renderer.emit_success(format_args!("Found {} task(s):", display_tasks.len()));
                renderer.emit_raw_stdout(format_args!(
                    "  (showing {}–{} of {}, page {} of {}, offset {}, page-size {})",
                    start, end, total_matching, current_page, total_pages, offset, limit
                ));
                for task in display_tasks {
                    let assignee = task.assignee.as_deref().unwrap_or("unassigned");
                    let mut line = format!(
                        "  {} - {} [{}] ({}) | type: {} | assignee: {}",
                        task.id, task.title, task.status, task.priority, task.task_type, assignee
                    );

                    if let Some(due_date) = task.due_date.as_deref() {
                        line.push_str(&format!(" | due: {}", due_date));
                    }

                    if let Some(effort) = task.effort.as_deref() {
                        line.push_str(&format!(" | effort: {}", effort));
                    }

                    renderer.emit_raw_stdout(line);

                    if show_details
                        && let Some(description) = &task.description
                        && !description.is_empty()
                    {
                        renderer.emit_raw_stdout(format_args!("    {}", description));
                    }
                }

                if has_more {
                    let remaining = total_matching - end;
                    renderer.emit_raw_stdout(format_args!(
                        "  … {} more task(s) not shown. Use --page {} (or --offset {}) to see the next page, or --page-size <N> to change page size.",
                        remaining,
                        current_page + 1,
                        end
                    ));
                } else if has_previous {
                    renderer.emit_raw_stdout(
                        "  (end of results — use --page 1 or --offset 0 to return to the first page)"
                            .to_string(),
                    );
                }
            }
        }
    }
}

struct BuiltTaskFilter {
    task_filter: TaskFilter,
    where_filters: Vec<(String, String)>,
}

struct TaskPostFilters<'a> {
    args: &'a TaskSearchArgs,
    config: &'a ResolvedConfig,
    resolver: &'a TasksDirectoryResolver,
    where_filters: Vec<(String, String)>,
}

impl<'a> TaskPostFilters<'a> {
    fn new(
        args: &'a TaskSearchArgs,
        config: &'a ResolvedConfig,
        resolver: &'a TasksDirectoryResolver,
        where_filters: Vec<(String, String)>,
    ) -> Self {
        Self {
            args,
            config,
            resolver,
            where_filters,
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

    /// Assignee filtering uses the shared fuzzy member predicate
    /// (`member_key` equality, same as REST/TaskService) instead of exact
    /// string equality, so `--assignee alice` matches stored `Alice` and
    /// `@alice` (DEV-57 alignment).
    fn apply_assignee_filter(&self, tasks: &mut Vec<(String, Task)>) {
        if let Some(assignee) = self.args.assignee.as_ref() {
            let target = if assignee == "@me" {
                crate::utils::identity::resolve_current_user(Some(self.resolver.path.as_path()))
            } else {
                Some(assignee.clone())
            };

            match target {
                Some(user) => {
                    let key = crate::utils::fuzzy_match::member_key(&user);
                    tasks.retain(|(_, task)| {
                        task.assignee
                            .as_deref()
                            .is_some_and(|a| crate::utils::fuzzy_match::member_key(a) == key)
                    });
                }
                None => tasks.clear(),
            }
        }
    }

    fn apply_mine_filter(&self, tasks: &mut Vec<(String, Task)>) {
        if self.args.mine {
            if let Some(me) =
                crate::utils::identity::resolve_current_user(Some(self.resolver.path.as_path()))
            {
                let key = crate::utils::fuzzy_match::member_key(&me);
                tasks.retain(|(_, task)| {
                    task.assignee
                        .as_deref()
                        .is_some_and(|a| crate::utils::fuzzy_match::member_key(a) == key)
                });
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

    /// Due flags share the executor's local-date bucket predicates
    /// (DEV-57): `--overdue` matches due dates strictly before today and
    /// `--due-soon[=days]` matches today through today+days inclusive, so a
    /// task due later today is "today" work, never overdue, and stored
    /// date-only values never depend on local-midnight resolution.
    fn apply_due_filters(&self, tasks: &mut Vec<(String, Task)>) {
        if !self.args.overdue && self.args.due_soon.is_none() {
            return;
        }
        let today = crate::services::task_query::today_local(chrono::Utc::now());
        if self.args.overdue {
            tasks.retain(|(_, task)| {
                crate::services::task_query::due_raw_matches_bucket(
                    task.due_date.as_deref(),
                    crate::services::task_query::DueFilter::Overdue,
                    today,
                )
            });
        }

        if let Some(due_soon_arg) = self.args.due_soon {
            let days = match due_soon_arg {
                Some(n) => n as i64,
                None => 7,
            };
            tasks.retain(|(_, task)| {
                crate::services::task_query::due_raw_within_window(
                    task.due_date.as_deref(),
                    today,
                    days,
                )
            });
        }
    }

    fn apply_where_filters(&self, tasks: &mut Vec<(String, Task)>) {
        if self.where_filters.is_empty() {
            return;
        }

        use std::collections::{HashMap, HashSet};

        let mut filters: HashMap<String, HashSet<String>> = HashMap::new();
        for (key, value) in &self.where_filters {
            filters
                .entry(key.clone())
                .or_default()
                .insert(value.clone());
        }

        let resolve_vals = |id: &str, task: &Task, key: &str| -> Option<Vec<String>> {
            crate::utils::custom_fields::resolve_task_filter_values(id, task, key, self.config)
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
