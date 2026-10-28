use super::common::{
    custom_value_key, emit_items, header, list_tasks, ranked_counts, scope_project, tally,
};
use crate::output::OutputRenderer;
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
    let counts = tally(
        tasks
            .into_iter()
            .flat_map(|(_, task)| task.custom_fields.into_keys()),
    );
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
    let counts = tally(
        tasks
            .iter()
            .filter_map(|(_, task)| task.custom_fields.get(name).map(custom_value_key)),
    );
    let rows = ranked_counts(counts, limit);
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
