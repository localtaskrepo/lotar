use crate::config::env_overrides::capture_env_override_snapshot;
use crate::config::types::{GlobalConfig, ProjectConfig, ResolvedConfig};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug)]
pub struct ConfigSourceEntry {
    pub path: &'static str,
    pub label_key: &'static str,
    pub inspect_key: &'static str,
}

const fn entry(
    path: &'static str,
    label_key: &'static str,
    inspect_key: &'static str,
) -> ConfigSourceEntry {
    ConfigSourceEntry {
        path,
        label_key,
        inspect_key,
    }
}

pub const CONFIG_SOURCE_ENTRIES: &[ConfigSourceEntry] = &[
    entry("server.port", "server_port", "server_port"),
    entry("default.project", "default_project", "default_project"),
    entry("default.assignee", "default_assignee", "default_assignee"),
    entry("default.reporter", "default_reporter", "default_reporter"),
    entry("default.tags", "default_tags", "default_tags"),
    entry("members", "members", "members"),
    entry("default.strict-members", "strict_members", "strict_members"),
    entry("default.priority", "default_priority", "default_priority"),
    entry("default.status", "default_status", "default_status"),
    entry("issue.states", "issue_states", "issue_states"),
    entry("issue.types", "issue_types", "issue_types"),
    entry("issue.priorities", "issue_priorities", "issue_priorities"),
    entry("issue.tags", "issue_tags", "tags"),
    entry("custom.fields", "custom_fields", "custom_fields"),
    entry(
        "scan.signal-words",
        "scan_signal_words",
        "scan_signal_words",
    ),
    entry(
        "scan.ticket-patterns",
        "scan_ticket_patterns",
        "scan_ticket_patterns",
    ),
    entry(
        "scan.enable-ticket-words",
        "scan_enable_ticket_words",
        "scan_enable_ticket_words",
    ),
    entry(
        "scan.enable-mentions",
        "scan_enable_mentions",
        "scan_enable_mentions",
    ),
    entry(
        "scan.strip-attributes",
        "scan_strip_attributes",
        "scan_strip_attributes",
    ),
    entry(
        "auto.populate-members",
        "auto_populate_members",
        "auto_populate_members",
    ),
    entry(
        "auto.set-reporter",
        "auto_set_reporter",
        "auto_set_reporter",
    ),
    entry(
        "auto.assign-on-status",
        "auto_assign_on_status",
        "auto_assign_on_status",
    ),
    entry(
        "auto.codeowners-assign",
        "auto_codeowners_assign",
        "auto_codeowners_assign",
    ),
    entry(
        "auto.tags-from-path",
        "auto_tags_from_path",
        "auto_tags_from_path",
    ),
    entry(
        "auto.branch-infer-type",
        "auto_branch_infer_type",
        "auto_branch_infer_type",
    ),
    entry(
        "auto.branch-infer-status",
        "auto_branch_infer_status",
        "auto_branch_infer_status",
    ),
    entry(
        "auto.branch-infer-priority",
        "auto_branch_infer_priority",
        "auto_branch_infer_priority",
    ),
    entry("auto.identity", "auto_identity", "auto_identity"),
    entry(
        "auto.identity-git",
        "auto_identity_git",
        "auto_identity_git",
    ),
    entry(
        "sprints.defaults.capacity_points",
        "sprints_defaults_capacity_points",
        "sprints_defaults_capacity_points",
    ),
    entry(
        "sprints.defaults.capacity_hours",
        "sprints_defaults_capacity_hours",
        "sprints_defaults_capacity_hours",
    ),
    entry(
        "sprints.defaults.length",
        "sprints_defaults_length",
        "sprints_defaults_length",
    ),
    entry(
        "sprints.defaults.overdue_after",
        "sprints_defaults_overdue_after",
        "sprints_defaults_overdue_after",
    ),
    entry(
        "sprints.notifications.enabled",
        "sprints_notifications_enabled",
        "sprints_notifications_enabled",
    ),
    entry(
        "branch.type-aliases",
        "branch_type_aliases",
        "branch_type_aliases",
    ),
    entry(
        "branch.status-aliases",
        "branch_status_aliases",
        "branch_status_aliases",
    ),
    entry(
        "branch.priority-aliases",
        "branch_priority_aliases",
        "branch_priority_aliases",
    ),
];

pub fn populate_source_labels<F>(labels: &mut HashMap<String, String>, mut resolver: F)
where
    F: FnMut(&ConfigSourceEntry) -> String,
{
    for entry in CONFIG_SOURCE_ENTRIES {
        labels.insert(entry.path.to_string(), resolver(entry));
    }
}

fn scope_for_global_value<T>(
    resolved_value: &T,
    home_value: Option<&T>,
    global_value: Option<&T>,
    default_value: &T,
) -> &'static str
where
    T: PartialEq,
{
    if home_value.is_some_and(|value| value == resolved_value) {
        return "home";
    }

    if global_value.is_some_and(|value| value == resolved_value && value != default_value) {
        return "global";
    }

    "default"
}

struct GlobalScopeContext<'a> {
    base_config: &'a ResolvedConfig,
    global_cfg: &'a Option<GlobalConfig>,
    home_cfg: &'a Option<GlobalConfig>,
    env_resolved: &'a ResolvedConfig,
    env_applied: &'a HashSet<&'static str>,
}

impl<'a> GlobalScopeContext<'a> {
    fn new(
        base_config: &'a ResolvedConfig,
        global_cfg: &'a Option<GlobalConfig>,
        home_cfg: &'a Option<GlobalConfig>,
        env_resolved: &'a ResolvedConfig,
        env_applied: &'a HashSet<&'static str>,
    ) -> Self {
        Self {
            base_config,
            global_cfg,
            home_cfg,
            env_resolved,
            env_applied,
        }
    }

    fn global_label(&self, key: &str) -> &'static str {
        source_label_for_global(
            self.base_config,
            self.global_cfg,
            self.home_cfg,
            self.env_resolved,
            self.env_applied,
            key,
        )
    }
}

fn project_scope_with_global(
    key: &str,
    has_project_override: bool,
    values_differ: bool,
    context: &GlobalScopeContext<'_>,
) -> String {
    if has_project_override || values_differ {
        "project".to_string()
    } else {
        context.global_label(key).to_string()
    }
}

pub fn build_global_source_labels(
    resolved: &ResolvedConfig,
    global_cfg: &Option<GlobalConfig>,
    home_cfg: &Option<GlobalConfig>,
) -> HashMap<String, String> {
    let snapshot = capture_env_override_snapshot();
    let env_resolved = &snapshot.resolved;
    let env_applied = snapshot.applied_keys();
    let mut labels = HashMap::new();
    populate_source_labels(&mut labels, |entry| {
        GlobalScopeContext::new(resolved, global_cfg, home_cfg, env_resolved, env_applied)
            .global_label(entry.label_key)
            .to_string()
    });
    labels
}

#[allow(clippy::too_many_arguments)]
pub fn build_project_source_labels(
    resolved_project: &ResolvedConfig,
    base_config: &ResolvedConfig,
    project_cfg: Option<&ProjectConfig>,
    global_cfg: &Option<GlobalConfig>,
    home_cfg: &Option<GlobalConfig>,
) -> HashMap<String, String> {
    let snapshot = capture_env_override_snapshot();
    let env_resolved = &snapshot.resolved;
    let env_applied = snapshot.applied_keys();
    let context =
        GlobalScopeContext::new(base_config, global_cfg, home_cfg, env_resolved, env_applied);
    let mut labels = HashMap::new();
    populate_source_labels(&mut labels, |entry| {
        source_label_for_project(entry.label_key, resolved_project, project_cfg, &context)
    });
    labels
}

pub fn collapse_label_to_scope(label: &str) -> &'static str {
    match label {
        "project" => "project",
        "global" | "home" | "env" => "global",
        _ => "built_in",
    }
}

fn source_label_for_global(
    resolved: &ResolvedConfig,
    global_cfg: &Option<GlobalConfig>,
    home_cfg: &Option<GlobalConfig>,
    env_resolved: &ResolvedConfig,
    env_applied: &HashSet<&'static str>,
    key: &str,
) -> &'static str {
    let env_matches_key = env_applied.iter().any(|candidate| *candidate == key);
    if env_matches_key && env_value_matches(resolved, env_resolved, key) {
        return "env";
    }

    let defaults = GlobalConfig::default();
    let home = home_cfg.as_ref();
    let global = global_cfg.as_ref();

    macro_rules! scope_field {
        ($field:ident) => {
            scope_for_global_value(
                &resolved.$field,
                home.map(|cfg| &cfg.$field),
                global.map(|cfg| &cfg.$field),
                &defaults.$field,
            )
        };
        ($field:ident $(.$rest:ident)+) => {
            scope_for_global_value(
                &resolved.$field $(.$rest)+,
                home.map(|cfg| &cfg.$field $(.$rest)+),
                global.map(|cfg| &cfg.$field $(.$rest)+),
                &defaults.$field $(.$rest)+,
            )
        };
    }

    match key {
        "server_port" => scope_field!(server_port),
        "default_project" => scope_field!(default_project),
        "default_assignee" => scope_field!(default_assignee),
        "default_reporter" => scope_field!(default_reporter),
        "default_tags" => scope_field!(default_tags),
        "members" => scope_field!(members),
        "strict_members" => scope_field!(strict_members),
        "auto_populate_members" => scope_field!(auto_populate_members),
        "default_priority" => scope_field!(default_priority),
        "default_status" => scope_field!(default_status),
        "issue_states" => scope_field!(issue_states.values),
        "issue_types" => scope_field!(issue_types.values),
        "issue_priorities" => scope_field!(issue_priorities.values),
        "issue_tags" => scope_field!(tags.values),
        "custom_fields" => scope_field!(custom_fields.values),
        "scan_signal_words" => scope_field!(scan_signal_words),
        "scan_ticket_patterns" => scope_field!(scan_ticket_patterns),
        "scan_enable_ticket_words" => scope_field!(scan_enable_ticket_words),
        "scan_enable_mentions" => scope_field!(scan_enable_mentions),
        "scan_strip_attributes" => scope_field!(scan_strip_attributes),
        "auto_set_reporter" => scope_field!(auto_set_reporter),
        "auto_assign_on_status" => scope_field!(auto_assign_on_status),
        "auto_codeowners_assign" => scope_field!(auto_codeowners_assign),
        "auto_tags_from_path" => scope_field!(auto_tags_from_path),
        "auto_branch_infer_type" => scope_field!(auto_branch_infer_type),
        "auto_branch_infer_status" => scope_field!(auto_branch_infer_status),
        "auto_branch_infer_priority" => scope_field!(auto_branch_infer_priority),
        "auto_identity" => scope_field!(auto_identity),
        "auto_identity_git" => scope_field!(auto_identity_git),
        "sprints_defaults_capacity_points" => scope_for_global_value(
            &resolved.sprint_defaults.capacity_points,
            home.map(|cfg| &cfg.sprints.defaults.capacity_points),
            global.map(|cfg| &cfg.sprints.defaults.capacity_points),
            &defaults.sprints.defaults.capacity_points,
        ),
        "sprints_defaults_capacity_hours" => scope_for_global_value(
            &resolved.sprint_defaults.capacity_hours,
            home.map(|cfg| &cfg.sprints.defaults.capacity_hours),
            global.map(|cfg| &cfg.sprints.defaults.capacity_hours),
            &defaults.sprints.defaults.capacity_hours,
        ),
        "sprints_defaults_length" => scope_for_global_value(
            &resolved.sprint_defaults.length,
            home.map(|cfg| &cfg.sprints.defaults.length),
            global.map(|cfg| &cfg.sprints.defaults.length),
            &defaults.sprints.defaults.length,
        ),
        "sprints_defaults_overdue_after" => scope_for_global_value(
            &resolved.sprint_defaults.overdue_after,
            home.map(|cfg| &cfg.sprints.defaults.overdue_after),
            global.map(|cfg| &cfg.sprints.defaults.overdue_after),
            &defaults.sprints.defaults.overdue_after,
        ),
        "sprints_notifications_enabled" => scope_for_global_value(
            &resolved.sprint_notifications.enabled,
            home.map(|cfg| &cfg.sprints.notifications.enabled),
            global.map(|cfg| &cfg.sprints.notifications.enabled),
            &defaults.sprints.notifications.enabled,
        ),
        "branch_type_aliases" => scope_field!(branch_type_aliases),
        "branch_status_aliases" => scope_field!(branch_status_aliases),
        "branch_priority_aliases" => scope_field!(branch_priority_aliases),
        _ => "default",
    }
}

fn env_value_matches(resolved: &ResolvedConfig, env_resolved: &ResolvedConfig, key: &str) -> bool {
    macro_rules! env_equal {
        ($field:ident) => {
            env_resolved.$field == resolved.$field
        };
        ($field:ident $(.$rest:ident)+) => {
            env_resolved.$field $(.$rest)+ == resolved.$field $(.$rest)+
        };
    }

    match key {
        "server_port" => env_equal!(server_port),
        "default_project" => env_equal!(default_project),
        "default_assignee" => env_equal!(default_assignee),
        "default_reporter" => env_equal!(default_reporter),
        "default_tags" => env_equal!(default_tags),
        "members" => env_equal!(members),
        "strict_members" => env_equal!(strict_members),
        "auto_populate_members" => env_equal!(auto_populate_members),
        "default_priority" => env_equal!(default_priority),
        "default_status" => env_equal!(default_status),
        "issue_states" => env_equal!(issue_states.values),
        "issue_types" => env_equal!(issue_types.values),
        "issue_priorities" => env_equal!(issue_priorities.values),
        "issue_tags" => env_equal!(tags.values),
        "custom_fields" => env_equal!(custom_fields.values),
        "scan_signal_words" => env_equal!(scan_signal_words),
        "scan_ticket_patterns" => env_equal!(scan_ticket_patterns),
        "scan_enable_ticket_words" => env_equal!(scan_enable_ticket_words),
        "scan_enable_mentions" => env_equal!(scan_enable_mentions),
        "scan_strip_attributes" => env_equal!(scan_strip_attributes),
        "auto_set_reporter" => env_equal!(auto_set_reporter),
        "auto_assign_on_status" => env_equal!(auto_assign_on_status),
        "auto_codeowners_assign" => env_equal!(auto_codeowners_assign),
        "auto_tags_from_path" => env_equal!(auto_tags_from_path),
        "auto_branch_infer_type" => env_equal!(auto_branch_infer_type),
        "auto_branch_infer_status" => env_equal!(auto_branch_infer_status),
        "auto_branch_infer_priority" => env_equal!(auto_branch_infer_priority),
        "auto_identity" => env_equal!(auto_identity),
        "auto_identity_git" => env_equal!(auto_identity_git),
        "branch_type_aliases" => env_equal!(branch_type_aliases),
        "branch_status_aliases" => env_equal!(branch_status_aliases),
        "branch_priority_aliases" => env_equal!(branch_priority_aliases),
        "sprints_defaults_capacity_points" => {
            env_equal!(sprint_defaults.capacity_points)
        }
        "sprints_defaults_capacity_hours" => {
            env_equal!(sprint_defaults.capacity_hours)
        }
        "sprints_defaults_length" => env_equal!(sprint_defaults.length),
        "sprints_defaults_overdue_after" => env_equal!(sprint_defaults.overdue_after),
        "sprints_notifications_enabled" => env_equal!(sprint_notifications.enabled),
        _ => false,
    }
}

fn source_label_for_project(
    key: &str,
    resolved_project: &ResolvedConfig,
    project_cfg: Option<&ProjectConfig>,
    context: &GlobalScopeContext<'_>,
) -> String {
    let base_config = context.base_config;
    let global_cfg = context.global_cfg;
    let home_cfg = context.home_cfg;
    let env_resolved = context.env_resolved;
    let env_applied = context.env_applied;

    macro_rules! project_scope {
        ($key_literal:literal, $has_override:expr, $diff:expr) => {
            project_scope_with_global($key_literal, $has_override, $diff, context)
        };
    }

    match key {
        "default_project" => "project".to_string(),
        "default_assignee" => {
            let has_override = project_cfg
                .and_then(|pc| pc.default_assignee.as_ref())
                .is_some();
            let diff = resolved_project.default_assignee != base_config.default_assignee;
            project_scope!("default_assignee", has_override, diff)
        }
        "default_reporter" => {
            let has_override = project_cfg
                .and_then(|pc| pc.default_reporter.as_ref())
                .is_some();
            let diff = resolved_project.default_reporter != base_config.default_reporter;
            project_scope!("default_reporter", has_override, diff)
        }
        "default_priority" => {
            let has_override = project_cfg
                .and_then(|pc| pc.default_priority.as_ref())
                .is_some();
            let diff = resolved_project.default_priority != base_config.default_priority;
            project_scope!("default_priority", has_override, diff)
        }
        "default_status" => {
            let has_override = project_cfg
                .and_then(|pc| pc.default_status.as_ref())
                .is_some();
            let diff = resolved_project.default_status != base_config.default_status;
            project_scope!("default_status", has_override, diff)
        }
        "default_tags" => {
            let has_override = project_cfg
                .and_then(|pc| pc.default_tags.as_ref())
                .is_some();
            let diff = resolved_project.default_tags != base_config.default_tags;
            project_scope!("default_tags", has_override, diff)
        }
        "members" => {
            let has_override = project_cfg.and_then(|pc| pc.members.as_ref()).is_some();
            let diff = resolved_project.members != base_config.members;
            project_scope!("members", has_override, diff)
        }
        "strict_members" => {
            let has_override = project_cfg.and_then(|pc| pc.strict_members).is_some();
            let diff = resolved_project.strict_members != base_config.strict_members;
            project_scope!("strict_members", has_override, diff)
        }
        "auto_populate_members" => {
            let has_override = project_cfg
                .and_then(|pc| pc.auto_populate_members)
                .is_some();
            let diff = resolved_project.auto_populate_members != base_config.auto_populate_members;
            project_scope!("auto_populate_members", has_override, diff)
        }
        "custom_fields" => {
            let has_override = project_cfg
                .and_then(|pc| pc.custom_fields.as_ref())
                .map(|fields| fields.values == resolved_project.custom_fields.values)
                .unwrap_or(false);
            let diff = resolved_project.custom_fields.values != base_config.custom_fields.values;
            project_scope!("custom_fields", has_override, diff)
        }
        "issue_states" => {
            let has_override = project_cfg
                .and_then(|pc| pc.issue_states.as_ref())
                .is_some();
            let diff = resolved_project.issue_states.values != base_config.issue_states.values;
            project_scope!("issue_states", has_override, diff)
        }
        "issue_types" => {
            let has_override = project_cfg.and_then(|pc| pc.issue_types.as_ref()).is_some();
            let diff = resolved_project.issue_types.values != base_config.issue_types.values;
            project_scope!("issue_types", has_override, diff)
        }
        "issue_priorities" => {
            let has_override = project_cfg
                .and_then(|pc| pc.issue_priorities.as_ref())
                .is_some();
            let diff =
                resolved_project.issue_priorities.values != base_config.issue_priorities.values;
            project_scope!("issue_priorities", has_override, diff)
        }
        "issue_tags" => {
            let has_override = project_cfg
                .and_then(|pc| pc.tags.as_ref())
                .map(|tags| tags.values == resolved_project.tags.values)
                .unwrap_or(false);
            let diff = resolved_project.tags.values != base_config.tags.values;
            project_scope!("issue_tags", has_override, diff)
        }
        "scan_signal_words" => {
            let has_override = project_cfg
                .and_then(|pc| pc.scan_signal_words.as_ref())
                .is_some();
            let diff = resolved_project.scan_signal_words != base_config.scan_signal_words;
            project_scope!("scan_signal_words", has_override, diff)
        }
        "scan_ticket_patterns" => {
            let has_override = project_cfg
                .and_then(|pc| pc.scan_ticket_patterns.as_ref())
                .is_some();
            let diff = resolved_project.scan_ticket_patterns != base_config.scan_ticket_patterns;
            project_scope!("scan_ticket_patterns", has_override, diff)
        }
        "scan_enable_ticket_words" => {
            let has_override = project_cfg
                .and_then(|pc| pc.scan_enable_ticket_words)
                .is_some();
            let diff =
                resolved_project.scan_enable_ticket_words != base_config.scan_enable_ticket_words;
            project_scope!("scan_enable_ticket_words", has_override, diff)
        }
        "scan_enable_mentions" => {
            let has_override = project_cfg.and_then(|pc| pc.scan_enable_mentions).is_some();
            let diff = resolved_project.scan_enable_mentions != base_config.scan_enable_mentions;
            project_scope!("scan_enable_mentions", has_override, diff)
        }
        "scan_strip_attributes" => {
            let has_override = project_cfg
                .and_then(|pc| pc.scan_strip_attributes)
                .is_some();
            let diff = resolved_project.scan_strip_attributes != base_config.scan_strip_attributes;
            project_scope!("scan_strip_attributes", has_override, diff)
        }
        "auto_set_reporter" => {
            let has_override = project_cfg.and_then(|pc| pc.auto_set_reporter).is_some();
            let diff = resolved_project.auto_set_reporter != base_config.auto_set_reporter;
            project_scope!("auto_set_reporter", has_override, diff)
        }
        "auto_assign_on_status" => {
            let has_override = project_cfg
                .and_then(|pc| pc.auto_assign_on_status)
                .is_some();
            let diff = resolved_project.auto_assign_on_status != base_config.auto_assign_on_status;
            project_scope!("auto_assign_on_status", has_override, diff)
        }
        "auto_codeowners_assign"
        | "auto_tags_from_path"
        | "auto_branch_infer_type"
        | "auto_branch_infer_status"
        | "auto_branch_infer_priority"
        | "auto_identity"
        | "auto_identity_git"
        | "sprints_defaults_capacity_points"
        | "sprints_defaults_capacity_hours"
        | "sprints_defaults_length"
        | "sprints_defaults_overdue_after"
        | "sprints_notifications_enabled" => source_label_for_global(
            base_config,
            global_cfg,
            home_cfg,
            env_resolved,
            env_applied,
            key,
        )
        .to_string(),
        "branch_type_aliases" => {
            let has_override = project_cfg
                .and_then(|pc| pc.branch_type_aliases.as_ref())
                .is_some();
            let diff = resolved_project.branch_type_aliases != base_config.branch_type_aliases;
            project_scope!("branch_type_aliases", has_override, diff)
        }
        "branch_status_aliases" => {
            let has_override = project_cfg
                .and_then(|pc| pc.branch_status_aliases.as_ref())
                .is_some();
            let diff = resolved_project.branch_status_aliases != base_config.branch_status_aliases;
            project_scope!("branch_status_aliases", has_override, diff)
        }
        "branch_priority_aliases" => {
            let has_override = project_cfg
                .and_then(|pc| pc.branch_priority_aliases.as_ref())
                .is_some();
            let diff =
                resolved_project.branch_priority_aliases != base_config.branch_priority_aliases;
            project_scope!("branch_priority_aliases", has_override, diff)
        }
        "server_port" => source_label_for_global(
            base_config,
            global_cfg,
            home_cfg,
            env_resolved,
            env_applied,
            key,
        )
        .to_string(),
        _ => source_label_for_global(
            base_config,
            global_cfg,
            home_cfg,
            env_resolved,
            env_applied,
            key,
        )
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[test]
    fn populate_source_labels_invokes_resolver_for_each_entry() {
        let seen = RefCell::new(Vec::new());
        let mut labels = HashMap::new();

        populate_source_labels(&mut labels, |entry| {
            seen.borrow_mut().push(entry.inspect_key);
            format!("src:{}", entry.label_key)
        });

        assert_eq!(labels.len(), CONFIG_SOURCE_ENTRIES.len());
        for entry in CONFIG_SOURCE_ENTRIES {
            let expected = format!("src:{}", entry.label_key);
            assert_eq!(labels.get(entry.path), Some(&expected));
        }

        assert_eq!(seen.into_inner().len(), CONFIG_SOURCE_ENTRIES.len());
    }

    #[test]
    fn collapse_label_to_scope_maps_labels() {
        assert_eq!(collapse_label_to_scope("project"), "project");
        assert_eq!(collapse_label_to_scope("global"), "global");
        assert_eq!(collapse_label_to_scope("home"), "global");
        assert_eq!(collapse_label_to_scope("env"), "global");
        assert_eq!(collapse_label_to_scope("default"), "built_in");
        assert_eq!(collapse_label_to_scope("anything_else"), "built_in");
    }
}
