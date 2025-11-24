use crate::config::types::{GlobalConfig, ProjectConfig, ResolvedConfig, StringConfigField};
use crate::config::validation::conflicts::PrefixConflictDetector;
use crate::config::validation::errors::{ValidationError, ValidationResult};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct ConfigValidator {
    tasks_dir: std::path::PathBuf,
}

const MAX_PREFIX_LENGTH: usize = 20;
const LONG_PREFIX_WARNING_THRESHOLD: usize = 12;

impl ConfigValidator {
    pub fn new(tasks_dir: &Path) -> Self {
        Self {
            tasks_dir: tasks_dir.to_path_buf(),
        }
    }

    pub fn validate_project_config(&self, config: &ProjectConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate project name
        self.validate_project_name(&config.project_name, &mut result);

        // Note: Project prefix validation is done separately as it's not stored in ProjectConfig

        if let Some(states) = &config.issue_states {
            if states.values.is_empty() {
                result.add_error(
                    ValidationError::error(
                        Some("issue_states".to_string()),
                        "Issue states override cannot be empty".to_string(),
                    )
                    .with_fix(
                        "Remove the override to inherit global statuses or add at least one state"
                            .to_string(),
                    ),
                );
            } else {
                self.warn_on_duplicate_values(
                    "issue_states",
                    states.values.iter().map(|v| v.as_str()),
                    &mut result,
                );
            }
        }

        if let Some(types) = &config.issue_types {
            if types.values.is_empty() {
                result.add_error(
                    ValidationError::error(
                        Some("issue_types".to_string()),
                        "Issue types override cannot be empty".to_string(),
                    )
                    .with_fix(
                        "Remove the override to inherit global types or add at least one type"
                            .to_string(),
                    ),
                );
            } else {
                self.warn_on_duplicate_values(
                    "issue_types",
                    types.values.iter().map(|v| v.as_str()),
                    &mut result,
                );
            }
        }

        if let Some(priorities) = &config.issue_priorities {
            if priorities.values.is_empty() {
                result.add_error(
                    ValidationError::error(
                        Some("issue_priorities".to_string()),
                        "Issue priorities override cannot be empty".to_string(),
                    )
                    .with_fix(
                        "Remove the override to inherit global priorities or add at least one priority"
                            .to_string(),
                    ),
                );
            } else {
                self.warn_on_duplicate_values(
                    "issue_priorities",
                    priorities.values.iter().map(|v| v.as_str()),
                    &mut result,
                );
            }
        }

        // Validate that defaults exist in their respective lists
        self.validate_defaults_consistency(config, &mut result);

        // Validate enum values
        self.validate_enum_fields(config, &mut result);

        // Validate field formats
        self.validate_field_formats(config, &mut result);

        // Validate scan.ticket_patterns (if present)
        if let Some(patterns) = &config.scan_ticket_patterns {
            self.validate_ticket_patterns(patterns, &mut result);
        }

        if config.strict_members.unwrap_or(false)
            && let Some(members) = &config.members
            && !Self::members_list_has_entries(members)
        {
            result.add_error(
                ValidationError::error(
                    Some("members".to_string()),
                    "strict_members is enabled but no members are configured".to_string(),
                )
                .with_fix("Add at least one member entry or disable strict_members".to_string()),
            );
        }

        if let (Some(default_tags), Some(tags)) = (&config.default_tags, &config.tags) {
            self.validate_default_tags_subset(
                "default_tags",
                default_tags,
                tags,
                "issue.tags",
                &mut result,
            );
        }

        if let (Some(aliases), Some(states)) = (&config.branch_status_aliases, &config.issue_states)
        {
            self.validate_alias_targets(
                "branch_status_aliases",
                "issue_states",
                aliases,
                &states.values,
                |alias, allowed| alias.eq_ignore_case(allowed.as_str()),
                &mut result,
            );
        }

        if let (Some(aliases), Some(types)) = (&config.branch_type_aliases, &config.issue_types) {
            self.validate_alias_targets(
                "branch_type_aliases",
                "issue_types",
                aliases,
                &types.values,
                |alias, allowed| alias.eq_ignore_case(allowed.as_str()),
                &mut result,
            );
        }

        if let (Some(aliases), Some(priorities)) =
            (&config.branch_priority_aliases, &config.issue_priorities)
        {
            self.validate_alias_targets(
                "branch_priority_aliases",
                "issue_priorities",
                aliases,
                &priorities.values,
                |alias, allowed| alias.eq_ignore_case(allowed.as_str()),
                &mut result,
            );
        }

        result
    }

    pub fn validate_global_config(&self, config: &GlobalConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate server port
        if config.server_port < 1024 {
            result.add_error(
                ValidationError::warning(
                    Some("server_port".to_string()),
                    format!(
                        "Port {} may require elevated privileges",
                        config.server_port
                    ),
                )
                .with_fix("Consider using a port >= 1024".to_string()),
            );
        }

        // Note: u16 max value is 65535, so no need to check config.server_port > 65535

        // Validate default prefix
        if !config.default_project.is_empty() {
            self.validate_prefix(&config.default_project, &mut result);
        }

        self.warn_on_duplicate_values(
            "issue_states",
            config.issue_states.values.iter().map(|v| v.as_str()),
            &mut result,
        );
        self.warn_on_duplicate_values(
            "issue_types",
            config.issue_types.values.iter().map(|v| v.as_str()),
            &mut result,
        );
        self.warn_on_duplicate_values(
            "issue_priorities",
            config.issue_priorities.values.iter().map(|v| v.as_str()),
            &mut result,
        );
        self.warn_on_duplicate_values(
            "default_tags",
            config.default_tags.iter().map(|v| v.as_str()),
            &mut result,
        );
        self.warn_on_duplicate_values(
            "scan_signal_words",
            config.scan_signal_words.iter().map(|v| v.as_str()),
            &mut result,
        );
        self.warn_on_duplicate_values(
            "tags",
            config.tags.values.iter().map(|v| v.as_str()),
            &mut result,
        );
        self.warn_on_duplicate_values(
            "custom_fields",
            config.custom_fields.values.iter().map(|v| v.as_str()),
            &mut result,
        );

        if config.strict_members && !Self::members_list_has_entries(&config.members) {
            result.add_error(
                ValidationError::error(
                    Some("members".to_string()),
                    "strict_members is enabled globally but no members are configured".to_string(),
                )
                .with_fix("Add entries under members or disable strict_members".to_string()),
            );
        }

        self.validate_default_tags_subset(
            "default_tags",
            &config.default_tags,
            &config.tags,
            "issue.tags",
            &mut result,
        );

        // Validate that lists are not empty
        if config.issue_states.values.is_empty() {
            result.add_error(
                ValidationError::error(
                    Some("issue_states".to_string()),
                    "Issue states list cannot be empty".to_string(),
                )
                .with_fix("Add at least one status like 'todo', 'in_progress', 'done'".to_string()),
            );
        }

        if config.issue_types.values.is_empty() {
            result.add_error(
                ValidationError::error(
                    Some("issue_types".to_string()),
                    "Issue types list cannot be empty".to_string(),
                )
                .with_fix("Add at least one type like 'feature', 'bug', 'chore'".to_string()),
            );
        }

        if config.issue_priorities.values.is_empty() {
            result.add_error(
                ValidationError::error(
                    Some("issue_priorities".to_string()),
                    "Issue priorities list cannot be empty".to_string(),
                )
                .with_fix("Add at least one priority like 'low', 'medium', 'high'".to_string()),
            );
        }

        // Validate default values exist in lists
        if let Some(default_status) = &config.default_status
            && !config.issue_states.values.contains(default_status)
        {
            result.add_error(
                ValidationError::warning(
                    Some("default_status".to_string()),
                    format!(
                        "Default status '{}' not found in issue_states list",
                        default_status
                    ),
                )
                .with_fix(
                    "Add the status to issue_states or choose a different default".to_string(),
                ),
            );
        }

        if !config
            .issue_priorities
            .values
            .iter()
            .any(|priority| priority.eq_ignore_case(config.default_priority.as_str()))
        {
            result.add_error(
                ValidationError::warning(
                    Some("default_priority".to_string()),
                    format!(
                        "Default priority '{}' not found in issue_priorities list",
                        config.default_priority
                    ),
                )
                .with_fix(
                    "Add the priority to issue_priorities or choose a different default"
                        .to_string(),
                ),
            );
        }

        self.validate_alias_targets(
            "branch_status_aliases",
            "issue_states",
            &config.branch_status_aliases,
            &config.issue_states.values,
            |alias, allowed| alias.eq_ignore_case(allowed.as_str()),
            &mut result,
        );
        self.validate_alias_targets(
            "branch_type_aliases",
            "issue_types",
            &config.branch_type_aliases,
            &config.issue_types.values,
            |alias, allowed| alias.eq_ignore_case(allowed.as_str()),
            &mut result,
        );
        self.validate_alias_targets(
            "branch_priority_aliases",
            "issue_priorities",
            &config.branch_priority_aliases,
            &config.issue_priorities.values,
            |alias, allowed| alias.eq_ignore_case(allowed.as_str()),
            &mut result,
        );

        // Validate scan.ticket_patterns (if present)
        if let Some(patterns) = &config.scan_ticket_patterns {
            self.warn_on_duplicate_values(
                "scan.ticket_patterns",
                patterns.iter().map(|v| v.as_str()),
                &mut result,
            );
            self.validate_ticket_patterns(patterns, &mut result);
        }

        result
    }

    #[allow(dead_code)]
    pub fn validate_resolved_config(&self, config: &ResolvedConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        // This validates the final resolved configuration for consistency
        // Similar validations as global config but for the resolved state

        if !config.issue_states.values.is_empty()
            && let Some(default_status) = config.default_status.as_ref()
            && !config
                .issue_states
                .values
                .iter()
                .any(|state| state.eq_ignore_case(default_status.as_str()))
        {
            result.add_error(
                ValidationError::warning(
                    Some("default_status".to_string()),
                    "Resolved default status not found in resolved issue states".to_string(),
                )
                .with_fix("Add the status to issue.states or adjust default_status".to_string()),
            );
        }

        if !config.issue_priorities.values.is_empty()
            && !config
                .issue_priorities
                .values
                .iter()
                .any(|priority| priority.eq_ignore_case(config.default_priority.as_str()))
        {
            result.add_error(
                ValidationError::warning(
                    Some("default_priority".to_string()),
                    "Resolved default priority not found in resolved issue priorities".to_string(),
                )
                .with_fix(
                    "Add the priority to issue.priorities or pick a different default".to_string(),
                ),
            );
        }

        if config.strict_members && !Self::members_list_has_entries(&config.members) {
            result.add_error(
                ValidationError::error(
                    Some("members".to_string()),
                    "strict_members is enabled but no members remain after resolution".to_string(),
                )
                .with_fix("Add entries under members or disable strict_members".to_string()),
            );
        }

        self.validate_default_tags_subset(
            "default_tags",
            &config.default_tags,
            &config.tags,
            "issue.tags",
            &mut result,
        );

        self.validate_alias_targets(
            "branch_status_aliases",
            "issue_states",
            &config.branch_status_aliases,
            &config.issue_states.values,
            |alias, allowed| alias.eq_ignore_case(allowed.as_str()),
            &mut result,
        );
        self.validate_alias_targets(
            "branch_type_aliases",
            "issue_types",
            &config.branch_type_aliases,
            &config.issue_types.values,
            |alias, allowed| alias.eq_ignore_case(allowed.as_str()),
            &mut result,
        );
        self.validate_alias_targets(
            "branch_priority_aliases",
            "issue_priorities",
            &config.branch_priority_aliases,
            &config.issue_priorities.values,
            |alias, allowed| alias.eq_ignore_case(allowed.as_str()),
            &mut result,
        );

        result
    }

    pub fn check_prefix_conflicts(&self, prefix: &str) -> ValidationResult {
        match PrefixConflictDetector::new(&self.tasks_dir) {
            Ok(detector) => detector.check_conflicts(prefix),
            Err(e) => {
                let mut result = ValidationResult::new();
                result.add_error(ValidationError::warning(
                    None,
                    format!("Could not check for prefix conflicts: {}", e),
                ));
                result
            }
        }
    }

    #[allow(dead_code)]
    pub fn validate_prefix_format(&self, prefix: &str) -> ValidationResult {
        let mut result = ValidationResult::new();
        self.validate_prefix(prefix, &mut result);
        result
    }

    fn validate_project_name(&self, name: &str, result: &mut ValidationResult) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            result.add_error(ValidationError::error(
                Some("project_name".to_string()),
                "Project name cannot be empty".to_string(),
            ));
        }

        if trimmed.len() > 100 {
            result.add_error(
                ValidationError::warning(
                    Some("project_name".to_string()),
                    "Project name is very long (>100 characters)".to_string(),
                )
                .with_fix("Consider using a shorter, more descriptive name".to_string()),
            );
        }
    }

    fn validate_prefix(&self, prefix: &str, result: &mut ValidationResult) {
        if prefix.is_empty() {
            result.add_error(ValidationError::warning(
                Some("default_project".to_string()),
                "Default prefix is empty, will auto-generate from project name".to_string(),
            ));
            return;
        }

        if prefix.len() > MAX_PREFIX_LENGTH {
            result.add_error(
                ValidationError::error(
                    Some("default_project".to_string()),
                    format!(
                        "Prefix is too long ({} characters); maximum supported length is {}",
                        prefix.len(),
                        MAX_PREFIX_LENGTH
                    ),
                )
                .with_fix(format!(
                    "Use a shorter prefix of at most {} characters",
                    MAX_PREFIX_LENGTH
                )),
            );
            return;
        }

        if prefix.len() > LONG_PREFIX_WARNING_THRESHOLD {
            result.add_error(
                ValidationError::warning(
                    Some("default_project".to_string()),
                    "Prefix is quite long, shorter prefixes are more practical".to_string(),
                )
                .with_fix("Consider using a 2-4 character prefix".to_string()),
            );
        }

        if !prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            result.add_error(
                ValidationError::error(
                    Some("default_project".to_string()),
                    "Prefix contains invalid characters".to_string(),
                )
                .with_fix("Use only letters, numbers, underscores, and hyphens".to_string()),
            );
        }
    }

    fn validate_defaults_consistency(&self, config: &ProjectConfig, result: &mut ValidationResult) {
        // Check if default status exists in issue states
        if let (Some(default_status), Some(issue_states)) =
            (&config.default_status, &config.issue_states)
            && !issue_states.values.contains(default_status)
        {
            result.add_error(
                ValidationError::warning(
                    Some("default_status".to_string()),
                    format!(
                        "Default status '{}' not found in issue_states",
                        default_status
                    ),
                )
                .with_fix("Add the status to issue_states or remove default_status".to_string()),
            );
        }

        // Check if default priority exists in issue priorities
        if let (Some(default_priority), Some(issue_priorities)) =
            (&config.default_priority, &config.issue_priorities)
            && !issue_priorities
                .values
                .iter()
                .any(|priority| priority.eq_ignore_case(default_priority.as_str()))
        {
            result.add_error(
                ValidationError::warning(
                    Some("default_priority".to_string()),
                    format!(
                        "Default priority '{}' not found in issue_priorities",
                        default_priority
                    ),
                )
                .with_fix(
                    "Add the priority to issue_priorities or remove default_priority".to_string(),
                ),
            );
        }
    }

    fn validate_enum_fields(&self, _config: &ProjectConfig, _result: &mut ValidationResult) {
        // The enum validation is handled by serde deserialization
        // This is here for future custom enum validation if needed
    }

    fn validate_field_formats(&self, config: &ProjectConfig, result: &mut ValidationResult) {
        // Validate assignee email format if present
        if let Some(assignee) = &config.default_assignee
            && !assignee.is_empty()
            && !self.is_valid_email_or_username(assignee)
        {
            result.add_error(
                ValidationError::warning(
                    Some("default_assignee".to_string()),
                    "Assignee format doesn't look like an email or @username".to_string(),
                )
                .with_fix("Use email format (user@domain.com) or @username format".to_string()),
            );
        }

        if let Some(reporter) = &config.default_reporter
            && !reporter.is_empty()
            && !self.is_valid_email_or_username(reporter)
        {
            result.add_error(
                ValidationError::warning(
                    Some("default_reporter".to_string()),
                    "Reporter format doesn't look like an email or @username".to_string(),
                )
                .with_fix("Use email format (user@domain.com) or @username format".to_string()),
            );
        }

        if let Some(tags) = &config.default_tags {
            self.warn_on_duplicate_values("default_tags", tags.iter().map(|v| v.as_str()), result);
        }

        if let Some(signal_words) = &config.scan_signal_words {
            self.warn_on_duplicate_values(
                "scan_signal_words",
                signal_words.iter().map(|v| v.as_str()),
                result,
            );
        }

        if let Some(patterns) = &config.scan_ticket_patterns {
            self.warn_on_duplicate_values(
                "scan.ticket_patterns",
                patterns.iter().map(|v| v.as_str()),
                result,
            );
        }

        if let Some(tags) = &config.tags {
            self.warn_on_duplicate_values("tags", tags.values.iter().map(|v| v.as_str()), result);
        }

        if let Some(custom_fields) = &config.custom_fields {
            self.warn_on_duplicate_values(
                "custom_fields",
                custom_fields.values.iter().map(|v| v.as_str()),
                result,
            );
        }
    }

    fn validate_ticket_patterns(&self, patterns: &[String], result: &mut ValidationResult) {
        // Errors for invalid regex, warnings for overlapping/ambiguous patterns
        let mut compiled: Vec<(String, regex::Regex)> = Vec::new();
        for (i, p) in patterns.iter().enumerate() {
            match regex::Regex::new(p) {
                Ok(r) => compiled.push((p.clone(), r)),
                Err(e) => result.add_error(
                    ValidationError::error(
                        Some(format!("scan.ticket_patterns[{}]", i)),
                        format!("Invalid regex: {}", e),
                    )
                    .with_fix("Ensure the pattern is a valid Rust regex".to_string()),
                ),
            }
        }

        // Check for ambiguous overlaps by testing representative samples
        // Heuristic: if two patterns can both match the same simple key formats, warn
        let samples = vec![
            "DEMO-123",
            "ABC_99",
            "[ticket=DEMO-1]",
            "feat/PROJ-42",
            "#123",
        ];
        for s in samples {
            let matches: Vec<&str> = compiled
                .iter()
                .filter(|(_, re)| re.is_match(s))
                .map(|(p, _)| p.as_str())
                .collect();
            if matches.len() > 1 {
                result.add_error(
                    ValidationError::warning(
                        Some("scan.ticket_patterns".to_string()),
                        format!(
                            "Multiple patterns match sample '{}': {}",
                            s,
                            matches.join(", ")
                        ),
                    )
                    .with_fix(
                        "Consider making patterns mutually exclusive or ordering them".to_string(),
                    ),
                );
            }
        }
    }

    fn is_valid_email_or_username(&self, value: &str) -> bool {
        // Simple validation for email or @username format
        if let Some(stripped) = value.strip_prefix('@') {
            return value.len() > 1
                && stripped
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-');
        }

        // Basic email validation
        value.contains('@') && value.contains('.') && value.len() > 5
    }

    fn warn_on_duplicate_values<'a, I>(&self, field: &str, values: I, result: &mut ValidationResult)
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut seen = HashSet::new();
        for raw in values {
            let value = raw.trim();
            if value.is_empty() || value == "*" {
                continue;
            }
            let key = value.to_ascii_lowercase();
            if !seen.insert(key) {
                result.add_error(
                    ValidationError::warning(
                        Some(field.to_string()),
                        format!("Duplicate value '{}' found in {}", value, field),
                    )
                    .with_fix("Remove duplicate entries to avoid ambiguity".to_string()),
                );
                break;
            }
        }
    }

    fn members_list_has_entries(values: &[String]) -> bool {
        values.iter().any(|value| !value.trim().is_empty())
    }

    fn validate_default_tags_subset(
        &self,
        field_name: &str,
        defaults: &[String],
        allowed: &StringConfigField,
        allowed_field_label: &str,
        result: &mut ValidationResult,
    ) {
        if defaults.is_empty() || allowed.has_wildcard() {
            return;
        }

        let mut invalid = Vec::new();
        for tag in defaults {
            let trimmed = tag.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !allowed
                .values
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(trimmed))
            {
                invalid.push(trimmed.to_string());
            }
        }

        if !invalid.is_empty() {
            result.add_error(
                ValidationError::error(
                    Some(field_name.to_string()),
                    format!(
                        "{} contains values not present in {}: {}",
                        field_name,
                        allowed_field_label,
                        invalid.join(", ")
                    ),
                )
                .with_fix(format!(
                    "Add the missing values to {} or remove them from {}",
                    allowed_field_label, field_name
                )),
            );
        }
    }

    fn validate_alias_targets<T, F>(
        &self,
        alias_field: &str,
        allowed_field: &str,
        aliases: &HashMap<String, T>,
        allowed: &[T],
        mut equals: F,
        result: &mut ValidationResult,
    ) where
        T: std::fmt::Display,
        F: FnMut(&T, &T) -> bool,
    {
        if aliases.is_empty() || allowed.is_empty() {
            return;
        }

        for (alias, target) in aliases {
            if allowed.iter().any(|candidate| equals(target, candidate)) {
                continue;
            }
            result.add_error(
                ValidationError::error(
                    Some(alias_field.to_string()),
                    format!(
                        "Alias '{}' maps to '{}' which is not present in {}",
                        alias, target, allowed_field
                    ),
                )
                .with_fix(format!(
                    "Add '{}' to {} or change the alias target",
                    target, allowed_field
                )),
            );
        }
    }
}
