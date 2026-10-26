//! Scaffold emission for `lotar init --with=...`.
//!
//! Scaffolds produce `automation.yml` or append commented blocks to the
//! config file. Agent templates are merged into the generated config by the
//! init orchestrator (see [`agent_template_config`]). They are intentionally
//! opinionated but minimal, with pointers to the docs for deeper reference.

use std::fs;
use std::path::Path;

use crate::output::OutputRenderer;

/// Parsed `--with=` selection.
#[derive(Debug, Default, Clone)]
pub struct ScaffoldPlan {
    pub automation: AutomationScaffold,
    pub agents: AgentsScaffold,
    pub sync_remotes: Vec<SyncRemote>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum AutomationScaffold {
    #[default]
    None,
    Example,
    Pipeline,
    Reviewed,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum AgentsScaffold {
    #[default]
    None,
    Example,
    Pipeline,
    Reviewed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncRemote {
    Jira,
    GitHub,
}

impl ScaffoldPlan {
    pub fn is_empty(&self) -> bool {
        matches!(self.automation, AutomationScaffold::None)
            && matches!(self.agents, AgentsScaffold::None)
            && self.sync_remotes.is_empty()
    }

    /// Parse comma-separated `--with=` tokens. Accepts:
    /// `automation`, `automation:pipeline`, `automation:reviewed`,
    /// `agents`, `agents:pipeline`, `agents:reviewed`,
    /// `sync:jira`, `sync:github`.
    pub fn parse(tokens: &[String]) -> Result<Self, String> {
        let mut plan = ScaffoldPlan::default();
        for raw in tokens {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            match token.to_ascii_lowercase().as_str() {
                "automation" => plan.automation = AutomationScaffold::Example,
                "automation:pipeline" => plan.automation = AutomationScaffold::Pipeline,
                "automation:reviewed" => plan.automation = AutomationScaffold::Reviewed,
                "agents" => plan.agents = AgentsScaffold::Example,
                "agents:pipeline" => plan.agents = AgentsScaffold::Pipeline,
                "agents:reviewed" => plan.agents = AgentsScaffold::Reviewed,
                "sync:jira" => {
                    if !plan.sync_remotes.contains(&SyncRemote::Jira) {
                        plan.sync_remotes.push(SyncRemote::Jira);
                    }
                }
                "sync:github" => {
                    if !plan.sync_remotes.contains(&SyncRemote::GitHub) {
                        plan.sync_remotes.push(SyncRemote::GitHub);
                    }
                }
                other => {
                    return Err(format!(
                        "Unknown scaffold '{}'. Supported: automation[:pipeline|:reviewed], agents[:pipeline|:reviewed], sync:jira, sync:github.",
                        other
                    ));
                }
            }
        }
        Ok(plan)
    }
}

/// Write scaffold files. Paths are relative to the provided tasks root.
/// When `project_prefix` is `Some`, scaffolds are written to the project dir.
pub fn apply(
    tasks_root: &Path,
    project_prefix: Option<&str>,
    plan: &ScaffoldPlan,
    force: bool,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    if plan.is_empty() {
        return Ok(());
    }

    // automation.yml
    if let Some(content) = automation_yaml(&plan.automation) {
        let path = match project_prefix {
            Some(p) => crate::utils::paths::project_automation_path(tasks_root, p),
            None => crate::utils::paths::global_automation_path(tasks_root),
        };
        write_scaffold(&path, &content, force, renderer)?;
    }

    // The example agent profile is appended to the config as a commented
    // block; agent templates are merged into the config by the caller.
    if matches!(plan.agents, AgentsScaffold::Example) {
        let target = match project_prefix {
            Some(p) => crate::utils::paths::project_config_path(tasks_root, p),
            None => crate::utils::paths::global_config_path(tasks_root),
        };
        append_comment_block(&target, AGENTS_EXAMPLE_BLOCK, "agents", renderer)?;
    }

    // sync remotes — append to project config (or global config) as a
    // commented block. Small scaffolds stay inline as requested.
    if !plan.sync_remotes.is_empty() {
        let target = match project_prefix {
            Some(p) => crate::utils::paths::project_config_path(tasks_root, p),
            None => crate::utils::paths::global_config_path(tasks_root),
        };
        append_sync_comment(&target, &plan.sync_remotes, renderer)?;
    }

    Ok(())
}

fn write_scaffold(
    path: &Path,
    content: &str,
    force: bool,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    if path.exists() && !force {
        renderer.emit_warning(format_args!(
            "Skipping existing {} (use --force to overwrite)",
            path.display()
        ));
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    crate::storage::safety::atomic_write_file(path, content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    renderer.emit_success(format_args!("Wrote scaffold: {}", path.display()));
    Ok(())
}

fn automation_yaml(scaffold: &AutomationScaffold) -> Option<String> {
    match scaffold {
        AutomationScaffold::None => None,
        AutomationScaffold::Example => Some(
            r#"# Automation rules scaffold.
# See docs/help/automation.md for the full reference.
automation:
  rules:
    # - name: Assign to reporter on start
    #   when:
    #     assignee: "@bot"
    #   on:
    #     job_start:
    #       set:
    #         status: InProgress
    #     complete:
    #       set:
    #         status: Done
"#
            .to_string(),
        ),
        AutomationScaffold::Pipeline => {
            Some(extract_section(AGENT_PIPELINE_TEMPLATE, "automation"))
        }
        AutomationScaffold::Reviewed => {
            Some(extract_section(AGENT_REVIEWED_TEMPLATE, "automation"))
        }
    }
}

const AGENT_PIPELINE_TEMPLATE: &str =
    include_str!("../../../../config/templates/agent-pipeline.yml");
const AGENT_REVIEWED_TEMPLATE: &str =
    include_str!("../../../../config/templates/agent-reviewed.yml");

/// The `config:` block of the agent template behind a scaffold, as YAML.
/// Its statuses, agent profiles and worktree settings are what the
/// template's automation rules refer to, so init merges them into the
/// generated config.
pub fn agent_template_config(scaffold: &AgentsScaffold) -> Option<String> {
    let template = match scaffold {
        AgentsScaffold::Pipeline => AGENT_PIPELINE_TEMPLATE,
        AgentsScaffold::Reviewed => AGENT_REVIEWED_TEMPLATE,
        AgentsScaffold::None | AgentsScaffold::Example => return None,
    };
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(template).ok()?;
    serde_yaml_ng::to_string(doc.get("config")?).ok()
}

/// Extract a top-level section (e.g. `automation:`) verbatim from a template
/// YAML file. Returns the section prefixed with a doc pointer comment.
fn extract_section(template: &str, section: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut in_section = false;
    let marker = format!("{}:", section);
    for line in template.lines() {
        if line.starts_with(&marker) {
            in_section = true;
            lines.push(line);
            continue;
        }
        if in_section {
            // End when another top-level key appears.
            let is_toplevel = !line.is_empty()
                && !line.starts_with(char::is_whitespace)
                && !line.starts_with('#');
            if is_toplevel {
                break;
            }
            lines.push(line);
        }
    }
    let mut out = format!("# {} scaffold (generated by `lotar init`).\n", section);
    out.push_str("# Tune rules and expectations to your workflow.\n");
    out.push_str("# See docs/help/automation.md and docs/help/agent.md.\n");
    if lines.is_empty() {
        out.push_str(&format!("{}:\n  # (no rules defined)\n", section));
    } else {
        out.push_str(&lines.join("\n"));
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn append_sync_comment(
    target: &Path,
    remotes: &[SyncRemote],
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mut block = String::from("# Sync remotes scaffold (generated by `lotar init`).\n");
    block.push_str("# Uncomment and customize. See docs/help/sync.md for details.\n");
    block.push_str("# remotes:\n");
    for remote in remotes {
        match remote {
            SyncRemote::Jira => block.push_str(JIRA_COMMENT_BLOCK),
            SyncRemote::GitHub => block.push_str(GITHUB_COMMENT_BLOCK),
        }
    }
    append_comment_block(target, &block, "sync", renderer)
}

/// Append a commented scaffold block to an existing config file.
fn append_comment_block(
    target: &Path,
    block: &str,
    label: &str,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    if !target.exists() {
        renderer.emit_warning(format_args!(
            "{} scaffold skipped: {} does not exist yet",
            label,
            target.display()
        ));
        return Ok(());
    }
    let mut out = fs::read_to_string(target)
        .map_err(|e| format!("Failed to read {}: {}", target.display(), e))?;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(block);
    crate::storage::safety::atomic_write_file(target, &out).map_err(|e| {
        format!(
            "Failed to append {} scaffold to {}: {}",
            label,
            target.display(),
            e
        )
    })?;
    renderer.emit_success(format_args!(
        "Appended {} scaffold to {}",
        label,
        target.display()
    ));
    Ok(())
}

const AGENTS_EXAMPLE_BLOCK: &str = r#"# Agent profiles scaffold (generated by `lotar init`).
# Uncomment and customize. Assigning a ticket to @<name> starts a job.
# See docs/help/agent.md for runners, permissions, model and effort.
# agents:
#   example:
#     runner: claude          # claude, codex, copilot, gemini or command
#     # model: sonnet
#     # effort: medium
#     instructions: |
#       Describe what this agent should do when assigned a ticket.
"#;

const JIRA_COMMENT_BLOCK: &str = r#"#   jira:
#     provider: jira
#     project: DEMO
#     auth_profile: jira.default
#     mapping:
#       title: summary
#       description: description
#       status:
#         field: status
#         values:
#           Todo: "To Do"
#           InProgress: "In Progress"
#           Done: Done
"#;

const GITHUB_COMMENT_BLOCK: &str = r#"#   github:
#     provider: github
#     repo: your-org/your-repo
#     auth_profile: github.default
#     mapping:
#       title: title
#       description: body
#       status:
#         field: state
#         values:
#           Todo: open
#           InProgress: open
#           Done: closed
"#;
