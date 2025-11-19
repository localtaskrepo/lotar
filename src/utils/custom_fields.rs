use std::collections::{BTreeMap, BTreeSet};

pub type PartitionedWhereFilters = (
    BTreeMap<String, Vec<String>>,
    Vec<(String, String)>,
    BTreeSet<String>,
);

use crate::config::types::ResolvedConfig;
use crate::types::{CustomFields, custom_value_to_string};

/// Determine whether a `--where` key targets a custom field and return the
/// canonical field name if so.
pub fn resolve_filter_name(raw_key: &str, config: &ResolvedConfig) -> Option<String> {
    let trimmed = raw_key.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(idx) = trimmed.find(':') {
        let (prefix, rest) = trimmed.split_at(idx);
        if prefix.eq_ignore_ascii_case("field") {
            let name = rest.trim_start_matches(':').trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }

    if config.custom_fields.has_wildcard()
        || config
            .custom_fields
            .values
            .iter()
            .any(|value| value.eq_ignore_ascii_case(trimmed))
    {
        return Some(trimmed.to_string());
    }

    None
}

/// Extract a string representation of a task's custom field value with the
/// provided name (case-insensitive lookup).
pub fn extract_value_strings(fields: &CustomFields, name: &str) -> Option<Vec<String>> {
    if let Some(value) = fields.get(name) {
        return Some(vec![custom_value_to_string(value)]);
    }

    let target = name.to_ascii_lowercase();
    fields
        .iter()
        .find(|(key, _)| key.to_ascii_lowercase() == target)
        .map(|(_, value)| vec![custom_value_to_string(value)])
}

/// Canonicalize a custom field key for set membership checks.
pub fn canonicalize(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// Partition an incoming list of `--where` filters into custom-field filters
/// (returned as a map) and remaining filters that should be applied later.
pub fn partition_where_filters(
    filters: &[(String, String)],
    config: &ResolvedConfig,
) -> PartitionedWhereFilters {
    let mut custom: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut remainder: Vec<(String, String)> = Vec::new();
    let mut applied: BTreeSet<String> = BTreeSet::new();

    for (key, value) in filters {
        if let Some(name) = resolve_filter_name(key, config) {
            custom.entry(name.clone()).or_default().push(value.clone());
            applied.insert(canonicalize(&name));
        } else {
            remainder.push((key.clone(), value.clone()));
        }
    }

    (custom, remainder, applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{GlobalConfig, StringConfigField};
    use crate::types::{CustomFields, custom_value_string};

    fn config_with_fields(names: &[&str]) -> ResolvedConfig {
        let global = GlobalConfig {
            custom_fields: StringConfigField {
                values: names.iter().map(|value| value.to_string()).collect(),
            },
            ..GlobalConfig::default()
        };
        ResolvedConfig::from_global(global)
    }

    #[test]
    fn resolve_filter_name_accepts_prefix_and_declared_fields() {
        let config = config_with_fields(&["sprint", "release"]);
        assert_eq!(
            resolve_filter_name("field:sprint", &config),
            Some("sprint".into())
        );
        assert_eq!(
            resolve_filter_name("release", &config),
            Some("release".into())
        );
        assert_eq!(resolve_filter_name("  ", &config), None);
    }

    #[test]
    fn resolve_filter_name_uses_wildcard_when_allowed() {
        let config = config_with_fields(&["*"]);
        assert_eq!(
            resolve_filter_name("unknown", &config),
            Some("unknown".into())
        );
    }

    #[test]
    fn extract_value_strings_matches_case_insensitively() {
        let mut fields: CustomFields = CustomFields::new();
        fields.insert("Iteration".into(), custom_value_string("beta"));

        let direct = extract_value_strings(&fields, "Iteration").unwrap();
        assert_eq!(direct, vec!["beta".to_string()]);

        let case_insensitive = extract_value_strings(&fields, "iteration").unwrap();
        assert_eq!(case_insensitive, vec!["beta".to_string()]);
    }

    #[test]
    fn partition_where_filters_separates_custom_field_entries() {
        let config = config_with_fields(&["sprint"]);
        let filters = vec![
            ("field:sprint".to_string(), "W35".to_string()),
            ("status".to_string(), "Todo".to_string()),
            ("sprint".to_string(), "w35".to_string()),
        ];

        let (custom, remainder, applied) = partition_where_filters(&filters, &config);
        assert_eq!(
            custom.get("sprint"),
            Some(&vec!["W35".to_string(), "w35".to_string()])
        );
        assert_eq!(remainder, vec![("status".to_string(), "Todo".to_string())]);
        assert!(applied.contains(&canonicalize("sprint")));
    }
}
