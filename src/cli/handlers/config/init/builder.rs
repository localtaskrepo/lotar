//! Workflow preset builder for `lotar init`.
//!
//! Produces minimal `ProjectConfig` / `GlobalConfig` values from a workflow
//! preset (`default`, `agile`, `kanban`) and applies user overrides. The
//! canonicalizer in [`crate::config::normalization`] then prunes anything that
//! matches built-in defaults, keeping the written YAML small.

use crate::config::types::{ConfigurableField, GlobalConfig, ProjectConfig, StringConfigField};
use crate::types::{Priority, TaskStatus, TaskType};

/// Overrides supplied on the CLI (after wizard prompts have filled in gaps).
#[derive(Debug, Default, Clone)]
pub struct InitOverrides {
    pub default_assignee: Option<String>,
    pub default_reporter: Option<String>,
    pub default_priority: Option<Priority>,
    pub default_status: Option<TaskStatus>,
    pub states: Option<Vec<TaskStatus>>,
    pub types: Option<Vec<TaskType>>,
    pub priorities: Option<Vec<Priority>>,
    pub tags: Option<Vec<String>>,
}

/// Build a minimal `ProjectConfig` for the given workflow + overrides.
///
/// "default" returns a config with only the project name set so the written
/// YAML is minimal (the canonicalizer prunes everything that matches the
/// built-in global defaults).
pub fn build_project_config(
    project_name: &str,
    workflow: WorkflowPreset,
    overrides: &InitOverrides,
) -> ProjectConfig {
    let mut cfg = ProjectConfig::new(project_name.to_string());

    match workflow {
        WorkflowPreset::Default => {
            // Nothing — rely on global defaults.
        }
        WorkflowPreset::Agile => {
            cfg.issue_states = Some(ConfigurableField {
                values: vec![
                    TaskStatus::from("Todo"),
                    TaskStatus::from("InProgress"),
                    TaskStatus::from("Verify"),
                    TaskStatus::from("Done"),
                ],
            });
            cfg.issue_types = Some(ConfigurableField {
                values: vec![
                    TaskType::from("Epic"),
                    TaskType::from("Feature"),
                    TaskType::from("Bug"),
                    TaskType::from("Spike"),
                    TaskType::from("Chore"),
                ],
            });
            cfg.issue_priorities = Some(ConfigurableField {
                values: vec![
                    Priority::from("Low"),
                    Priority::from("Medium"),
                    Priority::from("High"),
                    Priority::from("Critical"),
                ],
            });
        }
        WorkflowPreset::Kanban => {
            cfg.issue_states = Some(ConfigurableField {
                values: vec![
                    TaskStatus::from("Todo"),
                    TaskStatus::from("InProgress"),
                    TaskStatus::from("Verify"),
                    TaskStatus::from("Done"),
                ],
            });
            cfg.issue_types = Some(ConfigurableField {
                values: vec![
                    TaskType::from("Feature"),
                    TaskType::from("Bug"),
                    TaskType::from("Epic"),
                    TaskType::from("Chore"),
                ],
            });
        }
    }

    apply_overrides_project(&mut cfg, overrides);
    cfg
}

/// Build / update a `GlobalConfig` for `--global` init. `default_project` is
/// applied separately by the orchestrator.
pub fn build_global_config(
    base: Option<GlobalConfig>,
    workflow: WorkflowPreset,
    overrides: &InitOverrides,
) -> GlobalConfig {
    let mut cfg = base.unwrap_or_default();

    match workflow {
        WorkflowPreset::Default => {}
        WorkflowPreset::Agile => {
            cfg.issue_states = ConfigurableField {
                values: vec![
                    TaskStatus::from("Todo"),
                    TaskStatus::from("InProgress"),
                    TaskStatus::from("Verify"),
                    TaskStatus::from("Done"),
                ],
            };
            cfg.issue_types = ConfigurableField {
                values: vec![
                    TaskType::from("Epic"),
                    TaskType::from("Feature"),
                    TaskType::from("Bug"),
                    TaskType::from("Spike"),
                    TaskType::from("Chore"),
                ],
            };
            cfg.issue_priorities = ConfigurableField {
                values: vec![
                    Priority::from("Low"),
                    Priority::from("Medium"),
                    Priority::from("High"),
                    Priority::from("Critical"),
                ],
            };
        }
        WorkflowPreset::Kanban => {
            cfg.issue_states = ConfigurableField {
                values: vec![
                    TaskStatus::from("Todo"),
                    TaskStatus::from("InProgress"),
                    TaskStatus::from("Verify"),
                    TaskStatus::from("Done"),
                ],
            };
            cfg.issue_types = ConfigurableField {
                values: vec![
                    TaskType::from("Feature"),
                    TaskType::from("Bug"),
                    TaskType::from("Epic"),
                    TaskType::from("Chore"),
                ],
            };
        }
    }

    apply_overrides_global(&mut cfg, overrides);
    cfg
}

fn apply_overrides_project(cfg: &mut ProjectConfig, o: &InitOverrides) {
    if let Some(v) = &o.default_assignee {
        cfg.default_assignee = Some(v.clone());
    }
    if let Some(v) = &o.default_reporter {
        cfg.default_reporter = Some(v.clone());
    }
    if let Some(v) = &o.default_priority {
        cfg.default_priority = Some(v.clone());
    }
    if let Some(v) = &o.default_status {
        cfg.default_status = Some(v.clone());
    }
    if let Some(v) = &o.states {
        cfg.issue_states = Some(ConfigurableField { values: v.clone() });
    }
    if let Some(v) = &o.types {
        cfg.issue_types = Some(ConfigurableField { values: v.clone() });
    }
    if let Some(v) = &o.priorities {
        cfg.issue_priorities = Some(ConfigurableField { values: v.clone() });
    }
    if let Some(v) = &o.tags {
        cfg.tags = Some(StringConfigField { values: v.clone() });
    }
}

fn apply_overrides_global(cfg: &mut GlobalConfig, o: &InitOverrides) {
    if let Some(v) = &o.default_assignee {
        cfg.default_assignee = Some(v.clone());
    }
    if let Some(v) = &o.default_reporter {
        cfg.default_reporter = Some(v.clone());
    }
    if let Some(v) = &o.default_priority {
        cfg.default_priority = v.clone();
    }
    if let Some(v) = &o.default_status {
        cfg.default_status = Some(v.clone());
    }
    if let Some(v) = &o.states {
        cfg.issue_states = ConfigurableField { values: v.clone() };
    }
    if let Some(v) = &o.types {
        cfg.issue_types = ConfigurableField { values: v.clone() };
    }
    if let Some(v) = &o.priorities {
        cfg.issue_priorities = ConfigurableField { values: v.clone() };
    }
    if let Some(v) = &o.tags {
        cfg.tags = StringConfigField { values: v.clone() };
    }
}

/// Built-in workflow presets supported by `lotar init`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowPreset {
    Default,
    Agile,
    Kanban,
}

impl WorkflowPreset {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "default" | "" => Ok(Self::Default),
            "agile" => Ok(Self::Agile),
            "kanban" => Ok(Self::Kanban),
            other => Err(format!(
                "Unknown workflow preset '{}'. Expected: default, agile, kanban.",
                other
            )),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Agile => "agile",
            Self::Kanban => "kanban",
        }
    }
}
