//! One task-list query for every interface (DEV-230).
//!
//! The CLI `list` command, REST `/api/tasks/list` and `/api/tasks/export`,
//! MCP `task_list` and the dependency graph all fill a [`TaskQueryInput`]
//! from their own wire format, [`TaskQueryInput::resolve`] validates it into
//! a [`TaskQuery`], and [`run`] loads, filters and orders the tasks. Each
//! interface keeps only its input mapping, pagination and output.

use crate::api_types::{TaskDTO, TaskListFilter};
use crate::config::types::ResolvedConfig;
use crate::services::task_query::{self, SortKey, SortSpec, TaskQueryOptions};
use crate::storage::DeletionFilter;
use crate::storage::manager::Storage;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::path::Path;

/// REST query keys with their own meaning; any other key is a field filter.
pub const KNOWN_QUERY_KEYS: &[&str] = &[
    "project",
    "status",
    "priority",
    "type",
    "tags",
    "sprints",
    "q",
    // The web search syntax (DEV-235), added to the other parameters.
    "query",
    "assignee",
    "order",
    "sort_by",
    "limit",
    "offset",
    "page_size",
    "per_page",
    "due",
    "due_within",
    "recent",
    "needs",
    "has",
    "effort_min",
    "effort_max",
    // RESERVED lifecycle property key (DEV-92): strict active|deleted|all
    // grammar, never interpreted as a custom-field filter.
    "deletion",
];

/// Raw query terms, as the interface received them. Values are trimmed and
/// validated by [`TaskQueryInput::resolve`]; multi-value lists are already
/// split.
#[derive(Debug, Clone, Default)]
pub struct TaskQueryInput {
    pub project: Option<String>,
    pub status: Vec<String>,
    pub priority: Vec<String>,
    pub task_type: Vec<String>,
    pub tags: Vec<String>,
    /// Search text (all words must match, see `utils::text_search`).
    pub text: Option<String>,
    pub sprints: Vec<u32>,
    /// A member name, `@me`, or `__none__` for unassigned tasks.
    pub assignee: Option<String>,
    /// Other `key=value` filters: custom fields (declared, wildcard or
    /// `field:<name>`) and builtin fields such as `reporter`, matched loosely.
    pub fields: Vec<(String, String)>,
    pub deletion: DeletionFilter,
    pub due: Option<String>,
    /// Due from today through today + N days, inclusive.
    pub due_within: Option<String>,
    pub recent: Option<String>,
    /// CSV of `effort`, `due`.
    pub needs: Option<String>,
    /// CSV of `attachment`, `image`, `link`, `tag`.
    pub has: Option<String>,
    pub effort_min: Option<String>,
    pub effort_max: Option<String>,
    pub sort_by: Option<String>,
    pub order: Option<String>,
    /// Excluded filters (DEV-5): a task matching any of these is left out.
    /// Each holds one filter, e.g. from `-status:done`.
    pub exclude: Vec<TaskQueryInput>,
}

/// A validated query, ready for [`run`].
#[derive(Debug, Clone)]
pub struct TaskQuery {
    /// Filters applied while loading tasks.
    pub filter: TaskListFilter,
    /// Field filters matched in memory against each task.
    pub field_filters: BTreeMap<String, BTreeSet<String>>,
    /// Smart filters and ordering.
    pub options: TaskQueryOptions,
    /// The configuration enums and field names were resolved against: the
    /// requested project's, else the base configuration.
    pub config: ResolvedConfig,
    /// Tasks matching any of these are left out (DEV-5).
    pub exclude: Vec<TaskQuery>,
}

/// Why a query was rejected. Interfaces map these onto their own error
/// shapes; the messages match the former per-interface ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskQueryError {
    /// A status, priority or type the configuration does not allow.
    InvalidEnum {
        /// `status`, `priority` or `type`.
        field: &'static str,
        value: String,
        message: String,
    },
    /// `@me` was requested but no identity is configured.
    UnresolvedMe,
    Invalid(String),
}

impl fmt::Display for TaskQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskQueryError::InvalidEnum { message, .. } => f.write_str(message),
            TaskQueryError::UnresolvedMe => f.write_str(
                "Could not resolve @me: no identity configured (default_reporter, git user.name, or USER env)",
            ),
            TaskQueryError::Invalid(message) => f.write_str(message),
        }
    }
}

impl TaskQueryError {
    /// The message as the CLI prints it: enum errors name the field
    /// (`Status validation failed: ...`).
    pub fn cli_message(&self) -> String {
        match self {
            TaskQueryError::InvalidEnum { field, message, .. } => {
                let label = match *field {
                    "status" => "Status",
                    "priority" => "Priority",
                    _ => "Task type",
                };
                format!("{label} validation failed: {message}")
            }
            other => other.to_string(),
        }
    }
}

impl From<String> for TaskQueryError {
    fn from(message: String) -> Self {
        TaskQueryError::Invalid(message)
    }
}

impl TaskQueryInput {
    /// Read a REST-style query map: CSV values, `q` for the search text,
    /// `query` in the web search syntax, and every key not in
    /// [`KNOWN_QUERY_KEYS`] as a field filter.
    pub fn from_params(query: &HashMap<String, String>) -> Result<Self, TaskQueryError> {
        let list = |key: &str| -> Vec<String> {
            query
                .get(key)
                .map(|raw| {
                    raw.split(',')
                        .map(str::trim)
                        .filter(|part| !part.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut fields = Vec::new();
        for (key, value) in query {
            if KNOWN_QUERY_KEYS.contains(&key.as_str()) {
                continue;
            }
            for part in value.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                fields.push((key.clone(), part.to_string()));
            }
        }
        // Deterministic order regardless of the map's iteration order.
        fields.sort();
        let mut input = Self {
            project: query.get("project").cloned(),
            status: list("status"),
            priority: list("priority"),
            task_type: list("type"),
            tags: list("tags"),
            text: query.get("q").cloned(),
            sprints: parse_sprints_csv(query.get("sprints").map(String::as_str))?,
            assignee: query.get("assignee").cloned(),
            fields,
            deletion: parse_deletion(query.get("deletion").map(String::as_str))?,
            due: query.get("due").cloned(),
            due_within: query.get("due_within").cloned(),
            recent: query.get("recent").cloned(),
            needs: query.get("needs").cloned(),
            has: query.get("has").cloned(),
            effort_min: query.get("effort_min").cloned(),
            effort_max: query.get("effort_max").cloned(),
            sort_by: query.get("sort_by").cloned(),
            order: query.get("order").cloned(),
            exclude: Vec::new(),
        };
        if let Some(text) = query.get("query") {
            input.add_search(&crate::utils::search_syntax::parse(text))?;
        }
        Ok(input)
    }

    /// Add a search written in the web search-box syntax (DEV-231), e.g.
    /// `status:todo,done has:tag owner:alice "login redirect"`. List filters
    /// add to what is already set; single-value filters replace it; the
    /// remaining words become the search text.
    pub fn add_search(
        &mut self,
        search: &crate::utils::search_syntax::ParsedSearch,
    ) -> Result<(), TaskQueryError> {
        if !search.errors.is_empty() {
            return Err(TaskQueryError::Invalid(search.errors.join("; ")));
        }
        let csv = |value: &str| -> Vec<String> {
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        };
        let merge = |current: &Option<String>, value: &str| match current {
            Some(existing) if !existing.trim().is_empty() => format!("{existing},{value}"),
            _ => value.to_string(),
        };
        let mut text = (!search.text.is_empty()).then(|| search.text.clone());
        for (key, value) in &search.filters {
            // `-key:value` excludes (DEV-5): one exclusion per value, so a
            // task matching any of them is left out.
            if let Some(negated) = key.strip_prefix('-') {
                for value in csv(value) {
                    let mut exclusion = TaskQueryInput::default();
                    let mut single = crate::utils::search_syntax::ParsedSearch::default();
                    single.filters.insert(negated.to_string(), value);
                    exclusion.add_search(&single)?;
                    self.exclude.push(exclusion);
                }
                continue;
            }
            match key.as_str() {
                "status" => self.status.extend(csv(value)),
                "priority" => self.priority.extend(csv(value)),
                "type" => self.task_type.extend(csv(value)),
                "tags" => self.tags.extend(csv(value)),
                "sprints" => self.sprints.extend(parse_sprints_csv(Some(value))?),
                "project" => self.project = Some(value.clone()),
                "assignee" => self.assignee = Some(value.clone()),
                "mine" => self.assignee = Some("@me".to_string()),
                "due" => self.due = Some(value.clone()),
                "due_within" => self.due_within = Some(value.clone()),
                "recent" => self.recent = Some(value.clone()),
                "needs" => self.needs = Some(merge(&self.needs, value)),
                "has" => self.has = Some(merge(&self.has, value)),
                "effort_min" => self.effort_min = Some(value.clone()),
                "effort_max" => self.effort_max = Some(value.clone()),
                "order" => self.order = Some(value.clone()),
                "sort_by" => self.sort_by = Some(value.clone()),
                "deletion" => self.deletion = parse_deletion(Some(value))?,
                // `q=` sets the search text outright, as in the web UI.
                "q" => text = Some(value.clone()),
                _ => self
                    .fields
                    .extend(csv(value).into_iter().map(|v| (key.clone(), v))),
            }
        }
        if let Some(text) = text {
            self.text = Some(match self.text.take().filter(|t| !t.is_empty()) {
                Some(existing) if !search.filters.contains_key("q") => format!("{existing} {text}"),
                _ => text,
            });
        }
        Ok(())
    }

    /// Validate the input. Enums, custom field names and `@me` resolve
    /// against the requested project's configuration (else `base`), so
    /// project-only values work. Explicit invalid values are errors; nothing
    /// is silently dropped.
    pub fn resolve(
        self,
        base: &ResolvedConfig,
        tasks_root: &Path,
    ) -> Result<TaskQuery, TaskQueryError> {
        if let Some(text) = self.text.as_deref() {
            crate::utils::text_search::SearchQuery::validate(text)?;
        }

        let project = self
            .project
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string);
        let config = match project.as_deref() {
            Some(prefix) => crate::config::resolution::config_for_project(tasks_root, Some(prefix))
                .unwrap_or_else(|_| base.clone()),
            None => base.clone(),
        };

        let mut filter = TaskListFilter {
            project,
            tags: self.tags,
            text_query: self.text.clone(),
            sprints: self.sprints,
            deletion: self.deletion,
            ..TaskListFilter::default()
        };
        for raw in &self.status {
            filter.status.push(
                crate::types::TaskStatus::parse_with_config(raw, &config)
                    .map_err(|message| invalid_enum("status", raw, message))?,
            );
        }
        for raw in &self.priority {
            filter.priority.push(
                crate::types::Priority::parse_with_config(raw, &config)
                    .map_err(|message| invalid_enum("priority", raw, message))?,
            );
        }
        for raw in &self.task_type {
            filter.task_type.push(
                crate::types::TaskType::parse_with_config(raw, &config)
                    .map_err(|message| invalid_enum("type", raw, message))?,
            );
        }

        if let Some(raw) = self.assignee.as_deref().map(str::trim) {
            if raw.eq_ignore_ascii_case("__none__") {
                filter.assignee_none = true;
            } else if !raw.is_empty() {
                let name = if raw.eq_ignore_ascii_case("@me") {
                    crate::utils::identity::resolve_current_user_for_project(
                        Some(tasks_root),
                        filter.project.as_deref(),
                    )
                    .ok_or(TaskQueryError::UnresolvedMe)?
                } else {
                    raw.to_string()
                };
                filter
                    .assignee
                    .push(crate::utils::member::normalize_member_value(&name, |n| {
                        config.agent_profiles.contains_key(n)
                    }));
            }
        }

        // Builtin fields (`reporter`, `due_date`, ...) always match the task's
        // own value, even when the configuration allows any custom field.
        // Names are normalised, so spellings of one field (`Component`,
        // `field:COMPONENT`) share one filter whose values are ORed.
        let mut field_filters: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (key, value) in self.fields {
            if let Some(builtin) = crate::utils::fields::is_reserved_field(&key) {
                field_filters
                    .entry(builtin.to_string())
                    .or_default()
                    .insert(value);
            } else if let Some(name) =
                crate::utils::custom_fields::resolve_filter_name(&key, &config)
            {
                filter
                    .custom_fields
                    .entry(crate::utils::custom_fields::canonicalize(&name))
                    .or_default()
                    .push(value);
            } else {
                field_filters.entry(key).or_default().insert(value);
            }
        }

        let mut options = TaskQueryOptions {
            search: self
                .text
                .as_deref()
                .map(crate::utils::text_search::SearchQuery::parse),
            ..TaskQueryOptions::default()
        };
        if let Some(raw) = self.sort_by.as_deref() {
            options.sort = task_query::parse_sort_by(raw)?;
        }
        if let Some(raw) = self.order.as_deref() {
            options.order = task_query::parse_order(raw)?;
        }
        // Explicitly supplied blank values are errors (strict blanks):
        // callers omit empty parameters rather than sending `due=` &c.
        if let Some(raw) = self.due.as_deref() {
            options.due = Some(task_query::parse_due(raw)?);
        }
        if let Some(raw) = self.due_within.as_deref() {
            options.due_within_days = Some(task_query::parse_due_within(raw)?);
        }
        if let Some(raw) = self.recent.as_deref() {
            options.recent = Some(task_query::parse_recent(raw)?);
        }
        if let Some(raw) = self.needs.as_deref() {
            options.needs = task_query::parse_needs(raw)?;
        }
        if let Some(raw) = self.has.as_deref() {
            options.has = task_query::parse_has(raw)?;
        }
        if let Some(raw) = self.effort_min.as_deref() {
            options.effort_min = Some(task_query::parse_effort_bound("effort_min", raw)?);
        }
        if let Some(raw) = self.effort_max.as_deref() {
            options.effort_max = Some(task_query::parse_effort_bound("effort_max", raw)?);
        }

        options.ranks = task_query::SortRanks {
            priority: config
                .issue_priorities
                .values
                .iter()
                .map(ToString::to_string)
                .collect(),
            status: config
                .issue_states
                .values
                .iter()
                .map(ToString::to_string)
                .collect(),
        };

        // Exclusions look at every task, deleted ones included, so they can
        // remove whatever the main query found.
        let exclude = self
            .exclude
            .into_iter()
            .map(|mut exclusion| {
                exclusion.deletion = DeletionFilter::All;
                exclusion.resolve(base, tasks_root)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TaskQuery {
            filter,
            field_filters,
            options,
            config,
            exclude,
        })
    }
}

impl TaskQuery {
    /// Whether the query orders by search relevance.
    pub fn sorts_by_relevance(&self) -> bool {
        self.options.sort == SortSpec::Builtin(SortKey::Relevance)
    }
}

/// Load, filter and order the tasks a query selects. Pagination is left to
/// the caller.
pub fn run(
    storage: &Storage,
    query: &TaskQuery,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<(String, TaskDTO)> {
    let mut tasks = crate::services::task_service::TaskService::list(storage, &query.filter);
    apply_field_filters(&mut tasks, &query.field_filters, &query.config);
    task_query::apply(&mut tasks, &query.options, now);
    remove_excluded(storage, query, &mut tasks, now);
    tasks
}

/// Leave out tasks matching any of the query's exclusions (DEV-5).
pub fn remove_excluded(
    storage: &Storage,
    query: &TaskQuery,
    tasks: &mut Vec<(String, TaskDTO)>,
    now: chrono::DateTime<chrono::Utc>,
) {
    if query.exclude.is_empty() {
        return;
    }
    let excluded: std::collections::HashSet<String> = query
        .exclude
        .iter()
        .flat_map(|exclusion| run(storage, exclusion, now).into_iter().map(|(id, _)| id))
        .collect();
    tasks.retain(|(id, _)| !excluded.contains(id));
}

/// IDs of the tasks a search in the web syntax selects, for commands that
/// act on a selection (sprint add/remove over REST and MCP, DEV-236).
pub fn select_ids(
    storage: &Storage,
    base: &ResolvedConfig,
    tasks_root: &Path,
    search: &str,
) -> Result<Vec<String>, TaskQueryError> {
    let mut input = TaskQueryInput::default();
    input.add_search(&crate::utils::search_syntax::parse(search))?;
    let query = input.resolve(base, tasks_root)?;
    Ok(run(storage, &query, chrono::Utc::now())
        .into_iter()
        .map(|(id, _)| id)
        .collect())
}

/// Keep tasks whose field values loosely match every field filter.
pub fn apply_field_filters(
    tasks: &mut Vec<(String, TaskDTO)>,
    filters: &BTreeMap<String, BTreeSet<String>>,
    config: &ResolvedConfig,
) {
    if filters.is_empty() {
        return;
    }
    tasks.retain(|(id, task)| {
        filters.iter().all(|(key, allowed)| {
            let values: Vec<String> =
                crate::utils::custom_fields::resolve_task_filter_values(id, task, key, config)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|value| !value.is_empty())
                    .collect();
            let allowed: Vec<String> = allowed.iter().cloned().collect();
            !values.is_empty() && crate::utils::fuzzy_match::fuzzy_set_match(&values, &allowed)
        })
    });
}

fn invalid_enum(field: &'static str, value: &str, message: String) -> TaskQueryError {
    TaskQueryError::InvalidEnum {
        field,
        value: value.trim().to_string(),
        message,
    }
}

/// Strict `deletion` value (DEV-92): blank or absent means `active`;
/// anything other than exactly `active`, `deleted` or `all` is rejected.
pub fn parse_deletion(raw: Option<&str>) -> Result<DeletionFilter, String> {
    match raw {
        None => Ok(DeletionFilter::default()),
        Some(value) if value.trim().is_empty() => Ok(DeletionFilter::default()),
        Some("active") => Ok(DeletionFilter::Active),
        Some("deleted") => Ok(DeletionFilter::Deleted),
        Some("all") => Ok(DeletionFilter::All),
        Some(other) => Err(format!(
            "Invalid deletion filter: '{other}' (expected active, deleted, or all)"
        )),
    }
}

/// Strict `sprints` CSV: every entry of a non-blank value must be a positive
/// integer. Blank means no sprint filter (DEV-57).
pub fn parse_sprints_csv(raw: Option<&str>) -> Result<Vec<u32>, String> {
    let Some(value) = raw.filter(|v| !v.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    value
        .split(',')
        .map(|part| {
            let part = part.trim();
            match part.parse::<u32>() {
                Ok(id) if id > 0 => Ok(id),
                _ => Err(format!(
                    "Invalid sprints entry: '{part}' (expected CSV of positive sprint ids)"
                )),
            }
        })
        .collect()
}

/// Totals of one field over a task list (DEV-13): effort (time and points
/// kept apart) or a numeric custom field.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FieldSum {
    /// `effort` or `field:<name>`.
    pub field: String,
    /// Custom fields: the total of the numeric values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    /// Effort: total time in hours.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hours: Option<f64>,
    /// Effort: total points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<f64>,
    /// Tasks whose value was added.
    pub counted: usize,
    /// Tasks without a value.
    pub missing: usize,
    /// Tasks whose value is not a number (or not an effort).
    pub invalid: usize,
}

/// Sum `field` (`effort`, or a custom field by name or `field:<name>`) over
/// `tasks`. Other builtin fields are not numbers and are rejected.
pub fn sum_field(
    tasks: &[(String, TaskDTO)],
    field: &str,
    config: &ResolvedConfig,
) -> Result<FieldSum, String> {
    let field = field.trim();
    if field.eq_ignore_ascii_case("effort") {
        let mut sum = FieldSum {
            field: "effort".to_string(),
            total: None,
            hours: Some(0.0),
            points: Some(0.0),
            counted: 0,
            missing: 0,
            invalid: 0,
        };
        for (_, task) in tasks {
            match task
                .effort
                .as_deref()
                .map(str::trim)
                .filter(|e| !e.is_empty())
            {
                None => sum.missing += 1,
                Some(raw) => match crate::utils::effort::parse_effort(raw).map(|e| e.kind) {
                    Ok(crate::utils::effort::EffortKind::TimeHours(h)) => {
                        sum.hours = sum.hours.map(|t| t + h);
                        sum.counted += 1;
                    }
                    Ok(crate::utils::effort::EffortKind::Points(p)) => {
                        sum.points = sum.points.map(|t| t + p);
                        sum.counted += 1;
                    }
                    Err(_) => sum.invalid += 1,
                },
            }
        }
        return Ok(sum);
    }
    let name = if crate::utils::fields::is_reserved_field(field).is_some() {
        None
    } else {
        crate::utils::custom_fields::resolve_filter_name(field, config)
    }
    .ok_or_else(|| {
        format!("Cannot sum '{field}': use effort or a numeric custom field (field:<name>)")
    })?;
    let mut sum = FieldSum {
        field: format!("field:{name}"),
        total: Some(0.0),
        hours: None,
        points: None,
        counted: 0,
        missing: 0,
        invalid: 0,
    };
    for (_, task) in tasks {
        let value = crate::utils::custom_fields::extract_value_strings(&task.custom_fields, &name)
            .and_then(|values| values.into_iter().next())
            .filter(|v| !v.trim().is_empty());
        match value.map(|v| v.trim().parse::<f64>()) {
            None => sum.missing += 1,
            Some(Ok(n)) if n.is_finite() => {
                sum.total = sum.total.map(|t| t + n);
                sum.counted += 1;
            }
            Some(_) => sum.invalid += 1,
        }
    }
    Ok(sum)
}

impl FieldSum {
    /// One line for text output, e.g. `effort: 34h + 13pt over 12 task(s) (3 without)`.
    pub fn describe(&self) -> String {
        let number = |n: f64| {
            let rounded = (n * 100.0).round() / 100.0;
            if rounded.fract() == 0.0 {
                format!("{rounded:.0}")
            } else {
                format!("{rounded}")
            }
        };
        let total = match (self.total, self.hours, self.points) {
            (Some(total), _, _) => number(total),
            (None, Some(h), Some(p)) if p > 0.0 && h > 0.0 => {
                format!("{}h + {}pt", number(h), number(p))
            }
            (None, _, Some(p)) if p > 0.0 => format!("{}pt", number(p)),
            (None, Some(h), _) => format!("{}h", number(h)),
            _ => "0".to_string(),
        };
        let mut notes = Vec::new();
        if self.missing > 0 {
            notes.push(format!("{} without a value", self.missing));
        }
        if self.invalid > 0 {
            notes.push(format!("{} not a number", self.invalid));
        }
        let field = self.field.strip_prefix("field:").unwrap_or(&self.field);
        let notes = if notes.is_empty() {
            String::new()
        } else {
            format!(" ({})", notes.join(", "))
        };
        format!(
            "Sum of {field}: {total} over {} task(s){notes}",
            self.counted
        )
    }
}
