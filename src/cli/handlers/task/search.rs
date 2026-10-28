use crate::api_types::TaskDTO;
use crate::cli::TaskSearchArgs;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::config::types::ResolvedConfig;
use crate::services::task_list::{TaskQueryError, TaskQueryInput};
use crate::storage::DeletionFilter;
use crate::workspace::TasksDirectoryResolver;

/// Handler for `lotar list`: maps the flags onto the shared task query
/// (DEV-230), then pages and prints the result.
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
        let project_prefix = project.map(|p| ctx.project_prefix_for(Some(p)));
        let (input, matches_nothing) = Self::query_input(&args, project_prefix, &ctx.config)?;
        // Enums and fields validate against the context's configuration (the
        // detected project's when there is one), as the CLI always has.
        let query = input
            .resolve(&ctx.config, resolver.path.as_path())
            .map_err(|e| match e {
                TaskQueryError::InvalidEnum { field, message, .. } => {
                    let label = match field {
                        "status" => "Status",
                        "priority" => "Priority",
                        _ => "Task type",
                    };
                    format!("{label} validation failed: {message}")
                }
                other => other.to_string(),
            })?;

        renderer.log_debug("list: executing search");
        let tasks = if matches_nothing {
            Vec::new()
        } else {
            crate::services::task_list::run(&ctx.storage, &query, chrono::Utc::now())
        };
        let total_matching = tasks.len();
        let (offset, limit) = Self::resolve_pagination(&args)?;
        let (start, end) = crate::utils::pagination::slice_bounds(total_matching, offset, limit);
        let page = tasks.into_iter().skip(start).take(end - start).collect();

        Self::render_results(renderer, page, total_matching, offset, limit, args.details);

        Ok(())
    }
}

impl SearchHandler {
    /// The shared query input for the CLI flags. Shortcut flags map onto
    /// the shared filters: `--mine` is `@me`, `--high`/`--critical` add
    /// priorities, `--overdue` is the overdue bucket and `--due-soon[=N]`
    /// is due within N days. The flag is true when nothing can match: only
    /// priority shortcuts were given and the project has none of them.
    fn query_input(
        args: &TaskSearchArgs,
        project: Option<String>,
        config: &ResolvedConfig,
    ) -> Result<(TaskQueryInput, bool), String> {
        let mut priority = args.priority.clone();
        // A shortcut for a priority the project doesn't use matches nothing
        // rather than failing: `--critical` is a quick look, not a value.
        let shortcuts: Vec<&str> = [(args.high, "high"), (args.critical, "critical")]
            .into_iter()
            .filter_map(|(on, name)| on.then_some(name))
            .collect();
        let usable: Vec<&str> = shortcuts
            .iter()
            .copied()
            .filter(|name| crate::types::Priority::parse_with_config(name, config).is_ok())
            .collect();
        let matches_nothing = !shortcuts.is_empty() && usable.is_empty() && priority.is_empty();
        priority.extend(usable.into_iter().map(str::to_string));
        let assignee = if args.mine {
            Some("@me".to_string())
        } else if args.unassigned {
            Some("__none__".to_string())
        } else {
            args.assignee.clone()
        };
        let due = if args.overdue {
            Some("overdue".to_string())
        } else {
            args.due.clone()
        };
        let csv = |values: &[String]| (!values.is_empty()).then(|| values.join(","));

        let input = TaskQueryInput {
            project,
            status: args.status.clone(),
            priority,
            task_type: args.task_type.clone(),
            tags: args.tag.clone(),
            text: args.query.clone().filter(|q| !q.is_empty()),
            sprints: args.sprint.clone(),
            assignee,
            fields: Self::where_fields(args, config)?,
            deletion: if args.deleted {
                DeletionFilter::Deleted
            } else if args.include_deleted {
                DeletionFilter::All
            } else {
                DeletionFilter::Active
            },
            due,
            due_within: args.due_soon.map(|days| days.unwrap_or(7).to_string()),
            recent: args.recent.clone(),
            needs: csv(&args.needs),
            has: csv(&args.has),
            effort_min: args.effort_min.clone(),
            effort_max: args.effort_max.clone(),
            sort_by: args
                .sort_by
                .as_deref()
                .map(|key| Self::sort_key(key, config))
                .transpose()?,
            order: Some(Self::order(args).to_string()),
        };
        Ok((input, matches_nothing))
    }

    /// `--where` pairs. The CLI is stricter than REST: only equality on a
    /// supported builtin or a known custom field.
    fn where_fields(
        args: &TaskSearchArgs,
        config: &ResolvedConfig,
    ) -> Result<Vec<(String, String)>, String> {
        let mut fields = Vec::new();
        for (key, value) in &args.r#where {
            let key = key.trim();
            if key.is_empty()
                || value.trim().is_empty()
                || key.ends_with(['!', '<', '>'])
                || key.eq_ignore_ascii_case("field:")
            {
                return Err(format!(
                    "Invalid --where '{key}={value}': only non-empty key=value equality predicates are supported"
                ));
            }
            // Builtins take precedence over wildcard custom fields, while
            // explicit field:<name> remains an unambiguous custom-field query.
            if let Some(field) = crate::utils::fields::is_reserved_field(key) {
                if !matches!(
                    field,
                    "assignee" | "reporter" | "status" | "priority" | "type" | "tags" | "project"
                ) {
                    return Err(format!("Unsupported --where field '{key}'"));
                }
                fields.push((field.to_string(), value.clone()));
            } else if let Some(name) = crate::utils::custom_fields::resolve_filter_name(key, config)
            {
                fields.push((format!("field:{name}"), value.clone()));
            } else {
                return Err(format!(
                    "Unknown --where field '{key}': use a supported builtin, configured custom field, or field:<name>"
                ));
            }
        }
        Ok(fields)
    }

    /// `--sort-by` for the shared query. The CLI also accepts a bare declared
    /// (or wildcard) custom field name; any other unknown key is an error.
    fn sort_key(sort_key: &str, config: &ResolvedConfig) -> Result<String, String> {
        if crate::services::task_query::parse_sort_by(sort_key).is_ok() {
            return Ok(sort_key.to_string());
        }
        let key = sort_key.trim();
        if !key.is_empty()
            && (config.custom_fields.has_wildcard()
                || config
                    .custom_fields
                    .values
                    .iter()
                    .any(|v| v.eq_ignore_ascii_case(key)))
        {
            return Ok(format!("field:{key}"));
        }
        Err(format!(
            "Invalid --sort-by '{key}': not a builtin key or configured custom field"
        ))
    }

    /// Without `--sort-by`: newest change first (`--reverse` has no effect).
    /// With a key: ascending, `--reverse` descending. Relevance lists the
    /// best match first, `--reverse` the weakest.
    fn order(args: &TaskSearchArgs) -> &'static str {
        let relevance = args
            .sort_by
            .as_deref()
            .is_some_and(|key| key.trim().eq_ignore_ascii_case("relevance"));
        match (args.sort_by.is_some(), relevance, args.reverse) {
            (false, _, _) => "desc",
            (true, true, reverse) => {
                if reverse {
                    "asc"
                } else {
                    "desc"
                }
            }
            (true, false, reverse) => {
                if reverse {
                    "desc"
                } else {
                    "asc"
                }
            }
        }
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

    fn render_results(
        renderer: &crate::output::OutputRenderer,
        tasks: Vec<(String, TaskDTO)>,
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
                    deleted_at: task.deleted_at,
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

                    // Lifecycle marker for mixed views: the stored title is
                    // never rewritten; tombstones carry their timestamp.
                    if let Some(deleted_at) = task.deleted_at.as_deref() {
                        line.push_str(&format!(" | deleted: {}", deleted_at));
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
