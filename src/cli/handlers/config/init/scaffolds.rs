//! Scaffold emission for `lotar init --with=...`.
//!
//! Scaffolds produce side-files (`automation.yml`, `agents.yml`) or append
//! commented blocks to the project config file. They are intentionally
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

    // agents.yml
    if let Some(content) = agents_yaml(&plan.agents) {
        let path = match project_prefix {
            Some(p) => crate::utils::paths::project_dir(tasks_root, p).join("agents.yml"),
            None => tasks_root.join("agents.yml"),
        };
        write_scaffold(&path, &content, force, renderer)?;
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
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
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
        AutomationScaffold::Pipeline => Some(extract_section(
            include_str!("../../../../config/templates/agent-pipeline.yml"),
            "automation",
        )),
        AutomationScaffold::Reviewed => Some(extract_section(
            include_str!("../../../../config/templates/agent-reviewed.yml"),
            "automation",
        )),
    }
}

fn agents_yaml(scaffold: &AgentsScaffold) -> Option<String> {
    match scaffold {
        AgentsScaffold::None => None,
        AgentsScaffold::Example => Some(
            r#"# Agent profiles scaffold.
# See docs/help/agents.md for the full reference.
agents:
  example:
    runner: copilot
    instructions: |
      Describe what this agent should do when assigned a ticket.
"#
            .to_string(),
        ),
        AgentsScaffold::Pipeline => Some(extract_agents_section(include_str!(
            "../../../../config/templates/agent-pipeline.yml"
        ))),
        AgentsScaffold::Reviewed => Some(extract_agents_section(include_str!(
            "../../../../config/templates/agent-reviewed.yml"
        ))),
    }
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
    out.push_str("# See docs/help/automation.md and docs/help/agents.md.\n");
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

/// Extract the `agents:` block from a template's `config:` section and emit
/// it at the top level of a new `agents.yml`.
fn extract_agents_section(template: &str) -> String {
    // Template format: `config:` top-level, then indented `agents:` under it.
    // We walk line-by-line respecting indentation.
    let mut collected: Vec<String> = Vec::new();
    let mut in_config = false;
    let mut in_agents = false;
    let mut agents_indent: usize = 0;
    for line in template.lines() {
        if line.starts_with("config:") {
            in_config = true;
            continue;
        }
        if !in_config {
            continue;
        }
        // A non-indented, non-comment, non-empty line ends the config block.
        if !line.is_empty() && !line.starts_with(char::is_whitespace) && !line.starts_with('#') {
            break;
        }
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if !in_agents {
            if trimmed.starts_with("agents:") {
                in_agents = true;
                agents_indent = indent;
                // push top-level key without leading spaces
                collected.push("agents:".to_string());
            }
            continue;
        }
        // In agents block: continue while indent > agents_indent, or blank line.
        if trimmed.is_empty() {
            collected.push(String::new());
            continue;
        }
        if indent > agents_indent {
            // Strip the base indent so the section stands alone at top level.
            let drop = agents_indent.min(line.len());
            collected.push(line[drop..].to_string());
        } else {
            break;
        }
    }
    let mut out = String::new();
    out.push_str("# Agents scaffold (generated by `lotar init`).\n");
    out.push_str("# See docs/help/agents.md for the full reference.\n");
    if collected.is_empty() {
        out.push_str("agents:\n  # (no agents defined)\n");
    } else {
        out.push_str(&collected.join("\n"));
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
    if !target.exists() {
        renderer.emit_warning(format_args!(
            "Sync scaffold skipped: {} does not exist yet",
            target.display()
        ));
        return Ok(());
    }
    let existing = fs::read_to_string(target)
        .map_err(|e| format!("Failed to read {}: {}", target.display(), e))?;
    let mut block = String::from("\n# Sync remotes scaffold (generated by `lotar init`).\n");
    block.push_str("# Uncomment and customize. See docs/help/sync.md for details.\n");
    block.push_str("# remotes:\n");
    for remote in remotes {
        match remote {
            SyncRemote::Jira => block.push_str(JIRA_COMMENT_BLOCK),
            SyncRemote::GitHub => block.push_str(GITHUB_COMMENT_BLOCK),
        }
    }
    let mut out = existing;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&block);
    fs::write(target, out).map_err(|e| {
        format!(
            "Failed to append sync scaffold to {}: {}",
            target.display(),
            e
        )
    })?;
    renderer.emit_success(format_args!(
        "Appended sync scaffold to {}",
        target.display()
    ));
    Ok(())
}

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
