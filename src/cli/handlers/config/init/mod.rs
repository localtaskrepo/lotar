//! Orchestrator for `lotar init` / `lotar config init`.

mod builder;
mod scaffolds;
mod wizard;

use std::fs;
use std::path::{Path, PathBuf};

use serde_yaml::Value as Yaml;

use super::ConfigHandler;
use crate::config::normalization::{to_canonical_global_yaml, to_canonical_project_yaml};
use crate::config::types::{GlobalConfig, ProjectConfig};
use crate::output::OutputRenderer;
use crate::types::{Priority, TaskStatus, TaskType};
use crate::utils::project::{
    generate_project_prefix, generate_unique_project_prefix, validate_explicit_prefix,
};
use crate::workspace::TasksDirectoryResolver;

use builder::{InitOverrides, WorkflowPreset};
use scaffolds::{AgentsScaffold, AutomationScaffold, ScaffoldPlan, SyncRemote};
use wizard::{WizardInputs, WizardOutcome};

impl ConfigHandler {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn handle_config_init(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        args: &crate::cli::ConfigInitArgs,
    ) -> Result<(), String> {
        let tasks_root = resolver.path.clone();

        // Resolve workflow / legacy template alias → workflow + implicit scaffolds.
        let raw_workflow = args
            .workflow
            .as_deref()
            .or(args.template.as_deref())
            .unwrap_or("default")
            .to_string();
        let (workflow, implicit_scaffolds) = resolve_template_alias(&raw_workflow)?;

        // Parse --with.
        let mut scaffolds = ScaffoldPlan::parse(&args.with)?;
        merge_scaffolds(&mut scaffolds, implicit_scaffolds);

        // Build overrides from flags.
        let mut overrides = parse_overrides(args)?;

        // Global init: minimal prompts, write global config only.
        if args.global {
            return init_global(
                &tasks_root,
                renderer,
                args,
                workflow,
                &mut overrides,
                &scaffolds,
            );
        }

        // Project init: figure out project name + prefix.
        let detected = crate::project::detect_project_name();
        let mut inputs = WizardInputs {
            project_name: args.project.clone(),
            prefix: args.prefix.clone().map(|p| p.to_ascii_uppercase()),
            workflow: Some(workflow),
            overrides: overrides.clone(),
            scaffolds: scaffolds.clone(),
        };

        let outcome =
            if wizard::should_prompt(args.yes, args.dry_run) && args.project.is_none() && !args.yes
            {
                wizard::run(inputs, detected.as_deref(), renderer)?
            } else {
                // Non-interactive path: fill defaults where needed.
                if inputs.project_name.is_none() {
                    inputs.project_name = detected.clone();
                }
                WizardOutcome {
                    project_name: inputs
                        .project_name
                        .clone()
                        .unwrap_or_else(|| "default".to_string()),
                    prefix: inputs.prefix.clone(),
                    workflow: inputs.workflow.unwrap_or(WorkflowPreset::Default),
                    overrides: inputs.overrides,
                    scaffolds: inputs.scaffolds,
                }
            };

        overrides = outcome.overrides;
        scaffolds = outcome.scaffolds;

        // Resolve final prefix.
        let project_name = outcome.project_name.clone();
        let prefix = match outcome.prefix.clone() {
            Some(p) => {
                // With --force we skip the collision check so an existing project
                // can be reinitialized in place.
                if !args.force {
                    validate_explicit_prefix(&p, &project_name, &tasks_root)?;
                }
                p
            }
            None => {
                if args.force {
                    generate_project_prefix(&project_name)
                } else {
                    generate_unique_project_prefix(&project_name, &tasks_root)?
                }
            }
        };

        let project_config_path = crate::utils::paths::project_config_path(&tasks_root, &prefix);
        let global_config_path = crate::utils::paths::global_config_path(&tasks_root);

        if args.dry_run {
            renderer.emit_info(format_args!(
                "DRY RUN — workflow '{}', prefix '{}'",
                outcome.workflow.label(),
                prefix
            ));
            renderer.emit_raw_stdout(format_args!(
                "  • Project config: {}",
                project_config_path.display()
            ));
            if !global_config_path.exists() {
                renderer.emit_raw_stdout(format_args!(
                    "  • Global config: {} (new)",
                    global_config_path.display()
                ));
            } else {
                renderer.emit_raw_stdout(format_args!(
                    "  • Global config: {} (ensuring default.project={})",
                    global_config_path.display(),
                    prefix
                ));
            }
            render_scaffold_plan(&tasks_root, Some(&prefix), &scaffolds, renderer);
            renderer.emit_success("Dry run completed. Re-run without --dry-run to apply.");
            return Ok(());
        }

        // Build and write project config.
        let mut project_config =
            builder::build_project_config(&project_name, outcome.workflow, &overrides);

        // --copy-from: merge non-identity fields from the source project.
        if let Some(source_prefix) = args.copy_from.as_deref() {
            copy_from_project(&tasks_root, source_prefix, &mut project_config)?;
        }

        write_project_config(
            &tasks_root,
            &prefix,
            &project_name,
            &project_config,
            args.force,
            renderer,
        )?;

        // Always ensure a global config exists with default.project set.
        ensure_global_config_has_default(&tasks_root, &prefix, renderer)?;

        // Scaffolds (project-scoped).
        scaffolds::apply(&tasks_root, Some(&prefix), &scaffolds, args.force, renderer)?;

        // Invalidate caches so subsequent commands see the fresh config.
        crate::config::resolution::invalidate_config_cache_for(Some(&tasks_root));

        Ok(())
    }
}

fn init_global(
    tasks_root: &Path,
    renderer: &OutputRenderer,
    args: &crate::cli::ConfigInitArgs,
    workflow: WorkflowPreset,
    overrides: &mut InitOverrides,
    scaffolds: &ScaffoldPlan,
) -> Result<(), String> {
    let global_path = crate::utils::paths::global_config_path(tasks_root);
    let existing = load_existing_global(&global_path);

    if args.dry_run {
        renderer.emit_info(format_args!(
            "DRY RUN — global workflow '{}'",
            workflow.label()
        ));
        renderer.emit_raw_stdout(format_args!("  • Global config: {}", global_path.display()));
        render_scaffold_plan(tasks_root, None, scaffolds, renderer);
        renderer.emit_success("Dry run completed. Re-run without --dry-run to apply.");
        return Ok(());
    }

    if global_path.exists() && !args.force {
        return Err(format!(
            "Global config already exists at {}. Use --force to overwrite.",
            global_path.display()
        ));
    }

    let cfg = builder::build_global_config(existing, workflow, overrides);
    fs::create_dir_all(tasks_root).map_err(|e| format!("Failed to create tasks dir: {}", e))?;
    let yaml = to_canonical_global_yaml(&cfg);
    fs::write(&global_path, yaml).map_err(|e| format!("Failed to write global config: {}", e))?;
    renderer.emit_success(format_args!(
        "Global configuration initialized at: {}",
        global_path.display()
    ));

    scaffolds::apply(tasks_root, None, scaffolds, args.force, renderer)?;
    crate::config::resolution::invalidate_config_cache_for(Some(tasks_root));
    Ok(())
}

fn write_project_config(
    tasks_root: &Path,
    prefix: &str,
    project_name: &str,
    cfg: &ProjectConfig,
    force: bool,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let path = crate::utils::paths::project_config_path(tasks_root, prefix);
    if path.exists() && !force {
        return Err(format!(
            "Project config already exists at {}. Use --force to overwrite.",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create project dir {}: {}", parent.display(), e))?;
    }
    // Validate before writing.
    let validator = crate::config::validation::ConfigValidator::new(tasks_root);
    let result = validator.validate_project_config(cfg);
    for w in &result.warnings {
        renderer.emit_warning(w.to_string());
    }
    if result.has_errors() {
        for e in &result.errors {
            renderer.emit_error(e.to_string());
        }
        return Err("Generated project configuration failed validation".to_string());
    }

    let yaml = to_canonical_project_yaml(cfg);
    fs::write(&path, yaml)
        .map_err(|e| format!("Failed to write project config {}: {}", path.display(), e))?;
    renderer.emit_success(format_args!(
        "Project '{}' initialized at: {}",
        project_name,
        path.display()
    ));
    Ok(())
}

fn ensure_global_config_has_default(
    tasks_root: &Path,
    prefix: &str,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let path = crate::utils::paths::global_config_path(tasks_root);
    if path.exists() {
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        match crate::config::normalization::parse_global_from_yaml_str(&text) {
            Ok(mut cfg) => {
                if cfg.default_project.is_empty() {
                    cfg.default_project = prefix.to_string();
                    fs::write(&path, to_canonical_global_yaml(&cfg))
                        .map_err(|e| format!("Failed to update global config: {}", e))?;
                    renderer.emit_info(format_args!(
                        "Set default_project to '{}' in {}",
                        prefix,
                        path.display()
                    ));
                }
            }
            Err(e) => {
                renderer.emit_warning(format_args!(
                    "Could not parse existing global config at {} ({}); leaving untouched",
                    path.display(),
                    e
                ));
            }
        }
        return Ok(());
    }

    fs::create_dir_all(tasks_root).map_err(|e| format!("Failed to create tasks dir: {}", e))?;
    let cfg = GlobalConfig {
        default_project: prefix.to_string(),
        ..GlobalConfig::default()
    };
    fs::write(&path, to_canonical_global_yaml(&cfg))
        .map_err(|e| format!("Failed to write global config: {}", e))?;
    renderer.emit_success(format_args!(
        "Global configuration created at: {} (default_project={})",
        path.display(),
        prefix
    ));
    Ok(())
}

fn load_existing_global(path: &Path) -> Option<GlobalConfig> {
    if !path.exists() {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    crate::config::normalization::parse_global_from_yaml_str(&text).ok()
}

fn render_scaffold_plan(
    tasks_root: &Path,
    prefix: Option<&str>,
    plan: &ScaffoldPlan,
    renderer: &OutputRenderer,
) {
    if plan.is_empty() {
        return;
    }
    if !matches!(plan.automation, AutomationScaffold::None) {
        let p: PathBuf = match prefix {
            Some(p) => crate::utils::paths::project_automation_path(tasks_root, p),
            None => crate::utils::paths::global_automation_path(tasks_root),
        };
        renderer.emit_raw_stdout(format_args!("  • Automation scaffold: {}", p.display()));
    }
    if !matches!(plan.agents, AgentsScaffold::None) {
        let p: PathBuf = match prefix {
            Some(p) => crate::utils::paths::project_dir(tasks_root, p).join("agents.yml"),
            None => tasks_root.join("agents.yml"),
        };
        renderer.emit_raw_stdout(format_args!("  • Agents scaffold: {}", p.display()));
    }
    for r in &plan.sync_remotes {
        let label = match r {
            SyncRemote::Jira => "jira",
            SyncRemote::GitHub => "github",
        };
        renderer.emit_raw_stdout(format_args!("  • Sync scaffold (commented): {}", label));
    }
}

fn parse_overrides(args: &crate::cli::ConfigInitArgs) -> Result<InitOverrides, String> {
    let mut o = InitOverrides {
        default_assignee: args.default_assignee.clone(),
        default_reporter: args.default_reporter.clone(),
        ..InitOverrides::default()
    };
    if let Some(p) = &args.default_priority {
        o.default_priority = Some(p.parse().map_err(|e| format!("Invalid priority: {}", e))?);
    }
    if let Some(s) = &args.default_status {
        o.default_status = Some(s.parse().map_err(|e| format!("Invalid status: {}", e))?);
    }
    if !args.states.is_empty() {
        let mut v = Vec::with_capacity(args.states.len());
        for s in &args.states {
            v.push(
                s.parse::<TaskStatus>()
                    .map_err(|e| format!("Invalid status '{}': {}", s, e))?,
            );
        }
        o.states = Some(v);
    }
    if !args.types.is_empty() {
        let mut v = Vec::with_capacity(args.types.len());
        for s in &args.types {
            v.push(
                s.parse::<TaskType>()
                    .map_err(|e| format!("Invalid type '{}': {}", s, e))?,
            );
        }
        o.types = Some(v);
    }
    if !args.priorities.is_empty() {
        let mut v = Vec::with_capacity(args.priorities.len());
        for s in &args.priorities {
            v.push(
                s.parse::<Priority>()
                    .map_err(|e| format!("Invalid priority '{}': {}", s, e))?,
            );
        }
        o.priorities = Some(v);
    }
    if !args.tags.is_empty() {
        o.tags = Some(args.tags.clone());
    }
    Ok(o)
}

/// Translate a raw workflow / template name into a workflow preset plus any
/// implicit scaffolds (agent-pipeline etc.).
fn resolve_template_alias(raw: &str) -> Result<(WorkflowPreset, ScaffoldPlan), String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "default" | "" | "agile" | "kanban" => Ok((
            WorkflowPreset::parse(&lower).unwrap_or(WorkflowPreset::Default),
            ScaffoldPlan::default(),
        )),
        "agent-pipeline" => Ok((
            WorkflowPreset::Default,
            ScaffoldPlan {
                automation: AutomationScaffold::Pipeline,
                agents: AgentsScaffold::Pipeline,
                sync_remotes: vec![],
            },
        )),
        "agent-reviewed" => Ok((
            WorkflowPreset::Default,
            ScaffoldPlan {
                automation: AutomationScaffold::Reviewed,
                agents: AgentsScaffold::Reviewed,
                sync_remotes: vec![],
            },
        )),
        "jira" => Ok((
            WorkflowPreset::Default,
            ScaffoldPlan {
                automation: AutomationScaffold::None,
                agents: AgentsScaffold::None,
                sync_remotes: vec![SyncRemote::Jira],
            },
        )),
        "github" => Ok((
            WorkflowPreset::Default,
            ScaffoldPlan {
                automation: AutomationScaffold::None,
                agents: AgentsScaffold::None,
                sync_remotes: vec![SyncRemote::GitHub],
            },
        )),
        "jira-github" => Ok((
            WorkflowPreset::Default,
            ScaffoldPlan {
                automation: AutomationScaffold::None,
                agents: AgentsScaffold::None,
                sync_remotes: vec![SyncRemote::Jira, SyncRemote::GitHub],
            },
        )),
        other => Err(format!(
            "Unknown workflow/template '{}'. Supported: default, agile, kanban, agent-pipeline, agent-reviewed, jira, github, jira-github.",
            other
        )),
    }
}

fn merge_scaffolds(target: &mut ScaffoldPlan, implicit: ScaffoldPlan) {
    if matches!(target.automation, AutomationScaffold::None) {
        target.automation = implicit.automation;
    }
    if matches!(target.agents, AgentsScaffold::None) {
        target.agents = implicit.agents;
    }
    for r in implicit.sync_remotes {
        if !target.sync_remotes.contains(&r) {
            target.sync_remotes.push(r);
        }
    }
}

fn copy_from_project(
    tasks_root: &Path,
    source_prefix: &str,
    target: &mut ProjectConfig,
) -> Result<(), String> {
    let source_path = crate::utils::paths::project_config_path(tasks_root, source_prefix);
    if !source_path.exists() {
        return Err(format!("Source project '{}' does not exist", source_prefix));
    }
    let text = fs::read_to_string(&source_path)
        .map_err(|e| format!("Failed to read {}: {}", source_path.display(), e))?;
    let parsed =
        crate::config::normalization::parse_project_from_yaml_str(&target.project_name, &text)
            .map_err(|e| format!("Failed to parse source config: {}", e))?;

    // Copy everything except identity fields (name, prefix handled by caller).
    target.issue_states = parsed.issue_states.clone().or(target.issue_states.take());
    target.issue_types = parsed.issue_types.clone().or(target.issue_types.take());
    target.issue_priorities = parsed
        .issue_priorities
        .clone()
        .or(target.issue_priorities.take());
    target.tags = parsed.tags.clone().or(target.tags.take());
    target.default_assignee = parsed
        .default_assignee
        .clone()
        .or(target.default_assignee.take());
    target.default_reporter = parsed
        .default_reporter
        .clone()
        .or(target.default_reporter.take());
    target.default_priority = parsed
        .default_priority
        .clone()
        .or(target.default_priority.take());
    target.default_status = parsed
        .default_status
        .clone()
        .or(target.default_status.take());
    target.default_tags = parsed.default_tags.clone().or(target.default_tags.take());
    target.members = parsed.members.clone().or(target.members.take());
    target.custom_fields = parsed.custom_fields.clone().or(target.custom_fields.take());
    Ok(())
}

// Silence unused-import warnings by exposing Yaml via `super` when tests need it.
#[allow(dead_code)]
pub(crate) fn _yaml_phantom(_: Yaml) {}
