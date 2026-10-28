use std::collections::HashMap;

use super::common::{emit_items, header, list_tasks, ranked_counts, scope_project};
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

struct CommentRanking {
    scope: Option<String>,
    rows: Vec<(String, usize)>,
    titles: HashMap<String, String>,
}

/// Tasks in scope ranked by comment count.
fn rank_by_comments(
    global: bool,
    project: Option<&str>,
    limit: usize,
    filter: &str,
    resolver: &TasksDirectoryResolver,
) -> Result<CommentRanking, String> {
    let scope = scope_project(global, project, resolver);
    let tasks = list_tasks(scope.as_deref(), resolver, filter)?;
    let rows = ranked_counts(
        tasks
            .iter()
            .map(|(id, task)| (id.clone(), task.comments.len())),
        limit,
    );
    let titles = tasks
        .into_iter()
        .map(|(id, task)| (id, task.title))
        .collect();
    Ok(CommentRanking {
        scope,
        rows,
        titles,
    })
}

pub(crate) fn run_top(
    limit: usize,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let ranking = rank_by_comments(global, project, limit, filter, resolver)?;
    emit_items(
        renderer,
        header("stats.comments.top", global, ranking.scope.as_deref()),
        &ranking.rows,
        |(id, n)| serde_json::json!({"id": id, "title": ranking.titles.get(id), "comments": n}),
        |(id, n)| format!("{:>4}  {}", n, id),
        "No comments found.",
    );
    Ok(())
}

/// Comments carry no author, so this ranks tasks by comment count.
pub(crate) fn run_by_author(
    limit: usize,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let ranking = rank_by_comments(global, project, limit, filter, resolver)?;
    emit_items(
        renderer,
        header("stats.comments.by_task", global, ranking.scope.as_deref()),
        &ranking.rows,
        |(id, n)| serde_json::json!({"task": id, "comments": n}),
        |(id, n)| format!("{:>4}  {}", n, id),
        "No comments found.",
    );
    Ok(())
}
