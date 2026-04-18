use crate::automation::types::{
    AutomationAction, AutomationConditionGroup, AutomationFieldCondition, AutomationFile,
    AutomationRunAction,
};
use crate::config::types::ResolvedConfig;
use crate::config::validation::errors::{ValidationError, ValidationResult};
use crate::types::{Priority, TaskStatus, TaskType};

pub(crate) fn validate_rules(file: &AutomationFile, config: &ResolvedConfig) -> ValidationResult {
    let mut result = ValidationResult::new();
    for rule in file.automation.rules() {
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
        if let Some(status) = set.status.as_ref()
            && TaskStatus::parse_with_config(status, config).is_err()
        {
            result.add_error(ValidationError::warning(
                Some("automation".to_string()),
                format!("Automation action sets unknown status '{}'.", status),
            ));
        }
        if let Some(priority) = set.priority.as_ref()
            && Priority::parse_with_config(priority, config).is_err()
        {
            result.add_error(ValidationError::warning(
                Some("automation".to_string()),
                format!("Automation action sets unknown priority '{}'.", priority),
            ));
        }
        if let Some(task_type) = set.task_type.as_ref()
            && TaskType::parse_with_config(task_type, config).is_err()
        {
            result.add_error(ValidationError::warning(
                Some("automation".to_string()),
                format!("Automation action sets unknown type '{}'.", task_type),
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
