//! Shared project-aware mutation validation.
//!
//! Every task mutation surface (CLI, REST, MCP, sync) funnels enum and custom
//! field validation through the helpers in this module so a value is always
//! checked against the target project's resolved configuration, never against
//! an unrelated global snapshot.

use crate::config::types::ResolvedConfig;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::types::{CustomFields, Priority, TaskStatus, TaskType};

pub fn parse_status(raw: &str, config: &ResolvedConfig) -> LoTaRResult<TaskStatus> {
    TaskStatus::parse_with_config(raw, config).map_err(LoTaRError::ValidationError)
}

pub fn parse_priority(raw: &str, config: &ResolvedConfig) -> LoTaRResult<Priority> {
    Priority::parse_with_config(raw, config).map_err(LoTaRError::ValidationError)
}

pub fn parse_task_type(raw: &str, config: &ResolvedConfig) -> LoTaRResult<TaskType> {
    TaskType::parse_with_config(raw, config).map_err(LoTaRError::ValidationError)
}

/// Validate a custom field name against project configuration, returning the
/// canonical configured name when recognized.
pub fn validate_custom_field_name(
    field_name: &str,
    config: &ResolvedConfig,
) -> Result<String, String> {
    if let Some(canonical) = crate::utils::fields::is_reserved_field(field_name) {
        return Err(format!(
            "Field name '{}' collides with built-in field '{}'. Use the built-in option instead, or pick a different custom field name.",
            field_name, canonical
        ));
    }
    if config.custom_fields.has_wildcard() {
        return Ok(field_name.to_string());
    }
    if let Some(existing) = config
        .custom_fields
        .values
        .iter()
        .find(|value| value.eq_ignore_ascii_case(field_name.trim()))
    {
        return Ok(existing.clone());
    }
    let suggestion = find_closest_match(field_name, &config.custom_fields.values);
    let suggestion_text = match suggestion {
        Some(s) => format!(" Did you mean '{}'?", s),
        None => String::new(),
    };
    Err(format!(
        "Custom field '{}' is not allowed in this project. Valid custom fields: {}.{}",
        field_name,
        config.custom_fields.values.join(", "),
        suggestion_text
    ))
}

/// Resolve a custom field map against project configuration.
///
/// Configured keys are canonicalized to their configured spelling; keys that
/// already exist on the task pass through unchanged so removing a field from
/// project config cannot reject an unchanged echo under replace-all patch
/// semantics; brand-new keys must be configured (or allowed by wildcard).
/// Distinct keys that canonicalize to the same configured name are rejected
/// instead of silently overwriting each other.
pub fn resolve_custom_fields(
    fields: &CustomFields,
    config: &ResolvedConfig,
    existing: Option<&CustomFields>,
) -> LoTaRResult<CustomFields> {
    let mut resolved = CustomFields::new();
    let mut sources: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for (key, value) in fields {
        if let Some(canonical) = crate::utils::fields::is_reserved_field(key) {
            return Err(LoTaRError::ValidationError(format!(
                "Field name '{}' collides with built-in field '{}'. Use the built-in option instead, or pick a different custom field name.",
                key, canonical
            )));
        }
        let target = match configured_field_key(key, config) {
            Some(canonical) => canonical,
            None => {
                let legacy = existing.is_some_and(|map| {
                    map.keys()
                        .any(|existing_key| existing_key.eq_ignore_ascii_case(key.trim()))
                });
                if config.custom_fields.has_wildcard() || legacy {
                    key.trim().to_string()
                } else {
                    return Err(LoTaRError::ValidationError(format!(
                        "Custom field '{}' is not allowed in this project. Valid custom fields: {}.",
                        key,
                        config.custom_fields.values.join(", ")
                    )));
                }
            }
        };

        if let Some(other) = sources.get(&target)
            && other != key
        {
            return Err(LoTaRError::ValidationError(format!(
                "Custom field keys '{}' and '{}' both resolve to '{}'; use a single spelling.",
                other, key, target
            )));
        }
        sources.insert(target.clone(), key.clone());
        resolved.insert(target, value.clone());
    }

    Ok(resolved)
}

fn configured_field_key(field_name: &str, config: &ResolvedConfig) -> Option<String> {
    config
        .custom_fields
        .values
        .iter()
        .find(|value| value.eq_ignore_ascii_case(field_name.trim()))
        .cloned()
}

/// Find the closest match for a string in a list (edit distance).
pub fn find_closest_match(input: &str, candidates: &[String]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }

    let input_lower = input.to_lowercase();
    let mut best_match = None;
    let mut best_distance = usize::MAX;

    for candidate in candidates {
        let candidate_lower = candidate.to_lowercase();
        let distance = edit_distance(&input_lower, &candidate_lower);

        if distance < input.len() / 2 + 1 && distance < best_distance {
            best_distance = distance;
            best_match = Some(candidate.clone());
        }
    }

    best_match
}

/// Levenshtein distance between two strings.
pub fn edit_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for (i, row) in matrix.iter_mut().enumerate().take(len1 + 1) {
        row[0] = i;
    }
    for (j, cell) in matrix[0].iter_mut().enumerate().take(len2 + 1) {
        *cell = j;
    }

    for (i, c1) in s1.chars().enumerate() {
        for (j, c2) in s2.chars().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[len1][len2]
}
