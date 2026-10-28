#[expect(
    clippy::needless_pass_by_value,
    reason = "parsed CLI arguments are moved in from the command"
)]
pub(crate) fn run(
    field: crate::cli::args::stats::StatsDistributionField,
    limit: usize,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    let scope_project = super::common::scope_project(global, project, resolver);
    let tasks = super::common::list_tasks(scope_project.as_deref(), resolver, filter)?;

    use std::collections::HashMap;
    let mut freq: HashMap<String, usize> = HashMap::new();
    for (_id, t) in tasks.into_iter() {
        match field {
            crate::cli::args::stats::StatsDistributionField::Status => {
                *freq.entry(t.status.to_string()).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Priority => {
                *freq.entry(t.priority.to_string()).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Type => {
                *freq.entry(t.task_type.to_string()).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Assignee => {
                let key = t.assignee.unwrap_or_default();
                *freq.entry(key).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Reporter => {
                let key = t.reporter.unwrap_or_default();
                *freq.entry(key).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Project => {
                // Group by project prefix embedded in ID
                let proj = crate::storage::TaskId::parse(&t.id)
                    .map(|parsed| parsed.project)
                    .unwrap_or_default();
                *freq.entry(proj).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Tag => {
                for tag in t.tags.into_iter() {
                    if tag.trim().is_empty() {
                        continue;
                    }
                    *freq.entry(tag).or_insert(0) += 1;
                }
            }
        }
    }

    let limited =
        super::common::ranked_counts(freq.into_iter().filter(|(k, _)| !k.is_empty()), limit);

    let field_name = match field {
        crate::cli::args::stats::StatsDistributionField::Status => "status",
        crate::cli::args::stats::StatsDistributionField::Priority => "priority",
        crate::cli::args::stats::StatsDistributionField::Type => "type",
        crate::cli::args::stats::StatsDistributionField::Assignee => "assignee",
        crate::cli::args::stats::StatsDistributionField::Reporter => "reporter",
        crate::cli::args::stats::StatsDistributionField::Project => "project",
        crate::cli::args::stats::StatsDistributionField::Tag => "tag",
    };

    let mut header = super::common::header("stats.distribution", global, scope_project.as_deref());
    header.insert("field".into(), serde_json::json!(field_name));
    super::common::emit_items(
        renderer,
        header,
        &limited,
        |(key, count)| serde_json::json!({"key": key, "count": count}),
        |(key, count)| format!("{:>6}  {}", count, key),
        "No items found for the selected field.",
    );
    Ok(())
}
