//! Interactive wizard for `lotar init`.
//!
//! Runs only when stdin+stdout are TTYs and the caller didn't pass `--yes`.
//! All answers can be pre-supplied via flags; the wizard only asks for values
//! that are still missing.

use std::io::{BufRead, IsTerminal, Write};

use super::builder::{InitOverrides, WorkflowPreset};
use super::scaffolds::ScaffoldPlan;
use crate::output::OutputRenderer;
use crate::types::Priority;

/// Aggregate state filled in by the wizard.
pub struct WizardOutcome {
    pub project_name: String,
    pub prefix: Option<String>,
    pub workflow: WorkflowPreset,
    pub overrides: InitOverrides,
    pub scaffolds: ScaffoldPlan,
}

pub struct WizardInputs {
    pub project_name: Option<String>,
    pub prefix: Option<String>,
    pub workflow: Option<WorkflowPreset>,
    pub overrides: InitOverrides,
    pub scaffolds: ScaffoldPlan,
}

/// Returns true when we should prompt the user interactively.
pub fn should_prompt(yes: bool, dry_run: bool) -> bool {
    if yes {
        return false;
    }
    if dry_run {
        // Preview works without prompts; allow opt-in only when a TTY is present.
        return std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    }
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Execute the wizard, filling in any values the caller didn't provide.
pub fn run(
    inputs: WizardInputs,
    detected_project_name: Option<&str>,
    renderer: &OutputRenderer,
) -> Result<WizardOutcome, String> {
    renderer.emit_info("Project init wizard — press Enter to accept the [default].");
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();

    let default_name = inputs
        .project_name
        .clone()
        .or_else(|| detected_project_name.map(|s| s.to_string()))
        .unwrap_or_else(|| "my-project".to_string());
    let project_name = prompt(
        &mut stdin,
        &format!("Project name [{}]", default_name),
        Some(&default_name),
    )?;

    let default_prefix = inputs
        .prefix
        .clone()
        .unwrap_or_else(|| crate::utils::project::generate_project_prefix(&project_name));
    let prefix_raw = prompt(
        &mut stdin,
        &format!("Project prefix [{}]", default_prefix),
        Some(&default_prefix),
    )?;
    let prefix = if prefix_raw.trim().is_empty() {
        None
    } else {
        Some(prefix_raw.trim().to_ascii_uppercase())
    };

    let default_workflow = inputs.workflow.unwrap_or(WorkflowPreset::Default);
    let workflow_raw = prompt(
        &mut stdin,
        &format!(
            "Workflow (default/agile/kanban) [{}]",
            default_workflow.label()
        ),
        Some(default_workflow.label()),
    )?;
    let workflow = WorkflowPreset::parse(&workflow_raw)?;

    let mut overrides = inputs.overrides;

    if overrides.default_priority.is_none() {
        let current = overrides
            .default_priority
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| "Medium".to_string());
        let raw = prompt(
            &mut stdin,
            &format!("Default priority (Low/Medium/High/Critical) [{}]", current),
            Some(&current),
        )?;
        if !raw.trim().is_empty() {
            let p: Priority = raw
                .trim()
                .parse()
                .map_err(|e| format!("Invalid priority: {}", e))?;
            overrides.default_priority = Some(p);
        }
    }

    if overrides.default_assignee.is_none() {
        let raw = prompt(
            &mut stdin,
            "Default assignee (blank to skip, e.g. @me)",
            None,
        )?;
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            overrides.default_assignee = Some(trimmed.to_string());
        }
    }

    // Scaffolds: if the caller already picked any, skip asking.
    let mut scaffolds = inputs.scaffolds;
    if scaffolds.is_empty() {
        let raw = prompt(
            &mut stdin,
            "Scaffolds to add (comma-separated; e.g. automation,agents,sync:jira) [none]",
            Some(""),
        )?;
        if !raw.trim().is_empty() {
            let tokens: Vec<String> = raw
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            scaffolds = ScaffoldPlan::parse(&tokens)?;
        }
    }

    Ok(WizardOutcome {
        project_name,
        prefix,
        workflow,
        overrides,
        scaffolds,
    })
}

fn prompt<R: BufRead>(stdin: &mut R, label: &str, default: Option<&str>) -> Result<String, String> {
    print!("{}: ", label);
    std::io::stdout()
        .flush()
        .map_err(|e| format!("flush stdout: {}", e))?;
    let mut buf = String::new();
    stdin
        .read_line(&mut buf)
        .map_err(|e| format!("read stdin: {}", e))?;
    let trimmed = buf
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_string();
    if trimmed.is_empty() {
        return Ok(default.unwrap_or("").to_string());
    }
    Ok(trimmed)
}
