//! Single catalog of configuration field metadata (DEV-113).
//!
//! Each entry describes one canonical flat field: where `config set` may
//! write it, which dotted spellings are accepted for it, how `config inspect`
//! labels its source, and which validation passes it affects. The typed
//! setters in `operations.rs` stay the only place that interprets values;
//! this table only answers "which names exist and where".
//!
//! Adding a dotted alias here makes it accepted both when loading config
//! files and by `config set`, so new spellings are an explicit, reviewed
//! change rather than a side effect of merging lists.

use std::sync::LazyLock;

#[derive(Clone, Copy, Debug)]
pub struct ConfigFieldMeta {
    /// Canonical flat name (the typed schema key).
    pub flat: &'static str,
    /// Settable through global `config set`.
    pub global: bool,
    /// Settable through project `config set`.
    pub project: bool,
    /// Dotted spellings, applied in this order when loading config files
    /// (a later alias overrides an earlier one for the same field).
    pub aliases: &'static [&'static str],
    /// `config inspect` source label: (dotted label path, label key).
    pub source: Option<(&'static str, &'static str)>,
    /// Participates in resolved-config validation, so a global change needs
    /// per-project revalidation.
    pub resolved_relevant: bool,
    /// Changing it can invalidate existing tasks.
    pub task_conflict: bool,
}

const fn field(flat: &'static str) -> ConfigFieldMeta {
    ConfigFieldMeta {
        flat,
        global: false,
        project: false,
        aliases: &[],
        source: None,
        resolved_relevant: false,
        task_conflict: false,
    }
}

impl ConfigFieldMeta {
    const fn both(mut self) -> Self {
        self.global = true;
        self.project = true;
        self
    }
    const fn global(mut self) -> Self {
        self.global = true;
        self
    }
    const fn project(mut self) -> Self {
        self.project = true;
        self
    }
    const fn aliases(mut self, aliases: &'static [&'static str]) -> Self {
        self.aliases = aliases;
        self
    }
    /// Source label whose label key is the flat name.
    const fn source(mut self, path: &'static str) -> Self {
        self.source = Some((path, self.flat));
        self
    }
    const fn source_as(mut self, path: &'static str, label_key: &'static str) -> Self {
        self.source = Some((path, label_key));
        self
    }
    const fn resolved(mut self) -> Self {
        self.resolved_relevant = true;
        self
    }
    const fn task_conflict(mut self) -> Self {
        self.task_conflict = true;
        self
    }
}

/// The catalog. Order is the order of the global settable-field listing.
/// Fields with neither scope are load-only (aliases) or inspect-only.
pub const CONFIG_FIELDS: &[ConfigFieldMeta] = &[
    field("project_name")
        .project()
        .aliases(&["config.project_name", "project.id", "project.name"]),
    field("review_reviewers")
        .project()
        .aliases(&["review.reviewers"])
        .source("review.reviewers"),
    field("server_port")
        .global()
        .aliases(&["server.port"])
        .source("server.port"),
    field("default_project")
        .global()
        .aliases(&["default.project"])
        .source("default.project"),
    field("default_assignee")
        .both()
        .aliases(&["default.assignee"])
        .source("default.assignee"),
    field("default_reporter")
        .both()
        .aliases(&["default.reporter"])
        .source("default.reporter"),
    field("default_tags")
        .both()
        .aliases(&["default.tags"])
        .source("default.tags")
        .resolved(),
    field("members")
        .both()
        .aliases(&["default.members", "members"])
        .source("members")
        .resolved(),
    field("strict_members")
        .both()
        .aliases(&["default.strict_members"])
        .source("default.strict-members")
        .resolved(),
    field("auto_populate_members")
        .both()
        .aliases(&["auto.populate_members"])
        .source("auto.populate-members"),
    field("default_priority")
        .both()
        .aliases(&["default.priority"])
        .source("default.priority")
        .resolved(),
    field("default_status")
        .both()
        .aliases(&["default.status"])
        .source("default.status")
        .resolved(),
    field("tags")
        .both()
        .aliases(&["taxonomy.tags", "issue.tags"])
        .source_as("issue.tags", "issue_tags")
        .resolved(),
    field("custom_fields")
        .both()
        .aliases(&["custom.fields"])
        .source("custom.fields"),
    field("issue_states")
        .both()
        .aliases(&["issue.states"])
        .source("issue.states")
        .resolved()
        .task_conflict(),
    field("issue_done_states")
        .both()
        .aliases(&["issue.done_states"])
        .source("issue.done_states")
        .resolved(),
    field("issue_types")
        .both()
        .aliases(&["issue.types"])
        .source("issue.types")
        .resolved()
        .task_conflict(),
    field("issue_priorities")
        .both()
        .aliases(&["issue.priorities"])
        .source("issue.priorities")
        .resolved()
        .task_conflict(),
    field("auto_set_reporter")
        .both()
        .aliases(&["auto.set_reporter"])
        .source("auto.set-reporter"),
    field("auto_assign_on_status")
        .both()
        .aliases(&["auto.assign_on_status"])
        .source("auto.assign-on-status"),
    field("auto_codeowners_assign")
        .both()
        .aliases(&["auto.codeowners_assign"])
        .source("auto.codeowners-assign"),
    field("auto_tags_from_path")
        .both()
        .aliases(&["auto.tags_from_path"])
        .source("auto.tags-from-path"),
    field("auto_branch_infer_type")
        .both()
        .aliases(&["auto.branch_infer_type"])
        .source("auto.branch-infer-type"),
    field("auto_branch_infer_status")
        .both()
        .aliases(&["auto.branch_infer_status"])
        .source("auto.branch-infer-status"),
    field("auto_branch_infer_priority")
        .both()
        .aliases(&["auto.branch_infer_priority"])
        .source("auto.branch-infer-priority"),
    field("auto_identity")
        .both()
        .aliases(&["auto.identity"])
        .source("auto.identity"),
    field("auto_identity_git")
        .both()
        .aliases(&["auto.identity_git"])
        .source("auto.identity-git"),
    field("scan_signal_words")
        .both()
        .aliases(&["scan.signal_words"])
        .source("scan.signal-words"),
    field("scan_ticket_patterns")
        .both()
        .aliases(&["scan.ticket_patterns"])
        .source("scan.ticket-patterns"),
    field("scan_enable_ticket_words")
        .both()
        .aliases(&["scan.enable_ticket_words"])
        .source("scan.enable-ticket-words"),
    field("scan_enable_mentions")
        .both()
        .aliases(&["scan.enable_mentions"])
        .source("scan.enable-mentions"),
    field("scan_strip_attributes")
        .both()
        .aliases(&["scan.strip_attributes"])
        .source("scan.strip-attributes"),
    field("branch_type_aliases")
        .both()
        .aliases(&["branch.type_aliases"])
        .source("branch.type-aliases")
        .resolved(),
    field("branch_status_aliases")
        .both()
        .aliases(&["branch.status_aliases"])
        .source("branch.status-aliases")
        .resolved(),
    field("branch_priority_aliases")
        .both()
        .aliases(&["branch.priority_aliases"])
        .source("branch.priority-aliases")
        .resolved(),
    field("attachments_dir")
        .both()
        .aliases(&["attachments.dir"])
        .source("attachments.dir"),
    field("attachments_max_upload_mb")
        .both()
        .aliases(&["attachments.max_upload_mb"])
        .source("attachments.max_upload_mb"),
    field("sync_reports_dir")
        .both()
        .aliases(&["sync.reports_dir"]),
    field("sync_write_reports")
        .both()
        .aliases(&["sync.write_reports"]),
    field("agent_context_enabled")
        .both()
        .aliases(&["agent.context_enabled"])
        .source("agent.context-enabled"),
    field("agent_context_extension")
        .both()
        .aliases(&["agent.context_extension"]),
    field("agent_logs_dir").both().aliases(&["agent.logs_dir"]),
    field("agent_instructions")
        .both()
        .aliases(&["agent.instructions"])
        .source("agent.instructions"),
    field("agent_instructions_file").both(),
    field("agent_on_start_status").both(),
    field("agent_on_success_status").both(),
    field("agent_on_failure_status").both(),
    field("agent_on_cancel_status").both(),
    field("agent_on_start_reassign_to").both(),
    field("agent_on_success_reassign_to").both(),
    field("agent_on_failure_reassign_to").both(),
    field("agent_on_cancel_reassign_to").both(),
    field("agent_worktree_enabled")
        .both()
        .source("agent.worktree.enabled"),
    field("agent_worktree_dir")
        .both()
        .source("agent.worktree.dir"),
    field("agent_worktree_branch_prefix")
        .both()
        .source("agent.worktree.branch_prefix"),
    field("agent_worktree_cleanup_on_done").source("agent.worktree.cleanup_on_done"),
    field("agent_worktree_cleanup_on_failure").source("agent.worktree.cleanup_on_failure"),
    field("agent_worktree_cleanup_on_cancel").source("agent.worktree.cleanup_on_cancel"),
    field("agent_worktree_cleanup_delete_branches")
        .source("agent.worktree.cleanup_delete_branches"),
    field("agent_automation").aliases(&["agent.automation"]),
    field("agent_worktree").aliases(&["agent.worktree"]),
    field("remotes").both().aliases(&["sync.remotes"]),
    field("agents").both(),
    field("auth_profiles").aliases(&["sync.auth_profiles"]),
    field("sprints_defaults_capacity_points")
        .global()
        .source("sprints.defaults.capacity_points"),
    field("sprints_defaults_capacity_hours")
        .global()
        .source("sprints.defaults.capacity_hours"),
    field("sprints_defaults_length")
        .global()
        .source("sprints.defaults.length"),
    field("sprints_defaults_overdue_after")
        .global()
        .source("sprints.defaults.overdue_after"),
    field("sprints_notifications_enabled")
        .global()
        .source("sprints.notifications.enabled"),
    field("web_ui_path").global(),
];

static GLOBAL_SETTABLE: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    CONFIG_FIELDS
        .iter()
        .filter(|meta| meta.global)
        .map(|meta| meta.flat)
        .collect()
});

static PROJECT_SETTABLE: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    CONFIG_FIELDS
        .iter()
        .filter(|meta| meta.project)
        .map(|meta| meta.flat)
        .collect()
});

/// Canonical flat names settable through `config set` in the given scope.
pub fn settable_fields(is_global: bool) -> &'static [&'static str] {
    if is_global {
        &GLOBAL_SETTABLE
    } else {
        &PROJECT_SETTABLE
    }
}

/// Every (dotted alias, flat field) pair, in config-file application order.
pub fn alias_paths() -> impl Iterator<Item = (&'static str, &'static str)> {
    CONFIG_FIELDS
        .iter()
        .flat_map(|meta| meta.aliases.iter().map(move |alias| (*alias, meta.flat)))
}

/// Flat field a dotted alias maps to, if any.
pub fn flat_for_alias(alias: &str) -> Option<&'static str> {
    alias_paths()
        .find(|(dotted, _)| *dotted == alias)
        .map(|(_, flat)| flat)
}

fn lookup(flat: &str) -> Option<&'static ConfigFieldMeta> {
    CONFIG_FIELDS.iter().find(|meta| meta.flat == flat)
}

pub fn is_resolved_relevant(flat: &str) -> bool {
    lookup(flat).is_some_and(|meta| meta.resolved_relevant)
}

pub fn is_task_conflict_field(flat: &str) -> bool {
    lookup(flat).is_some_and(|meta| meta.task_conflict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    // Literal catalogs as they stood before DEV-113. The derived views must
    // keep accepting and rejecting exactly these names.
    const LEGACY_GLOBAL: &[&str] = &[
        "server_port",
        "default_project",
        "default_assignee",
        "default_reporter",
        "default_tags",
        "members",
        "strict_members",
        "auto_populate_members",
        "default_priority",
        "default_status",
        "tags",
        "custom_fields",
        "issue_states",
        "issue_done_states",
        "issue_types",
        "issue_priorities",
        "auto_set_reporter",
        "auto_assign_on_status",
        "auto_codeowners_assign",
        "auto_tags_from_path",
        "auto_branch_infer_type",
        "auto_branch_infer_status",
        "auto_branch_infer_priority",
        "auto_identity",
        "auto_identity_git",
        "scan_signal_words",
        "scan_ticket_patterns",
        "scan_enable_ticket_words",
        "scan_enable_mentions",
        "scan_strip_attributes",
        "branch_type_aliases",
        "branch_status_aliases",
        "branch_priority_aliases",
        "attachments_dir",
        "attachments_max_upload_mb",
        "sync_reports_dir",
        "sync_write_reports",
        "agent_context_enabled",
        "agent_context_extension",
        "agent_logs_dir",
        "agent_instructions",
        "agent_instructions_file",
        "agent_on_start_status",
        "agent_on_success_status",
        "agent_on_failure_status",
        "agent_on_cancel_status",
        "agent_on_start_reassign_to",
        "agent_on_success_reassign_to",
        "agent_on_failure_reassign_to",
        "agent_on_cancel_reassign_to",
        "agent_worktree_enabled",
        "agent_worktree_dir",
        "agent_worktree_branch_prefix",
        "remotes",
        "agents",
        "sprints_defaults_capacity_points",
        "sprints_defaults_capacity_hours",
        "sprints_defaults_length",
        "sprints_defaults_overdue_after",
        "sprints_notifications_enabled",
        "web_ui_path",
    ];

    const LEGACY_PROJECT: &[&str] = &[
        "project_name",
        "review_reviewers",
        "default_assignee",
        "default_reporter",
        "default_tags",
        "members",
        "strict_members",
        "auto_populate_members",
        "default_priority",
        "default_status",
        "issue_states",
        "issue_done_states",
        "issue_types",
        "issue_priorities",
        "tags",
        "custom_fields",
        "auto_set_reporter",
        "auto_assign_on_status",
        "auto_codeowners_assign",
        "auto_tags_from_path",
        "auto_branch_infer_type",
        "auto_branch_infer_status",
        "auto_branch_infer_priority",
        "auto_identity",
        "auto_identity_git",
        "scan_signal_words",
        "scan_ticket_patterns",
        "scan_enable_ticket_words",
        "scan_enable_mentions",
        "scan_strip_attributes",
        "branch_type_aliases",
        "branch_status_aliases",
        "branch_priority_aliases",
        "attachments_dir",
        "attachments_max_upload_mb",
        "sync_reports_dir",
        "sync_write_reports",
        "agent_context_enabled",
        "agent_context_extension",
        "agent_logs_dir",
        "agent_instructions",
        "agent_instructions_file",
        "agent_on_start_status",
        "agent_on_success_status",
        "agent_on_failure_status",
        "agent_on_cancel_status",
        "agent_on_start_reassign_to",
        "agent_on_success_reassign_to",
        "agent_on_failure_reassign_to",
        "agent_on_cancel_reassign_to",
        "agent_worktree_enabled",
        "agent_worktree_dir",
        "agent_worktree_branch_prefix",
        "remotes",
        "agents",
    ];

    const LEGACY_CONFIG_PATHS: &[(&str, &str)] = &[
        ("config.project_name", "project_name"),
        ("project.id", "project_name"),
        ("project.name", "project_name"),
        ("server.port", "server_port"),
        ("default.project", "default_project"),
        ("default.assignee", "default_assignee"),
        ("default.reporter", "default_reporter"),
        ("default.tags", "default_tags"),
        ("default.members", "members"),
        ("members", "members"),
        ("review.reviewers", "review_reviewers"),
        ("default.strict_members", "strict_members"),
        ("default.priority", "default_priority"),
        ("default.status", "default_status"),
        ("taxonomy.tags", "tags"),
        ("issue.tags", "tags"),
        ("issue.states", "issue_states"),
        ("issue.done_states", "issue_done_states"),
        ("issue.types", "issue_types"),
        ("issue.priorities", "issue_priorities"),
        ("custom.fields", "custom_fields"),
        ("scan.signal_words", "scan_signal_words"),
        ("scan.ticket_patterns", "scan_ticket_patterns"),
        ("scan.enable_ticket_words", "scan_enable_ticket_words"),
        ("scan.enable_mentions", "scan_enable_mentions"),
        ("scan.strip_attributes", "scan_strip_attributes"),
        ("auto.identity", "auto_identity"),
        ("auto.identity_git", "auto_identity_git"),
        ("auto.populate_members", "auto_populate_members"),
        ("auto.set_reporter", "auto_set_reporter"),
        ("auto.assign_on_status", "auto_assign_on_status"),
        ("auto.codeowners_assign", "auto_codeowners_assign"),
        ("auto.tags_from_path", "auto_tags_from_path"),
        ("auto.branch_infer_type", "auto_branch_infer_type"),
        ("auto.branch_infer_status", "auto_branch_infer_status"),
        ("auto.branch_infer_priority", "auto_branch_infer_priority"),
        ("branch.type_aliases", "branch_type_aliases"),
        ("branch.status_aliases", "branch_status_aliases"),
        ("branch.priority_aliases", "branch_priority_aliases"),
        ("attachments.dir", "attachments_dir"),
        ("attachments.max_upload_mb", "attachments_max_upload_mb"),
        ("sync.reports_dir", "sync_reports_dir"),
        ("sync.write_reports", "sync_write_reports"),
        ("agent.context_enabled", "agent_context_enabled"),
        ("agent.context_extension", "agent_context_extension"),
        ("agent.logs_dir", "agent_logs_dir"),
        ("agent.instructions", "agent_instructions"),
        ("agent.automation", "agent_automation"),
        ("agent.worktree", "agent_worktree"),
        ("sync.remotes", "remotes"),
        ("sync.auth_profiles", "auth_profiles"),
    ];

    // (label path, label key, inspect key)
    const LEGACY_SOURCE_ENTRIES: &[(&str, &str, &str)] = &[
        ("server.port", "server_port", "server_port"),
        ("default.project", "default_project", "default_project"),
        ("default.assignee", "default_assignee", "default_assignee"),
        ("default.reporter", "default_reporter", "default_reporter"),
        ("default.tags", "default_tags", "default_tags"),
        ("members", "members", "members"),
        ("review.reviewers", "review_reviewers", "review_reviewers"),
        ("default.strict-members", "strict_members", "strict_members"),
        ("default.priority", "default_priority", "default_priority"),
        ("default.status", "default_status", "default_status"),
        ("issue.states", "issue_states", "issue_states"),
        (
            "issue.done_states",
            "issue_done_states",
            "issue_done_states",
        ),
        ("issue.types", "issue_types", "issue_types"),
        ("issue.priorities", "issue_priorities", "issue_priorities"),
        ("issue.tags", "issue_tags", "tags"),
        ("custom.fields", "custom_fields", "custom_fields"),
        (
            "scan.signal-words",
            "scan_signal_words",
            "scan_signal_words",
        ),
        (
            "scan.ticket-patterns",
            "scan_ticket_patterns",
            "scan_ticket_patterns",
        ),
        (
            "scan.enable-ticket-words",
            "scan_enable_ticket_words",
            "scan_enable_ticket_words",
        ),
        (
            "scan.enable-mentions",
            "scan_enable_mentions",
            "scan_enable_mentions",
        ),
        (
            "scan.strip-attributes",
            "scan_strip_attributes",
            "scan_strip_attributes",
        ),
        (
            "auto.populate-members",
            "auto_populate_members",
            "auto_populate_members",
        ),
        (
            "auto.set-reporter",
            "auto_set_reporter",
            "auto_set_reporter",
        ),
        (
            "auto.assign-on-status",
            "auto_assign_on_status",
            "auto_assign_on_status",
        ),
        (
            "auto.codeowners-assign",
            "auto_codeowners_assign",
            "auto_codeowners_assign",
        ),
        (
            "auto.tags-from-path",
            "auto_tags_from_path",
            "auto_tags_from_path",
        ),
        (
            "auto.branch-infer-type",
            "auto_branch_infer_type",
            "auto_branch_infer_type",
        ),
        (
            "auto.branch-infer-status",
            "auto_branch_infer_status",
            "auto_branch_infer_status",
        ),
        (
            "auto.branch-infer-priority",
            "auto_branch_infer_priority",
            "auto_branch_infer_priority",
        ),
        ("auto.identity", "auto_identity", "auto_identity"),
        (
            "auto.identity-git",
            "auto_identity_git",
            "auto_identity_git",
        ),
        ("attachments.dir", "attachments_dir", "attachments_dir"),
        (
            "attachments.max_upload_mb",
            "attachments_max_upload_mb",
            "attachments_max_upload_mb",
        ),
        (
            "sprints.defaults.capacity_points",
            "sprints_defaults_capacity_points",
            "sprints_defaults_capacity_points",
        ),
        (
            "sprints.defaults.capacity_hours",
            "sprints_defaults_capacity_hours",
            "sprints_defaults_capacity_hours",
        ),
        (
            "sprints.defaults.length",
            "sprints_defaults_length",
            "sprints_defaults_length",
        ),
        (
            "sprints.defaults.overdue_after",
            "sprints_defaults_overdue_after",
            "sprints_defaults_overdue_after",
        ),
        (
            "sprints.notifications.enabled",
            "sprints_notifications_enabled",
            "sprints_notifications_enabled",
        ),
        (
            "agent.context-enabled",
            "agent_context_enabled",
            "agent_context_enabled",
        ),
        (
            "agent.instructions",
            "agent_instructions",
            "agent_instructions",
        ),
        (
            "agent.worktree.enabled",
            "agent_worktree_enabled",
            "agent_worktree_enabled",
        ),
        (
            "agent.worktree.dir",
            "agent_worktree_dir",
            "agent_worktree_dir",
        ),
        (
            "agent.worktree.branch_prefix",
            "agent_worktree_branch_prefix",
            "agent_worktree_branch_prefix",
        ),
        (
            "agent.worktree.cleanup_on_done",
            "agent_worktree_cleanup_on_done",
            "agent_worktree_cleanup_on_done",
        ),
        (
            "agent.worktree.cleanup_on_failure",
            "agent_worktree_cleanup_on_failure",
            "agent_worktree_cleanup_on_failure",
        ),
        (
            "agent.worktree.cleanup_on_cancel",
            "agent_worktree_cleanup_on_cancel",
            "agent_worktree_cleanup_on_cancel",
        ),
        (
            "agent.worktree.cleanup_delete_branches",
            "agent_worktree_cleanup_delete_branches",
            "agent_worktree_cleanup_delete_branches",
        ),
        (
            "branch.type-aliases",
            "branch_type_aliases",
            "branch_type_aliases",
        ),
        (
            "branch.status-aliases",
            "branch_status_aliases",
            "branch_status_aliases",
        ),
        (
            "branch.priority-aliases",
            "branch_priority_aliases",
            "branch_priority_aliases",
        ),
    ];

    fn set<'a>(items: impl IntoIterator<Item = &'a str>) -> BTreeSet<&'a str> {
        items.into_iter().collect()
    }

    #[test]
    fn global_settable_fields_match_legacy_list_in_order() {
        assert_eq!(settable_fields(true), LEGACY_GLOBAL);
    }

    #[test]
    fn project_settable_fields_match_legacy_list() {
        let derived = settable_fields(false);
        assert_eq!(
            derived.len(),
            LEGACY_PROJECT.len(),
            "no duplicates or extras"
        );
        assert_eq!(
            set(derived.iter().copied()),
            set(LEGACY_PROJECT.iter().copied())
        );
    }

    #[test]
    fn alias_paths_match_legacy_table_with_per_field_order() {
        let derived: Vec<_> = alias_paths().collect();
        let mut expected: Vec<_> = LEGACY_CONFIG_PATHS.to_vec();
        let mut actual = derived.clone();
        expected.sort();
        actual.sort();
        assert_eq!(actual, expected);
        // Later aliases override earlier ones for the same field while
        // loading, so the relative order within a field must be preserved.
        let order = |pairs: &[(&'static str, &'static str)]| {
            let mut by_field: HashMap<&str, Vec<&str>> = HashMap::new();
            for (alias, flat) in pairs {
                by_field.entry(flat).or_default().push(alias);
            }
            by_field
        };
        assert_eq!(order(&derived), order(LEGACY_CONFIG_PATHS));
    }

    #[test]
    fn source_entries_match_legacy_table() {
        let derived: BTreeSet<(&str, &str, &str)> = CONFIG_FIELDS
            .iter()
            .filter_map(|meta| meta.source.map(|(path, label)| (path, label, meta.flat)))
            .collect();
        let expected: BTreeSet<_> = LEGACY_SOURCE_ENTRIES.iter().copied().collect();
        assert_eq!(derived, expected);
    }

    #[test]
    fn validation_flags_match_legacy_sets() {
        let resolved: BTreeSet<_> = CONFIG_FIELDS
            .iter()
            .filter(|meta| meta.resolved_relevant)
            .map(|meta| meta.flat)
            .collect();
        assert_eq!(
            resolved,
            set([
                "issue_states",
                "issue_done_states",
                "issue_types",
                "issue_priorities",
                "tags",
                "default_tags",
                "default_priority",
                "default_status",
                "members",
                "strict_members",
                "branch_type_aliases",
                "branch_status_aliases",
                "branch_priority_aliases",
            ])
        );
        let conflicts: BTreeSet<_> = CONFIG_FIELDS
            .iter()
            .filter(|meta| meta.task_conflict)
            .map(|meta| meta.flat)
            .collect();
        assert_eq!(
            conflicts,
            set(["issue_states", "issue_types", "issue_priorities"])
        );
    }

    #[test]
    fn config_set_accepts_catalog_names_only_in_their_scopes() {
        use crate::config::operations::{canonicalize_field_name, is_global_only_field_name};
        for meta in CONFIG_FIELDS {
            for (is_global, allowed) in [(true, meta.global), (false, meta.project)] {
                for name in std::iter::once(meta.flat).chain(meta.aliases.iter().copied()) {
                    let result = canonicalize_field_name(name, is_global);
                    if allowed {
                        assert_eq!(
                            result.ok().as_deref(),
                            Some(meta.flat),
                            "{name} global={is_global}"
                        );
                    } else {
                        assert!(
                            result.is_err(),
                            "{name} must be rejected for global={is_global}"
                        );
                    }
                }
            }
            let global_only = meta.global && !meta.project;
            assert_eq!(
                is_global_only_field_name(meta.flat).is_some(),
                global_only,
                "{} global-only promotion",
                meta.flat
            );
            // Inspect label paths are display names, not accepted spellings,
            // unless they are also listed as aliases.
            if let Some((path, _)) = meta.source
                && path != meta.flat
                && !meta.aliases.contains(&path)
            {
                assert!(canonicalize_field_name(path, true).is_err(), "{path}");
                assert!(canonicalize_field_name(path, false).is_err(), "{path}");
            }
        }
    }

    #[test]
    fn catalog_invariants_hold() {
        let mut flats = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for meta in CONFIG_FIELDS {
            assert!(flats.insert(meta.flat), "duplicate field {}", meta.flat);
            for alias in meta.aliases {
                assert!(aliases.insert(*alias), "alias {alias} maps to two fields");
            }
            if let Some((path, _)) = meta.source {
                assert!(paths.insert(path), "duplicate source path {path}");
            }
            // Task-conflict fields are always revalidated.
            assert!(!meta.task_conflict || meta.resolved_relevant);
            // Every field is reachable somehow.
            assert!(
                meta.global || meta.project || !meta.aliases.is_empty() || meta.source.is_some(),
                "{} has no role",
                meta.flat
            );
        }
    }
}
