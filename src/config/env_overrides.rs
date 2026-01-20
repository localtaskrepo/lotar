use crate::config::operations::apply_field_to_global_config;
use crate::config::types::{GlobalConfig, ResolvedConfig};
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone)]
pub struct EnvOverrideDefinition {
    pub key: &'static str,
    pub field: &'static str,
    pub env_vars: &'static [&'static str],
}

const fn def(
    key: &'static str,
    field: &'static str,
    env_vars: &'static [&'static str],
) -> EnvOverrideDefinition {
    EnvOverrideDefinition {
        key,
        field,
        env_vars,
    }
}

pub const ENV_OVERRIDE_DEFINITIONS: &[EnvOverrideDefinition] = &[
    def(
        "server_port",
        "server_port",
        &["LOTAR_PORT", "LOTAR_SERVER_PORT"],
    ),
    def(
        "default_project",
        "default_project",
        &["LOTAR_PROJECT", "LOTAR_DEFAULT_PROJECT"],
    ),
    def(
        "default_assignee",
        "default_assignee",
        &["LOTAR_DEFAULT_ASSIGNEE"],
    ),
    def(
        "default_reporter",
        "default_reporter",
        &["LOTAR_DEFAULT_REPORTER"],
    ),
    def("default_tags", "default_tags", &["LOTAR_DEFAULT_TAGS"]),
    def("members", "members", &["LOTAR_MEMBERS"]),
    def(
        "strict_members",
        "strict_members",
        &["LOTAR_STRICT_MEMBERS"],
    ),
    def(
        "auto_populate_members",
        "auto_populate_members",
        &["LOTAR_AUTO_POPULATE_MEMBERS"],
    ),
    def(
        "default_priority",
        "default_priority",
        &["LOTAR_DEFAULT_PRIORITY"],
    ),
    def(
        "default_status",
        "default_status",
        &["LOTAR_DEFAULT_STATUS"],
    ),
    def("issue_states", "issue_states", &["LOTAR_ISSUE_STATES"]),
    def("issue_types", "issue_types", &["LOTAR_ISSUE_TYPES"]),
    def(
        "issue_priorities",
        "issue_priorities",
        &["LOTAR_ISSUE_PRIORITIES"],
    ),
    def("issue_tags", "tags", &["LOTAR_ISSUE_TAGS"]),
    def("custom_fields", "custom_fields", &["LOTAR_CUSTOM_FIELDS"]),
    def(
        "scan_signal_words",
        "scan_signal_words",
        &["LOTAR_SCAN_SIGNAL_WORDS"],
    ),
    def(
        "scan_ticket_patterns",
        "scan_ticket_patterns",
        &["LOTAR_SCAN_TICKET_PATTERNS"],
    ),
    def(
        "scan_enable_ticket_words",
        "scan_enable_ticket_words",
        &["LOTAR_SCAN_ENABLE_TICKET_WORDS"],
    ),
    def(
        "scan_enable_mentions",
        "scan_enable_mentions",
        &["LOTAR_SCAN_ENABLE_MENTIONS"],
    ),
    def(
        "scan_strip_attributes",
        "scan_strip_attributes",
        &["LOTAR_SCAN_STRIP_ATTRIBUTES"],
    ),
    def(
        "auto_set_reporter",
        "auto_set_reporter",
        &["LOTAR_AUTO_SET_REPORTER"],
    ),
    def(
        "auto_assign_on_status",
        "auto_assign_on_status",
        &["LOTAR_AUTO_ASSIGN_ON_STATUS"],
    ),
    def(
        "auto_codeowners_assign",
        "auto_codeowners_assign",
        &["LOTAR_AUTO_CODEOWNERS_ASSIGN"],
    ),
    def(
        "auto_tags_from_path",
        "auto_tags_from_path",
        &["LOTAR_AUTO_TAGS_FROM_PATH"],
    ),
    def(
        "auto_branch_infer_type",
        "auto_branch_infer_type",
        &["LOTAR_AUTO_BRANCH_INFER_TYPE"],
    ),
    def(
        "auto_branch_infer_status",
        "auto_branch_infer_status",
        &["LOTAR_AUTO_BRANCH_INFER_STATUS"],
    ),
    def(
        "auto_branch_infer_priority",
        "auto_branch_infer_priority",
        &["LOTAR_AUTO_BRANCH_INFER_PRIORITY"],
    ),
    def("auto_identity", "auto_identity", &["LOTAR_AUTO_IDENTITY"]),
    def(
        "auto_identity_git",
        "auto_identity_git",
        &["LOTAR_AUTO_IDENTITY_GIT"],
    ),
    def(
        "attachments_dir",
        "attachments_dir",
        &["LOTAR_ATTACHMENTS_DIR"],
    ),
    def(
        "attachments_max_upload_mb",
        "attachments_max_upload_mb",
        &["LOTAR_ATTACHMENTS_MAX_UPLOAD_MB"],
    ),
    def(
        "sync_reports_dir",
        "sync_reports_dir",
        &["LOTAR_SYNC_REPORTS_DIR"],
    ),
    def(
        "sync_write_reports",
        "sync_write_reports",
        &["LOTAR_SYNC_WRITE_REPORTS"],
    ),
    def(
        "branch_type_aliases",
        "branch_type_aliases",
        &["LOTAR_BRANCH_TYPE_ALIASES"],
    ),
    def(
        "branch_status_aliases",
        "branch_status_aliases",
        &["LOTAR_BRANCH_STATUS_ALIASES"],
    ),
    def(
        "branch_priority_aliases",
        "branch_priority_aliases",
        &["LOTAR_BRANCH_PRIORITY_ALIASES"],
    ),
    def(
        "sprints_defaults_capacity_points",
        "sprints_defaults_capacity_points",
        &["LOTAR_SPRINTS_DEFAULT_CAPACITY_POINTS"],
    ),
    def(
        "sprints_defaults_capacity_hours",
        "sprints_defaults_capacity_hours",
        &["LOTAR_SPRINTS_DEFAULT_CAPACITY_HOURS"],
    ),
    def(
        "sprints_defaults_length",
        "sprints_defaults_length",
        &["LOTAR_SPRINTS_DEFAULT_LENGTH"],
    ),
    def(
        "sprints_defaults_overdue_after",
        "sprints_defaults_overdue_after",
        &["LOTAR_SPRINTS_DEFAULT_OVERDUE_AFTER"],
    ),
    def(
        "sprints_notifications_enabled",
        "sprints_notifications_enabled",
        &["LOTAR_SPRINTS_NOTIFICATIONS_ENABLED"],
    ),
    def("web_ui_path", "web_ui_path", &["LOTAR_WEB_UI_PATH"]),
];

#[derive(Debug)]
pub struct EnvOverrideSnapshot {
    pub global: GlobalConfig,
    pub resolved: ResolvedConfig,
    pub report: EnvOverrideReport,
}

impl EnvOverrideSnapshot {
    pub fn applied_keys(&self) -> &HashSet<&'static str> {
        &self.report.applied_keys
    }
}

#[derive(Debug, Default, Clone)]
pub struct EnvOverrideReport {
    pub applied_keys: HashSet<&'static str>,
    pub errors: Vec<EnvOverrideError>,
}

#[derive(Debug, Clone)]
pub struct EnvOverrideError {
    pub key: &'static str,
    pub env_var: &'static str,
    pub message: String,
}

pub fn capture_env_override_snapshot() -> EnvOverrideSnapshot {
    let mut global = GlobalConfig::default();
    let report = apply_env_overrides(&mut global);
    let resolved = ResolvedConfig::from_global(global.clone());
    EnvOverrideSnapshot {
        global,
        resolved,
        report,
    }
}

pub fn apply_env_overrides(config: &mut GlobalConfig) -> EnvOverrideReport {
    let mut report = EnvOverrideReport::default();

    for definition in ENV_OVERRIDE_DEFINITIONS {
        for env_name in definition.env_vars {
            match std::env::var(env_name) {
                Ok(raw_value) => {
                    match apply_field_to_global_config(config, definition.field, &raw_value) {
                        Ok(()) => {
                            report.applied_keys.insert(definition.key);
                        }
                        Err(err) => {
                            report.errors.push(EnvOverrideError {
                                key: definition.key,
                                env_var: env_name,
                                message: err.to_string(),
                            });
                        }
                    }
                    break;
                }
                Err(std::env::VarError::NotPresent) => {
                    continue;
                }
                Err(std::env::VarError::NotUnicode(_)) => {
                    report.errors.push(EnvOverrideError {
                        key: definition.key,
                        env_var: env_name,
                        message: "contains invalid UTF-8".to_string(),
                    });
                    break;
                }
            }
        }
    }

    report
}

pub fn env_signature() -> String {
    let mut names = BTreeSet::new();
    for definition in ENV_OVERRIDE_DEFINITIONS {
        for env_name in definition.env_vars {
            names.insert(*env_name);
        }
    }

    names
        .into_iter()
        .map(|name| {
            let value = std::env::var(name).unwrap_or_default();
            format!("{name}={value}")
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub fn env_vars_for_key(key: &str) -> Option<&'static [&'static str]> {
    ENV_OVERRIDE_DEFINITIONS
        .iter()
        .find(|definition| definition.key == key)
        .map(|definition| definition.env_vars)
}
