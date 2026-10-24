use crate::cli::args::task::RelationshipKind;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::output::{OutputFormat, OutputRenderer};
use crate::types::TaskRelationships;
use crate::workspace::TasksDirectoryResolver;
use serde_json::{Map as JsonMap, Value as JsonValue};

pub fn execute_action(
    action: crate::cli::args::task::TaskRelationshipsAction,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    use crate::api_types::{
        RelationshipGraphRequest, RelationshipMutationRequest, RelationshipOperation,
        RelationshipRepairRequest,
    };
    use crate::cli::args::task::TaskRelationshipsAction as Action;
    use crate::services::relationships;

    let storage = crate::storage::manager::Storage::try_open(&resolver.path)
        .ok_or("Relationship snapshot unavailable: tasks directory does not exist")?;
    let project =
        project.map(|value| crate::utils::project::resolve_project_input(value, &resolver.path));
    match action {
        Action::Check => {
            let report =
                relationships::check(&storage, project.as_deref()).map_err(|e| e.to_string())?;
            let blocking = !report.complete || report.issues.iter().any(|i| i.severity == "error");
            render_report(renderer, &report)?;
            if blocking {
                return Err("Relationship check found blocking findings or incomplete data".into());
            }
        }
        Action::Graph {
            root,
            depth,
            max_nodes,
            relationship_kinds,
            include_isolated,
        } => {
            let root = root
                .map(|id| resolve_id(&id, project.as_deref(), resolver))
                .transpose()?;
            let report = relationships::graph(
                &storage,
                &RelationshipGraphRequest {
                    project,
                    task_id: root,
                    depth: Some(depth),
                    max_nodes: Some(max_nodes),
                    filters: Default::default(),
                    relationship_kinds: if relationship_kinds.is_empty() {
                        None
                    } else {
                        Some(
                            relationship_kinds
                                .into_iter()
                                .map(|kind| {
                                    serde_json::from_value(JsonValue::String(kind))
                                        .map_err(|e| e.to_string())
                                })
                                .collect::<Result<_, _>>()?,
                        )
                    },
                    include_isolated,
                },
            )
            .map_err(|e| e.to_string())?;
            let incomplete = !report.complete
                || report.truncated
                || report.nodes.iter().any(|node| {
                    matches!(node.state.as_str(), "missing" | "unreadable" | "ambiguous")
                });
            render_report(renderer, &report)?;
            if incomplete {
                return Err("Relationship graph is incomplete or truncated".into());
            }
        }
        Action::Repair {
            apply,
            dry_run: _,
            expected_revision,
            issue_ids,
        } => {
            let report = relationships::repair(
                &storage,
                &RelationshipRepairRequest {
                    project,
                    apply,
                    revision: expected_revision,
                    issue_ids,
                },
            )
            .map_err(|e| e.to_string())?;
            render_report(renderer, &report)?;
        }
        action => {
            let (id, target, kind, operation, from_kind, expected) = match action {
                Action::Add {
                    id,
                    target,
                    kind,
                    expected_current,
                } => (
                    id,
                    target,
                    kind,
                    RelationshipOperation::Add,
                    None,
                    expected_current,
                ),
                Action::Remove { id, target, kind } => {
                    (id, target, kind, RelationshipOperation::Remove, None, None)
                }
                Action::Move {
                    id,
                    target,
                    from,
                    to,
                    expected_current,
                } => (
                    id,
                    target,
                    to,
                    RelationshipOperation::ChangeKind,
                    Some(from),
                    expected_current,
                ),
                _ => unreachable!(),
            };
            let source = resolve_id(&id, project.as_deref(), resolver)?;
            let source_project = crate::storage::TaskId::parse(&source)
                .map_err(|e| e.to_string())?
                .project;
            let target = resolve_id(&target, Some(&source_project), resolver)?;
            let expected_current = expected
                .map(|id| resolve_id(&id, Some(&source_project), resolver))
                .transpose()?;
            let report = relationships::mutate(
                &storage,
                &RelationshipMutationRequest {
                    source,
                    target,
                    kind: relationship_field(kind),
                    operation,
                    from_kind: from_kind.map(relationship_field),
                    expected_current,
                },
            )
            .map_err(|e| e.to_string())?;
            render_report(renderer, &report)?;
        }
    }
    Ok(())
}

fn resolve_id(
    raw: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
) -> Result<String, String> {
    let mut projects = crate::cli::project::ProjectResolver::new(resolver)?;
    projects.validate_task_id_format(raw)?;
    let full = projects.get_full_task_id(raw, project)?;
    crate::storage::TaskId::parse(&full)
        .map(|id| id.canonical())
        .map_err(|e| e.to_string())
}

fn relationship_field(kind: RelationshipKind) -> crate::api_types::RelationshipField {
    use crate::api_types::RelationshipField as Api;
    use crate::cli::args::task::RelationshipKind as Cli;
    match kind {
        Cli::DependsOn => Api::DependsOn,
        Cli::Blocks => Api::Blocks,
        Cli::Related => Api::Related,
        Cli::Fixes => Api::Fixes,
        Cli::Parent => Api::Parent,
        Cli::Children => Api::Children,
        Cli::DuplicateOf => Api::DuplicateOf,
    }
}

fn render_report(renderer: &OutputRenderer, report: &impl serde::Serialize) -> Result<(), String> {
    match renderer.format {
        OutputFormat::Json => renderer.emit_json(report),
        OutputFormat::Text => {
            let report = serde_json::to_value(report).map_err(|e| e.to_string())?;
            for key in ["revision", "complete", "truncated", "applied", "changed"] {
                if let Some(value) = report.get(key) {
                    let text = value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string());
                    renderer.emit_raw_stdout(format_args!("{key}: {text}"));
                }
            }
            for key in ["changed_tasks", "nodes", "edges", "issues"] {
                if let Some(entries) = report.get(key).and_then(JsonValue::as_array) {
                    renderer.emit_raw_stdout(format_args!("{key}: {}", entries.len()));
                    for entry in entries {
                        let text = if key == "issues" {
                            format!(
                                "{} [{}]: {}",
                                entry["severity"].as_str().unwrap_or_default(),
                                entry["code"].as_str().unwrap_or_default(),
                                entry["message"].as_str().unwrap_or_default()
                            )
                        } else if key == "edges" {
                            format!(
                                "{} --{}--> {}",
                                entry["source"].as_str().unwrap_or_default(),
                                entry["kind"].as_str().unwrap_or_default(),
                                entry["target"].as_str().unwrap_or_default()
                            )
                        } else if key == "nodes" {
                            format!(
                                "{} [{}] {}",
                                entry["id"].as_str().unwrap_or_default(),
                                entry["state"].as_str().unwrap_or_default(),
                                entry["title"].as_str().unwrap_or_default()
                            )
                        } else {
                            entry.as_str().unwrap_or_default().to_string()
                        };
                        renderer.emit_raw_stdout(format_args!("  {text}"));
                    }
                }
            }
        }
    }
    Ok(())
}

pub struct RelationshipsHandler;

pub struct RelationshipsArgs {
    pub task_id: String,
    pub kinds: Vec<RelationshipKind>,
    pub explicit_project: Option<String>,
}

impl CommandHandler for RelationshipsHandler {
    type Args = RelationshipsArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let project_hint = args.explicit_project.as_deref().or(project);

        let mut ctx =
            TaskCommandContext::new_read_only(resolver, project_hint, Some(&args.task_id))?;
        let LoadedTask {
            full_id,
            project_prefix,
            task,
        } = load_task(&mut ctx, &args.task_id, project_hint)?;

        let filtered = filter_relationships(&task.relationships, &args.kinds);
        render_relationships(renderer, &full_id, &project_prefix, &args.kinds, &filtered);
        Ok(())
    }
}

fn should_include(kinds: &[RelationshipKind], candidate: RelationshipKind) -> bool {
    kinds.is_empty() || kinds.contains(&candidate)
}

fn filter_relationships(
    relationships: &TaskRelationships,
    kinds: &[RelationshipKind],
) -> TaskRelationships {
    let mut filtered = relationships.clone();

    if !should_include(kinds, RelationshipKind::DependsOn) {
        filtered.depends_on.clear();
    }
    if !should_include(kinds, RelationshipKind::Blocks) {
        filtered.blocks.clear();
    }
    if !should_include(kinds, RelationshipKind::Related) {
        filtered.related.clear();
    }
    if !should_include(kinds, RelationshipKind::Parent) {
        filtered.parent = None;
    }
    if !should_include(kinds, RelationshipKind::Children) {
        filtered.children.clear();
    }
    if !should_include(kinds, RelationshipKind::Fixes) {
        filtered.fixes.clear();
    }
    if !should_include(kinds, RelationshipKind::DuplicateOf) {
        filtered.duplicate_of = None;
    }

    filtered.depends_on.sort();
    filtered.blocks.sort();
    filtered.related.sort();
    filtered.children.sort();
    filtered.fixes.sort();

    filtered
}

enum RelationshipValue {
    Single(String),
    Many(Vec<String>),
}

fn collect_entries(source: &TaskRelationships) -> Vec<(RelationshipKind, RelationshipValue)> {
    let mut entries = Vec::new();
    const ORDER: &[RelationshipKind] = &[
        RelationshipKind::Parent,
        RelationshipKind::Children,
        RelationshipKind::DependsOn,
        RelationshipKind::Blocks,
        RelationshipKind::Related,
        RelationshipKind::Fixes,
        RelationshipKind::DuplicateOf,
    ];

    for kind in ORDER {
        match kind {
            RelationshipKind::Parent => {
                if let Some(value) = &source.parent {
                    entries.push((
                        RelationshipKind::Parent,
                        RelationshipValue::Single(value.clone()),
                    ));
                }
            }
            RelationshipKind::Children => {
                if !source.children.is_empty() {
                    entries.push((
                        RelationshipKind::Children,
                        RelationshipValue::Many(source.children.clone()),
                    ));
                }
            }
            RelationshipKind::DependsOn => {
                if !source.depends_on.is_empty() {
                    entries.push((
                        RelationshipKind::DependsOn,
                        RelationshipValue::Many(source.depends_on.clone()),
                    ));
                }
            }
            RelationshipKind::Blocks => {
                if !source.blocks.is_empty() {
                    entries.push((
                        RelationshipKind::Blocks,
                        RelationshipValue::Many(source.blocks.clone()),
                    ));
                }
            }
            RelationshipKind::Related => {
                if !source.related.is_empty() {
                    entries.push((
                        RelationshipKind::Related,
                        RelationshipValue::Many(source.related.clone()),
                    ));
                }
            }
            RelationshipKind::Fixes => {
                if !source.fixes.is_empty() {
                    entries.push((
                        RelationshipKind::Fixes,
                        RelationshipValue::Many(source.fixes.clone()),
                    ));
                }
            }
            RelationshipKind::DuplicateOf => {
                if let Some(value) = &source.duplicate_of {
                    entries.push((
                        RelationshipKind::DuplicateOf,
                        RelationshipValue::Single(value.clone()),
                    ));
                }
            }
        }
    }

    entries
}

fn render_relationships(
    renderer: &OutputRenderer,
    task_id: &str,
    project: &str,
    kinds: &[RelationshipKind],
    relationships: &TaskRelationships,
) {
    let entries = collect_entries(relationships);

    let message = if entries.is_empty() {
        if kinds.is_empty() {
            format!("Task {} has no relationships recorded", task_id)
        } else {
            let requested = kinds
                .iter()
                .map(|kind| kind.as_kebab())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Task {} has no relationships matching {}",
                task_id, requested
            )
        }
    } else {
        format!("Task {} relationships", task_id)
    };

    match renderer.format {
        OutputFormat::Json => {
            let mut root = JsonMap::new();
            root.insert(
                "status".to_string(),
                JsonValue::String("success".to_string()),
            );
            root.insert("message".to_string(), JsonValue::String(message));
            root.insert(
                "task_id".to_string(),
                JsonValue::String(task_id.to_string()),
            );
            if !project.trim().is_empty() {
                root.insert(
                    "project".to_string(),
                    JsonValue::String(project.to_string()),
                );
            }
            if !kinds.is_empty() {
                let kinds_values = kinds
                    .iter()
                    .map(|kind| JsonValue::String(kind.as_kebab().to_string()))
                    .collect();
                let mut filters = JsonMap::new();
                filters.insert("kinds".to_string(), JsonValue::Array(kinds_values));
                root.insert("filters".to_string(), JsonValue::Object(filters));
            }

            let mut rel_map = JsonMap::new();
            for (kind, value) in entries {
                match value {
                    RelationshipValue::Single(val) => {
                        rel_map.insert(kind.as_snake().to_string(), JsonValue::String(val));
                    }
                    RelationshipValue::Many(list) => {
                        rel_map.insert(
                            kind.as_snake().to_string(),
                            JsonValue::Array(
                                list.into_iter().map(JsonValue::String).collect::<Vec<_>>(),
                            ),
                        );
                    }
                }
            }
            root.insert("relationships".to_string(), JsonValue::Object(rel_map));

            let payload = JsonValue::Object(root);
            renderer.emit_json(&payload);
        }
        OutputFormat::Text => {
            if entries.is_empty() {
                renderer.emit_success(&message);
                return;
            }
            renderer.emit_success(&message);
            for (kind, value) in entries {
                match value {
                    RelationshipValue::Single(val) => {
                        renderer.emit_raw_stdout(format_args!("  {}: {}", kind.as_kebab(), val));
                    }
                    RelationshipValue::Many(list) => {
                        renderer.emit_raw_stdout(format_args!("  {}:", kind.as_kebab()));
                        for item in list {
                            renderer.emit_raw_stdout(format_args!("    - {}", item));
                        }
                    }
                }
            }
        }
    }
}
