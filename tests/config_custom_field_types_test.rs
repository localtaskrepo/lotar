//! Typed custom fields (DEV-373): `{ name, type }` objects inside the
//! `custom.fields` list, kept as a separate map internally, merged per field.

use lotar::config::normalization::{parse_global_from_yaml_str, parse_project_from_yaml_str, to_canonical_global_yaml, to_canonical_project_yaml};
use lotar::config::operations::parse_custom_field_properties;
use lotar::config::types::{CustomFieldType, ResolvedConfig};
use lotar::config::validation::validator::ConfigValidator;

#[test]
fn plain_field_lists_load_and_write_exactly_as_before() {
    let cfg = parse_global_from_yaml_str("custom:\n  fields: [product, sprint]\n").unwrap();
    assert_eq!(cfg.custom_fields.values, vec!["product", "sprint"]);
    assert!(cfg.custom_field_properties.is_empty());
    let yaml = to_canonical_global_yaml(&cfg);
    assert!(yaml.contains("fields:\n  - product\n  - sprint\n"), "{yaml}");
    assert!(!yaml.contains("field_properties"));
    assert!(!yaml.contains("name:"));
    assert_eq!(ResolvedConfig::from_global(cfg).custom_field_type("product"), CustomFieldType::Text);
}

#[test]
fn objects_in_the_field_list_carry_a_type_and_round_trip() {
    let source =
        "custom:\n  fields:\n    - product\n    - { name: affects, type: version }\n    - name: fixed_in\n      type: version\n      future_key: kept\n";
    let cfg = parse_global_from_yaml_str(source).unwrap();
    assert_eq!(cfg.custom_fields.values, vec!["product", "affects", "fixed_in"]);
    assert!(cfg.custom_field_properties["fixed_in"].extra.contains_key("future_key"), "unknown keys survive");

    let yaml = to_canonical_global_yaml(&cfg);
    assert!(yaml.contains("name: affects"), "{yaml}");
    assert!(!yaml.contains("field_properties"), "everything fits in the list: {yaml}");
    assert_eq!(parse_global_from_yaml_str(&yaml).unwrap(), cfg);

    let resolved = ResolvedConfig::from_global(cfg);
    assert_eq!(resolved.custom_field_type("affects"), CustomFieldType::Version);
    assert_eq!(resolved.custom_field_type("Fixed_In"), CustomFieldType::Version);
    assert_eq!(resolved.custom_field_type("product"), CustomFieldType::Text);
}

#[test]
fn a_project_types_inherited_fields_without_replacing_the_list() {
    let project = parse_project_from_yaml_str("DEMO", "custom:\n  field_properties:\n    affects: { type: version }\n").unwrap();
    assert!(project.custom_fields.is_none(), "the field list stays inherited");
    let yaml = to_canonical_project_yaml(&project);
    assert!(yaml.contains("field_properties:"), "{yaml}");
    assert!(!yaml.contains("  fields:"), "{yaml}");
    assert_eq!(parse_project_from_yaml_str("DEMO", &yaml).unwrap(), project);

    let own = parse_project_from_yaml_str("DEMO", "custom:\n  fields: [{ name: affects, type: version }]\n").unwrap();
    assert_eq!(own.custom_fields.as_ref().unwrap().values, vec!["affects"]);
    let yaml = to_canonical_project_yaml(&own);
    assert!(yaml.contains("name: affects"), "{yaml}");
    assert_eq!(parse_project_from_yaml_str("DEMO", &yaml).unwrap(), own);
}

#[test]
fn a_wildcard_list_takes_typed_fields_as_objects() {
    let mut cfg = parse_global_from_yaml_str("").unwrap();
    cfg.custom_field_properties = parse_custom_field_properties("affects=version").unwrap();
    let yaml = to_canonical_global_yaml(&cfg);
    let reparsed = parse_global_from_yaml_str(&yaml).unwrap();
    assert!(reparsed.custom_fields.has_wildcard(), "{yaml}");
    assert_eq!(reparsed.custom_field_properties, cfg.custom_field_properties);
}

#[test]
fn project_properties_override_global_ones_per_field() {
    let global = parse_global_from_yaml_str("custom:\n  fields: ['*', { name: affects, type: version }, { name: build, type: version }]\n").unwrap();
    let mut resolved = ResolvedConfig::from_global(global);
    let project = parse_project_from_yaml_str("DEMO", "custom:\n  field_properties:\n    build: { type: text }\n").unwrap();
    resolved.custom_field_properties.extend(project.custom_field_properties.unwrap_or_default());
    assert_eq!(resolved.custom_field_type("affects"), CustomFieldType::Version);
    assert_eq!(resolved.custom_field_type("build"), CustomFieldType::Text);
}

#[test]
fn config_set_values_accept_maps_and_shorthand_and_check_types() {
    let map = parse_custom_field_properties("{affects: {type: version}}").unwrap();
    assert_eq!(map["affects"].field_type.as_deref(), Some("version"));
    let short = parse_custom_field_properties("affects=Version, fixed_in = version").unwrap();
    assert_eq!(short["affects"].field_type.as_deref(), Some("version"));
    assert_eq!(short.len(), 2);
    assert!(parse_custom_field_properties("").unwrap().is_empty());
    assert!(parse_custom_field_properties("affects=number").is_err());
    assert!(parse_custom_field_properties("A=version, a=text").is_err());
    assert!(parse_custom_field_properties("*=version").is_err());
    assert!(parse_custom_field_properties("status=version").is_err());
}

#[test]
fn field_objects_need_a_name() {
    assert!(parse_global_from_yaml_str("custom:\n  fields:\n    - type: version\n").is_err());
}

#[test]
fn validation_warns_about_unknown_types_and_unlisted_fields() {
    let cfg = parse_global_from_yaml_str("custom:\n  fields: [product, { name: affects, type: semver }]\n  field_properties:\n    other: { type: version }\n")
        .unwrap();
    let result = ConfigValidator::new(std::path::Path::new(".")).validate_global_config(&cfg);
    let messages: Vec<String> = result.warnings.iter().map(|w| w.message.clone()).collect();
    assert!(messages.iter().any(|m| m.contains("unknown type 'semver'")), "{messages:?}");
    assert!(messages.iter().any(|m| m.contains("'other' has properties but is not in custom.fields")), "{messages:?}");
    assert_eq!(ResolvedConfig::from_global(cfg).custom_field_type("affects"), CustomFieldType::Text);
}
