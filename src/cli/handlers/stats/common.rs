//! Helpers shared by the `lotar stats` subcommands: project scoping, task
//! loading, the git window for history-based stats, and list output.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};

use crate::api_types::TaskDTO;
use crate::output::{OutputFormat, OutputRenderer};
use crate::storage::manager::Storage;
use crate::workspace::TasksDirectoryResolver;

pub(crate) fn custom_value_key(v: &crate::types::CustomFieldValue) -> String {
    crate::types::custom_value_to_string(v)
}

/// Project a stats query covers: `None` with `--global`, otherwise the
/// explicit project or the workspace's effective project.
pub(crate) fn scope_project(
    global: bool,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
) -> Option<String> {
    if global {
        return None;
    }
    project
        .map(|p| crate::utils::resolve_project_input(p, resolver.path.as_path()))
        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
}

/// Current task snapshot for the scope (all projects when `None`).
/// Tasks in scope that match `filter`, a query in the web search syntax,
/// run as the shared task query (DEV-238). An empty filter keeps every task.
pub(crate) fn list_tasks(
    scope_project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    filter: &str,
) -> Result<Vec<(String, TaskDTO)>, String> {
    let base = crate::config::resolution::load_and_merge_configs(Some(resolver.path.as_path()))
        .map_err(|e| format!("Failed to load config: {e}"))?;
    let mut input = crate::services::task_list::TaskQueryInput {
        project: scope_project.map(str::to_string),
        ..Default::default()
    };
    input
        .add_search(&crate::utils::search_syntax::parse(filter))
        .map_err(|e| e.cli_message())?;
    let query = input
        .resolve(&base, resolver.path.as_path())
        .map_err(|e| e.cli_message())?;
    let storage = Storage::new(&resolver.path);
    Ok(crate::services::task_list::run(
        &storage,
        &query,
        chrono::Utc::now(),
    ))
}

/// The IDs `filter` keeps, for stats that list tickets from git; `None`
/// when there is no filter.
pub(crate) fn filter_ids(
    scope_project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    filter: &str,
) -> Result<Option<std::collections::HashSet<String>>, String> {
    if filter.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(
        list_tasks(scope_project, resolver, filter)?
            .into_iter()
            .map(|(id, _)| id)
            .collect(),
    ))
}

/// Common JSON envelope fields for a stats result.
pub(crate) fn header(
    action: &str,
    global: bool,
    scope_project: Option<&str>,
) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("status".into(), json!("ok"));
    map.insert("action".into(), json!(action));
    map.insert("global".into(), json!(global));
    map.insert("project".into(), json!(scope_project));
    map
}

/// [`header`] plus the `since`/`until` window of history-based stats.
pub(crate) fn window_header(
    action: &str,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    global: bool,
    scope_project: Option<&str>,
) -> Map<String, Value> {
    let mut map = header(action, global, scope_project);
    map.insert("since".into(), json!(since.to_rfc3339()));
    map.insert("until".into(), json!(until.to_rfc3339()));
    map
}

/// Git repository context for stats computed from commit history.
pub(crate) struct GitWindow {
    pub(crate) repo_root: PathBuf,
    tasks_abs: PathBuf,
    tasks_relative: Option<PathBuf>,
}

impl GitWindow {
    /// Tasks directory relative to the repo root, or absolute when it lives
    /// outside the repository.
    pub(crate) fn tasks_rel(&self) -> PathBuf {
        self.tasks_relative
            .clone()
            .unwrap_or_else(|| self.tasks_abs.clone())
    }

    /// Like [`Self::tasks_rel`], but falls back to the directory name so the
    /// path stays repo-relative.
    pub(crate) fn tasks_rel_or_name(&self) -> PathBuf {
        self.tasks_relative.clone().unwrap_or_else(|| {
            self.tasks_abs
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| self.tasks_abs.clone())
        })
    }

    pub(crate) fn tasks_abs(&self) -> &PathBuf {
        &self.tasks_abs
    }
}

/// Locate the git repository for history-based stats. Outside a repository
/// this emits an empty result (`header` plus a note) and returns `None`.
pub(crate) fn git_window(
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
    header: Map<String, Value>,
) -> Result<Option<GitWindow>, String> {
    const NOTE: &str = "Not in a git repository; returning empty set";
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let Some(repo_root) = crate::utils::git::find_repo_root(&cwd) else {
        match renderer.format {
            OutputFormat::Json => {
                let mut obj = header;
                obj.insert("items".into(), json!([]));
                obj.insert("note".into(), json!(NOTE));
                renderer.emit_json(&Value::Object(obj));
            }
            _ => renderer.emit_warning(NOTE),
        }
        return Ok(None);
    };
    let tasks_abs = resolver.path.clone();
    let tasks_relative = crate::utils::git::repo_relative_path(&repo_root, &tasks_abs);
    Ok(Some(GitWindow {
        repo_root,
        tasks_abs,
        tasks_relative,
    }))
}

/// Counts sorted by frequency (descending), then key, limited to `limit`.
pub(crate) fn ranked_counts<N: Ord + Copy>(
    counts: impl IntoIterator<Item = (String, N)>,
    limit: usize,
) -> Vec<(String, N)> {
    let mut items: Vec<(String, N)> = counts.into_iter().collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(limit);
    items
}

/// Tally occurrences of each non-blank key.
pub(crate) fn tally<I: IntoIterator<Item = String>>(keys: I) -> HashMap<String, u64> {
    let mut counts = HashMap::new();
    for key in keys {
        if !key.trim().is_empty() {
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    counts
}

/// Emit a list result: JSON gets `header` plus `count` and `items`; text gets
/// one line per row, or `empty_message` when there are none.
pub(crate) fn emit_items<T>(
    renderer: &OutputRenderer,
    header: Map<String, Value>,
    rows: &[T],
    to_json: impl Fn(&T) -> Value,
    to_line: impl Fn(&T) -> String,
    empty_message: &str,
) {
    match renderer.format {
        OutputFormat::Json => {
            let mut obj = header;
            obj.insert("count".into(), json!(rows.len()));
            obj.insert(
                "items".into(),
                Value::Array(rows.iter().map(to_json).collect()),
            );
            renderer.emit_json(&Value::Object(obj));
        }
        _ if rows.is_empty() => renderer.emit_success(empty_message),
        _ => {
            for row in rows {
                renderer.emit_raw_stdout(format_args!("{}", to_line(row)));
            }
        }
    }
}
