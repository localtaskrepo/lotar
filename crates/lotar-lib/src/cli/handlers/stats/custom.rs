use super::common::{emit_items, header, list_tasks, ranked_counts, scope_project, tally};
use crate::output::OutputRenderer;
use crate::utils::version::Version;
use crate::workspace::TasksDirectoryResolver;

pub(crate) fn run_keys(
    limit: usize,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let scope = scope_project(global, project, resolver);
    let tasks = list_tasks(scope.as_deref(), resolver, filter)?;
    let counts = tally(tasks.into_iter().flat_map(|(_, task)| task.custom_fields.into_keys()));
    let rows = ranked_counts(counts, limit);
    emit_items(
        renderer,
        header("stats.custom.keys", global, scope.as_deref()),
        &rows,
        |(key, n)| serde_json::json!({"key": key, "count": n}),
        |(key, n)| format!("{:>4}  {}", n, key),
        "No custom fields found.",
    );
    Ok(())
}

pub(crate) fn run_field(
    name: &str,
    limit: usize,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let scope = scope_project(global, project, resolver);
    let tasks = list_tasks(scope.as_deref(), resolver, filter)?;
    // A list value counts once for each item (DEV-391).
    let counts = tally(tasks.iter().filter_map(|(_, task)| task.custom_fields.get(name)).flat_map(crate::types::custom_value_items));
    let config = crate::config::resolution::load_and_merge_configs(Some(resolver.path.as_path())).map_err(|e| format!("Failed to load config: {e}"))?;
    let rows = if crate::utils::custom_fields::version_field_name(name, &config).is_some() {
        newest_versions_first(counts, limit)
    } else {
        ranked_counts(counts, limit)
    };
    let mut head = header("stats.custom.field", global, scope.as_deref());
    head.insert("field".into(), serde_json::json!(name));
    emit_items(
        renderer,
        head,
        &rows,
        |(value, n)| serde_json::json!({"value": value, "count": n}),
        |(value, n)| format!("{:>4}  {}", n, value),
        "No values found for field.",
    );
    Ok(())
}

/// Values of a version field, newest version first; values that don't read
/// as versions follow by count.
fn newest_versions_first(counts: impl IntoIterator<Item = (String, u64)>, limit: usize) -> Vec<(String, u64)> {
    let (versions, others): (Vec<_>, Vec<_>) = counts.into_iter().partition(|(value, _)| Version::parse(value).is_ok());
    let mut versions: Vec<(Version, String, u64)> =
        versions.into_iter().filter_map(|(value, n)| Version::parse(&value).ok().map(|version| (version, value, n))).collect();
    versions.sort_by(|a, b| b.0.sort_cmp(&a.0));
    let mut rows: Vec<(String, u64)> = versions.into_iter().map(|(_, value, n)| (value, n)).collect();
    rows.extend(ranked_counts(others, usize::MAX));
    rows.truncate(limit);
    rows
}
