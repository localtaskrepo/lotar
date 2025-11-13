use crate::cli::args::task::RelationshipKind;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::output::{OutputFormat, OutputRenderer};
use crate::types::TaskRelationships;
use crate::workspace::TasksDirectoryResolver;
use serde_json::{Map as JsonMap, Value as JsonValue};

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
                        renderer.emit_raw_stdout(&format!("  {}: {}", kind.as_kebab(), val));
                    }
                    RelationshipValue::Many(list) => {
                        renderer.emit_raw_stdout(&format!("  {}:", kind.as_kebab()));
                        for item in list {
                            renderer.emit_raw_stdout(&format!("    - {}", item));
                        }
                    }
                }
            }
        }
    }
}
