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
fn rank_by_comments(global: bool, project: Option<&str>, limit: usize, filter: &str, resolver: &TasksDirectoryResolver) -> Result<CommentRanking, String> {
    let scope = scope_project(global, project, resolver);
    let tasks = list_tasks(scope.as_deref(), resolver, filter)?;
    let rows = ranked_counts(tasks.iter().map(|(id, task)| (id.clone(), task.comments.len())), limit);
    let titles = tasks.into_iter().map(|(id, task)| (id, task.title)).collect();
    Ok(CommentRanking { scope, rows, titles })
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

/// Comments in scope counted per author; comments from before DEV-327
/// carry no author and count together as `null`.
pub(crate) fn run_by_author(
    limit: usize,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let scope = scope_project(global, project, resolver);
    let tasks = list_tasks(scope.as_deref(), resolver, filter)?;
    let mut counts: HashMap<Option<String>, usize> = HashMap::new();
    for comment in tasks.into_iter().flat_map(|(_, task)| task.comments) {
        *counts.entry(comment.author).or_insert(0) += 1;
    }
    let mut rows: Vec<(Option<String>, usize)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    rows.truncate(limit);
    emit_items(
        renderer,
        header("stats.comments.by_author", global, scope.as_deref()),
        &rows,
        |(author, n)| serde_json::json!({"author": author, "comments": n}),
        |(author, n)| format!("{:>4}  {}", n, author.as_deref().unwrap_or("(no author)")),
        "No comments found.",
    );
    Ok(())
}
