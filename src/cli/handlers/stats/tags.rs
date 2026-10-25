use super::common::{emit_items, header, list_tasks, ranked_counts, scope_project, tally};
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

pub(crate) fn run(
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let scope = scope_project(global, project, resolver);
    let tasks = list_tasks(scope.as_deref(), resolver);
    let rows = ranked_counts(
        tally(tasks.into_iter().flat_map(|(_, task)| task.tags)),
        limit,
    );
    emit_items(
        renderer,
        header("stats.tags", global, scope.as_deref()),
        &rows,
        |(tag, count)| serde_json::json!({"tag": tag, "count": count}),
        |(tag, count)| format!("{:>6}  {}", count, tag),
        "No tags found.",
    );
    Ok(())
}
