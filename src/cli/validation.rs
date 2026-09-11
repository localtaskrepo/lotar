use crate::config::types::ResolvedConfig;
use crate::storage::task::Task;
use crate::types::{Priority, TaskStatus, TaskType};

/// Configuration-aware validation for CLI inputs
pub struct CliValidator<'a> {
    config: &'a ResolvedConfig,
}

impl<'a> CliValidator<'a> {
    pub fn new(config: &'a ResolvedConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &ResolvedConfig {
        self.config
    }

    /// Validate status against project configuration (case-insensitive, returns canonical form)
    pub fn validate_status(&self, status: &str) -> Result<TaskStatus, String> {
        TaskStatus::parse_with_config(status, self.config)
    }

    /// Validate task type against project configuration (case-insensitive, returns canonical form)
    pub fn validate_task_type(&self, task_type: &str) -> Result<TaskType, String> {
        TaskType::parse_with_config(task_type, self.config)
    }

    /// Validate priority against project configuration (case-insensitive, returns canonical form)
    pub fn validate_priority(&self, priority: &str) -> Result<Priority, String> {
        Priority::parse_with_config(priority, self.config)
    }

    /// Validate tag against project configuration
    pub fn validate_tag(&self, tag: &str) -> Result<String, String> {
        let normalized = tag.trim();
        if normalized.is_empty() {
            return Err("Tag cannot be empty or whitespace".to_string());
        }

        if self.config.tags.has_wildcard() {
            // Any tag is allowed
            Ok(normalized.to_string())
        } else if self.config.tags.values.contains(&normalized.to_string()) {
            Ok(normalized.to_string())
        } else {
            let suggestion = crate::services::task_validation::find_closest_match(
                normalized,
                &self.config.tags.values,
            );
            let suggestion_text = match suggestion {
                Some(s) => format!(" Did you mean '{}'?", s),
                None => String::new(),
            };

            Err(format!(
                "Tag '{}' is not allowed in this project. Valid tags: {}.{}",
                normalized,
                self.config.tags.values.join(", "),
                suggestion_text
            ))
        }
    }

    /// Validate custom field name against project configuration
    pub fn validate_custom_field_name(&self, field_name: &str) -> Result<String, String> {
        crate::services::task_validation::validate_custom_field_name(field_name, self.config)
    }

    /// Validate custom field key-value pair
    pub fn validate_custom_field(
        &self,
        field_name: &str,
        field_value: &str,
    ) -> Result<(String, String), String> {
        // First validate the field name
        let validated_name = self.validate_custom_field_name(field_name)?;

        // For now, allow any value for custom fields
        // In the future, this could be extended to validate values based on field type
        Ok((validated_name, field_value.to_string()))
    }

    /// Validate assignee format (basic email validation) and enforce configured members
    pub fn validate_assignee(&self, assignee: &str) -> Result<String, String> {
        self.validate_member_value("Assignee", assignee, false)
    }

    /// Validate assignee but allow values not yet registered as members.
    pub fn validate_assignee_allow_unknown(&self, assignee: &str) -> Result<String, String> {
        self.validate_member_value("Assignee", assignee, true)
    }

    /// Validate reporter format (basic email validation) and enforce configured members
    pub fn validate_reporter(&self, reporter: &str) -> Result<String, String> {
        self.validate_member_value("Reporter", reporter, false)
    }

    /// Validate reporter but allow values not yet registered as members.
    pub fn validate_reporter_allow_unknown(&self, reporter: &str) -> Result<String, String> {
        self.validate_member_value("Reporter", reporter, true)
    }

    /// Parse and validate due date/time. Normalizes to RFC3339 (UTC) string.
    ///
    /// Supported:
    /// - Absolute date: YYYY-MM-DD (interpreted as local midnight, converted to UTC)
    /// - RFC3339 datetime: 2025-12-31T15:04:05Z or with offset
    /// - Local naive datetime: "YYYY-MM-DD HH:MM[:SS]" or "YYYY-MM-DDTHH:MM[:SS]" (assumed local tz)
    /// - Keywords: today, tomorrow, next week, next <weekday>
    /// - Shortcuts: in Nd/Nw, +/-Nd/+/-Nw, +Nbd (business days), next business day,
    ///   this/by <weekday>, <weekday>, next week <weekday>
    ///
    /// Date-like inputs normalize to a local YYYY-MM-DD string; datetime-like
    /// inputs normalize to RFC3339. Parsing itself is delegated to
    /// [`crate::utils::time::parse_human_datetime_to_utc`].
    pub fn parse_due_date(&self, due_date: &str) -> Result<String, String> {
        let s_raw = due_date.trim();

        let invalid = || {
            format!(
                "Invalid date format: '{}'. Try one of: YYYY-MM-DD, RFC3339 (2025-12-31T15:04:05Z), 'in 3 days', '+3d', '+2w', '+1bd', 'next business day', 'next monday', 'this friday', 'by fri', 'next week monday'",
                due_date
            )
        };

        // Datetime-like forms normalize to RFC3339 (UTC)
        let datetime_like = chrono::DateTime::parse_from_rfc3339(s_raw).is_ok()
            || crate::utils::time::parse_naive_local_datetime_to_utc(s_raw).is_some();

        crate::utils::time::parse_human_datetime_to_utc(s_raw)
            .map(|dt| {
                if datetime_like {
                    dt.to_rfc3339()
                } else {
                    dt.with_timezone(&chrono::Local)
                        .date_naive()
                        .format("%Y-%m-%d")
                        .to_string()
                }
            })
            .map_err(|_| invalid())
    }

    /// Validate effort estimate format
    pub fn validate_effort(&self, effort: &str) -> Result<String, String> {
        // Accept both time and points units as valid effort formats.
        let t = effort.trim().to_lowercase();
        match crate::utils::effort::parse_effort(&t) {
            Ok(_) => Ok(effort.to_string()),
            Err(_) => Err("Invalid effort format. Use a number followed by a valid unit (h, d, w, m, pt, points, etc.), e.g., 2h, 1.5d, 1w, 5pt, 3points".to_string()),
        }
    }

    pub fn ensure_task_membership(&self, task: &Task) -> Result<(), String> {
        if !self.config.strict_members {
            return Ok(());
        }

        let allowed = self.normalized_members();
        if allowed.is_empty() {
            return Err(Self::strict_members_misconfiguration_error());
        }

        self.enforce_member_value("Reporter", task.reporter.as_deref(), &allowed)?;
        self.enforce_member_value("Assignee", task.assignee.as_deref(), &allowed)?;
        Ok(())
    }

    fn enforce_member_for_value(&self, field_label: &str, value: &str) -> Result<(), String> {
        if !self.config.strict_members {
            return Ok(());
        }

        let allowed = self.normalized_members();
        if allowed.is_empty() {
            return Err(Self::strict_members_misconfiguration_error());
        }

        self.enforce_member_value(field_label, Some(value), &allowed)
    }

    fn validate_member_value(
        &self,
        field_label: &str,
        raw_value: &str,
        allow_unknown: bool,
    ) -> Result<String, String> {
        use crate::utils::member::{
            is_builtin_directive, is_email_like, is_valid_username, normalize_member_value,
        };

        let trimmed = raw_value.trim();

        if trimmed.is_empty() {
            return Err(format!("{} cannot be empty or whitespace", field_label));
        }

        // Built-in directives (@me) are returned as-is.
        if is_builtin_directive(trimmed) {
            return Ok(trimmed.to_string());
        }

        // @-prefixed: validate the name portion, then normalize.
        if let Some(username) = trimmed.strip_prefix('@') {
            if username.is_empty() || !is_valid_username(username) {
                return Err("Invalid username format. Usernames can only contain letters, numbers, underscore, dash, and period.".to_string());
            }
            // Normalize: strip @ for non-directive / non-agent values.
            let result = normalize_member_value(trimmed, |name| {
                self.config.agent_profiles.contains_key(name)
            });
            if !allow_unknown {
                self.enforce_member_for_value(field_label, &result)?;
            }
            return Ok(result);
        }

        // Email addresses pass through unchanged.
        if is_email_like(trimmed) {
            if !allow_unknown {
                self.enforce_member_for_value(field_label, trimmed)?;
            }
            return Ok(trimmed.to_string());
        }

        // Bare usernames (no @ prefix, no email @).
        if is_valid_username(trimmed) {
            if !allow_unknown {
                self.enforce_member_for_value(field_label, trimmed)?;
            }
            return Ok(trimmed.to_string());
        }

        Err(format!(
            "{} must be a username, email address, or @directive",
            field_label
        ))
    }

    fn enforce_member_value(
        &self,
        field_label: &str,
        value: Option<&str>,
        allowed: &[String],
    ) -> Result<(), String> {
        let Some(raw) = value else {
            return Ok(());
        };

        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        let norm_val = crate::utils::member::member_for_comparison(trimmed);
        let permitted = allowed
            .iter()
            .any(|candidate| crate::utils::member::member_for_comparison(candidate) == norm_val);

        if permitted {
            return Ok(());
        }

        let preview = self.member_preview(allowed);
        Err(format!(
            "{} '{}' is not in configured members. Allowed members: {}.",
            field_label, trimmed, preview
        ))
    }

    fn normalized_members(&self) -> Vec<String> {
        self.config
            .members
            .iter()
            .map(|member| member.trim().to_string())
            .filter(|member| !member.is_empty())
            .collect()
    }

    fn member_preview(&self, allowed: &[String]) -> String {
        if allowed.is_empty() {
            return String::new();
        }
        if allowed.len() <= 10 {
            allowed.join(", ")
        } else {
            let head = allowed
                .iter()
                .take(10)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} ... (+{} more)", head, allowed.len() - 10)
        }
    }

    fn strict_members_misconfiguration_error() -> String {
        "Strict members are enabled but no members are configured. Add entries under members or disable strict_members.".to_string()
    }
}

// inline tests moved to tests/cli_validation_unit_test.rs
