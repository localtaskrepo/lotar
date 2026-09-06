//! Orchestrator for `lotar init` / `lotar config init`.

mod builder;
mod scaffolds;
mod wizard;

use std::fs;
use std::path::{Path, PathBuf};

use serde_yaml_ng::Value as Yaml;

use super::ConfigHandler;
use crate::config::manager::ConfigManager;
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

        validate_init_path(&tasks_root, Some(&prefix))?;
        validate_init_path(&tasks_root, None)?;
        // Fail before creating a project if the global config cannot be preserved.
        load_existing_global(&crate::utils::paths::global_config_path(&tasks_root))?;
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
    validate_init_path(tasks_root, None)?;
    let existing = load_existing_global(&global_path)?;

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
    ConfigManager::save_global_config(tasks_root, &cfg).map_err(|e| e.to_string())?;
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
    validate_init_path(tasks_root, Some(prefix))?;
    let path = crate::utils::paths::project_config_path(tasks_root, prefix);
    if path.exists() && !force {
        return Err(format!(
            "Project config already exists at {}. Use --force to overwrite.",
            path.display()
        ));
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

    ConfigManager::save_project_config(tasks_root, prefix, cfg).map_err(|e| e.to_string())?;
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
        match crate::config::persistence::load_global_config(Some(tasks_root)) {
            Ok(mut cfg) => {
                if cfg.default_project.is_empty() {
                    cfg.default_project = prefix.to_string();
                    ConfigManager::save_global_config(tasks_root, &cfg)
                        .map_err(|e| e.to_string())?;
                    renderer.emit_info(format_args!(
                        "Set default_project to '{}' in {}",
                        prefix,
                        path.display()
                    ));
                }
            }
            Err(e) => {
                return Err(format!("Could not parse existing global config: {}", e));
            }
        }
        return Ok(());
    }

    let cfg = GlobalConfig {
        default_project: prefix.to_string(),
        ..GlobalConfig::default()
    };
    ConfigManager::save_global_config(tasks_root, &cfg).map_err(|e| e.to_string())?;
    renderer.emit_success(format_args!(
        "Global configuration created at: {} (default_project={})",
        path.display(),
        prefix
    ));
    Ok(())
}

fn load_existing_global(path: &Path) -> Result<Option<GlobalConfig>, String> {
    if !path.exists() {
        return Ok(None);
    }
    crate::config::persistence::load_global_config(path.parent())
        .map(Some)
        .map_err(|e| e.to_string())
}

// Reject symlinks at and below the selected root, including dangling links.
// Check before any directories are created.
fn validate_init_path(tasks_root: &Path, prefix: Option<&str>) -> Result<(), String> {
    if let Some(prefix) = prefix {
        crate::storage::safety::validate_project_prefix(prefix)?;
    }
    let path = match prefix {
        Some(prefix) => crate::utils::paths::project_config_path(tasks_root, prefix),
        None => crate::utils::paths::global_config_path(tasks_root),
    };
    // macOS commonly exposes /tmp and /var through symlinks, so ancestors above
    // the selected tasks root are resolved normally. The root itself must not link out.
    for candidate in [
        Some(tasks_root.to_path_buf()),
        prefix.map(|p| tasks_root.join(p)),
        Some(path),
    ]
    .into_iter()
    .flatten()
    {
        match fs::symlink_metadata(&candidate) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "Config init refuses symlink path: {}",
                    candidate.display()
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Failed to inspect config path: {}", e)),
        }
    }
    Ok(())
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
    validate_init_path(tasks_root, Some(source_prefix))?;
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
