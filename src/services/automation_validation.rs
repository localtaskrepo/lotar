use crate::automation::types::{
    AutomationAction, AutomationConditionGroup, AutomationFieldCondition, AutomationFile,
    AutomationRunAction,
};
use crate::config::types::ResolvedConfig;
use crate::config::validation::errors::{ValidationError, ValidationResult};
use crate::services::automation_service::{SetEnumField, SetPlan};
use crate::types::{Priority, TaskStatus, TaskType};
use std::time::Duration;

pub(crate) fn parse_cooldown(value: &str) -> Option<Duration> {
    let value = value.trim();
    let unit = value.chars().last()?;
    let multiplier = match unit {
        's' => 1,
        'm' => 60,
        'h' => 3600,
        'd' => 86400,
        _ => return None,
    };
    let count = value.strip_suffix(unit)?.parse::<u64>().ok()?;
    Some(Duration::from_secs(count.checked_mul(multiplier)?))
}

pub(crate) fn validate_rules(file: &AutomationFile, config: &ResolvedConfig) -> ValidationResult {
    let mut result = ValidationResult::new();
    for rule in file.automation.rules() {
        if let Some(cooldown) = rule.cooldown.as_deref()
            && parse_cooldown(cooldown).is_none()
        {
            result.add_error(ValidationError::error(
                Some("automation.cooldown".to_string()),
                "Cooldown must be a nonnegative integer followed by s, m, h, or d, within the supported duration range.".to_string(),
            ));
        }
        if let Some(group) = rule.when.as_ref() {
            validate_condition_group(group, config, &mut result);
        }
        // Legacy catch-all
        validate_action(rule.on.start.as_ref(), config, &mut result);
        // Ticket events
        validate_action(rule.on.created.as_ref(), config, &mut result);
        validate_action(rule.on.updated.as_ref(), config, &mut result);
        validate_action(rule.on.assigned.as_ref(), config, &mut result);
        validate_action(rule.on.commented.as_ref(), config, &mut result);
        validate_action(rule.on.sprint_changed.as_ref(), config, &mut result);
        // Job events
        validate_action(rule.on.job_started.as_ref(), config, &mut result);
        validate_action(rule.on.job_completed.as_ref(), config, &mut result);
        validate_action(rule.on.job_failed.as_ref(), config, &mut result);
        validate_action(rule.on.job_cancelled.as_ref(), config, &mut result);
    }
    result
}

fn validate_condition_group(
    group: &AutomationConditionGroup,
    config: &ResolvedConfig,
    result: &mut ValidationResult,
) {
    if let Some(all) = group.all.as_ref() {
        for entry in all {
            validate_condition_group(entry, config, result);
        }
    }
    if let Some(any) = group.any.as_ref() {
        for entry in any {
            validate_condition_group(entry, config, result);
        }
    }
    if let Some(not) = group.not.as_ref() {
        validate_condition_group(not, config, result);
    }
    if let Some(changes) = group.changes.as_ref() {
        for (field, change) in changes {
            if let Some(from) = change.from.as_ref() {
                validate_condition_value(field, from, config, result);
            }
            if let Some(to) = change.to.as_ref() {
                validate_condition_value(field, to, config, result);
            }
        }
    }
    for (field, condition) in group.fields.iter() {
        validate_condition_value(field, condition, config, result);
    }
    for (field, condition) in group.custom_fields.iter() {
        validate_condition_value(
            &format!("custom_fields.{}", field),
            condition,
            config,
            result,
        );
    }
}

fn validate_condition_value(
    field: &str,
    condition: &AutomationFieldCondition,
    config: &ResolvedConfig,
    result: &mut ValidationResult,
) {
    let map = condition.as_map();
    // An invalid pattern would silently never match; say so when saving.
    if let Some(pattern) = map.matches.as_ref()
        && let Err(error) = regex::Regex::new(pattern)
    {
        let detail = error.to_string();
        let reason = detail.lines().last().unwrap_or("invalid pattern").trim();
        let reason = reason.strip_prefix("error: ").unwrap_or(reason);
        result.add_error(ValidationError::error(
            Some("automation".to_string()),
            format!(
                "Automation condition on '{field}' has an invalid pattern '{pattern}': {reason}."
            ),
        ));
    }
    let Some(target) = map.equals.as_ref() else {
        if let Some(values) = map.any.as_ref() {
            for value in values {
                validate_condition_value(
                    field,
                    &AutomationFieldCondition::Scalar(value.clone()),
                    config,
                    result,
                );
            }
        }
        return;
    };
    match field.to_lowercase().as_str() {
        "status" if TaskStatus::parse_with_config(target, config).is_err() => {
            result.add_error(ValidationError::warning(
                Some("automation".to_string()),
                format!(
                    "Automation condition references unknown status '{}'.",
                    target
                ),
            ));
        }
        "priority" if Priority::parse_with_config(target, config).is_err() => {
            result.add_error(ValidationError::warning(
                Some("automation".to_string()),
                format!(
                    "Automation condition references unknown priority '{}'.",
                    target
                ),
            ));
        }
        "type" | "task_type" if TaskType::parse_with_config(target, config).is_err() => {
            result.add_error(ValidationError::warning(
                Some("automation".to_string()),
                format!("Automation condition references unknown type '{}'.", target),
            ));
        }
        "assignee" if target.eq_ignore_ascii_case("@agent") && config.agent_profiles.is_empty() => {
            result.add_error(ValidationError::warning(
                Some("automation".to_string()),
                "Automation uses @agent but no agent profiles are configured.".to_string(),
            ));
        }
        _ => {}
    }
}

fn validate_action(
    action: Option<&AutomationAction>,
    config: &ResolvedConfig,
    result: &mut ValidationResult,
) {
    let Some(action) = action else {
        return;
    };
    if let Some(set) = action.set.as_ref() {
        let plan = SetPlan::parse(set, config, str::to_string);
        for (field, value) in plan.rejected {
            let label = match field {
                SetEnumField::Status => "status",
                SetEnumField::Priority => "priority",
                SetEnumField::Type => "type",
            };
            result.add_error(ValidationError::warning(
                Some("automation".to_string()),
                format!("Automation action sets unknown {} '{}'.", label, value),
            ));
        }
    }

    if let Some(run) = action.run.as_ref() {
        let trimmed = match run {
            AutomationRunAction::Shell(value) => value.trim(),
            AutomationRunAction::Command(command) => command.command.trim(),
        };
        if trimmed.is_empty() {
            result.add_error(ValidationError::warning(
                Some("automation".to_string()),
                "Automation run command cannot be empty.".to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::GlobalConfig;

    fn validate(yaml: &str) -> ValidationResult {
        let file: AutomationFile = serde_yaml_ng::from_str(yaml).unwrap();
        validate_rules(&file, &ResolvedConfig::from_global(GlobalConfig::default()))
    }

    #[test]
    fn reports_invalid_match_patterns() {
        let result = validate(
            "automation:\n  rules:\n    - when:\n        title:\n          matches: '(unclosed'\n      on:\n        updated:\n          add_tags: [x]\n",
        );
        assert!(
            result
                .errors
                .iter()
                .any(|error| error.to_string().contains("invalid pattern '(unclosed'")),
            "{:?}",
            result.errors
        );
        let ok = validate(
            "automation:\n  rules:\n    - when:\n        title:\n          matches: '^\\p{L}+ \\w+$'\n      on:\n        updated:\n          add_tags: [x]\n",
        );
        assert!(ok.errors.is_empty(), "{:?}", ok.errors);
    }

    /// DEV-221: Unicode classes user patterns (search, scan, validation,
    /// automation) keep after trimming the regex crate's tables.
    #[test]
    fn unicode_support_in_user_patterns() {
        let matches =
            |pattern: &str, text: &str| regex::Regex::new(pattern).unwrap().is_match(text);
        // Every script works with \w, \b, case folding and general categories.
        assert!(matches(r"^\w+$", "Größe"));
        assert!(matches(r"^\w+$", "задача"));
        assert!(matches(r"^\w+$", "任务"));
        assert!(matches(r"(?i)^ΑΒΓ$", "αβγ"));
        assert!(matches(r"\bfix\b", "fix ошибку"));
        assert!(matches(r"^\p{L}+$", "日本語"));
        assert!(matches(r"^\p{Lu}", "Éclair"));
        assert!(matches(r"\d", "٣"));
        // Named script and boolean properties (\p{Greek}, \p{Emoji}) are left
        // out of normal builds; test builds can't show it, because dev-only
        // dependencies turn the regex crate's full Unicode tables back on.
    }
}
