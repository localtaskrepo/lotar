//! Tag properties such as colours (DEV-20): written as objects inside the
//! `issue.tags` list, kept as a separate map internally, merged per tag.

use lotar::config::normalization::{
    parse_global_from_yaml_str, parse_project_from_yaml_str, to_canonical_global_yaml,
    to_canonical_project_yaml,
};
use lotar::config::operations::parse_tag_properties;
use lotar::config::types::{ResolvedConfig, TagProperties};
use lotar::config::validation::validator::ConfigValidator;

fn color(props: &std::collections::BTreeMap<String, TagProperties>, tag: &str) -> Option<String> {
    props.get(tag).and_then(|p| p.color.clone())
}

#[test]
fn plain_tag_lists_load_and_write_exactly_as_before() {
    let cfg = parse_global_from_yaml_str("issue:\n  tags: [bug, ui]\n").unwrap();
    assert_eq!(cfg.tags.values, vec!["bug", "ui"]);
    assert!(cfg.tag_properties.is_empty());
    let yaml = to_canonical_global_yaml(&cfg);
    assert!(yaml.contains("tags:\n  - bug\n  - ui\n"), "{yaml}");
    assert!(!yaml.contains("tag_properties"));
    assert!(!yaml.contains("name:"));
}

#[test]
fn objects_in_the_tag_list_carry_properties_and_round_trip() {
    let source = "issue:\n  tags:\n    - bug\n    - name: ui\n      color: '#0ea5e9'\n      future_key: kept\n    - name: v1.2\n      color: teal\n";
    let cfg = parse_global_from_yaml_str(source).unwrap();
    assert_eq!(cfg.tags.values, vec!["bug", "ui", "v1.2"]);
    assert_eq!(color(&cfg.tag_properties, "ui").as_deref(), Some("#0ea5e9"));
    assert_eq!(color(&cfg.tag_properties, "v1.2").as_deref(), Some("teal"));
    assert!(
        cfg.tag_properties["ui"].extra.contains_key("future_key"),
        "unknown keys survive"
    );

    let yaml = to_canonical_global_yaml(&cfg);
    assert!(yaml.contains("- bug\n"), "{yaml}");
    assert!(yaml.contains("name: ui"), "{yaml}");
    assert!(
        !yaml.contains("tag_properties"),
        "everything fits in the list: {yaml}"
    );
    assert_eq!(parse_global_from_yaml_str(&yaml).unwrap(), cfg);
}

#[test]
fn a_wildcard_list_takes_coloured_tags_as_objects() {
    let mut cfg = parse_global_from_yaml_str("").unwrap();
    cfg.tag_properties = parse_tag_properties("ui=#0ea5e9").unwrap();
    let yaml = to_canonical_global_yaml(&cfg);
    assert!(yaml.contains("- '*'") || yaml.contains("- \"*\""), "{yaml}");
    let reparsed = parse_global_from_yaml_str(&yaml).unwrap();
    assert!(reparsed.tags.has_wildcard());
    assert_eq!(
        color(&reparsed.tag_properties, "ui").as_deref(),
        Some("#0ea5e9")
    );
}

#[test]
fn a_project_colours_inherited_tags_without_replacing_the_list() {
    let project = parse_project_from_yaml_str(
        "DEMO",
        "issue:\n  tag_properties:\n    ui: { color: green }\n",
    )
    .unwrap();
    assert!(project.tags.is_none(), "the tag list stays inherited");
    assert_eq!(
        color(project.tag_properties.as_ref().unwrap(), "ui").as_deref(),
        Some("green")
    );
    let yaml = to_canonical_project_yaml(&project);
    assert!(yaml.contains("tag_properties:"), "{yaml}");
    assert!(!yaml.contains("  tags:"), "{yaml}");
    assert_eq!(parse_project_from_yaml_str("DEMO", &yaml).unwrap(), project);
}

#[test]
fn project_properties_override_global_ones_per_tag() {
    let global = parse_global_from_yaml_str(
        "issue:\n  tags: ['*', { name: ui, color: blue }, { name: api, color: red }]\n",
    )
    .unwrap();
    let mut resolved = ResolvedConfig::from_global(global);
    let project = parse_project_from_yaml_str(
        "DEMO",
        "issue:\n  tag_properties:\n    ui: { color: green }\n",
    )
    .unwrap();
    // Same order as project resolution: project entries win per tag.
    resolved
        .tag_properties
        .extend(project.tag_properties.unwrap_or_default());
    assert_eq!(
        color(&resolved.tag_properties, "ui").as_deref(),
        Some("green")
    );
    assert_eq!(
        color(&resolved.tag_properties, "api").as_deref(),
        Some("red")
    );
}

#[test]
fn config_set_values_accept_maps_and_shorthand_and_check_colours() {
    let map = parse_tag_properties("{ui: {color: \"#0EA5E9\"}, api: {color: teal}}").unwrap();
    assert_eq!(color(&map, "ui").as_deref(), Some("#0EA5E9"));
    let short = parse_tag_properties("ui=#0ea5e9, api = teal").unwrap();
    assert_eq!(color(&short, "api").as_deref(), Some("teal"));
    assert!(parse_tag_properties("").unwrap().is_empty());
    assert!(parse_tag_properties("ui=chartreuse").is_err());
    assert!(parse_tag_properties("ui=#12345").is_err());
    assert!(parse_tag_properties("UI=red, ui=blue").is_err());
    assert!(parse_tag_properties("*=red").is_err());
}

#[test]
fn tag_objects_need_a_name() {
    assert!(parse_global_from_yaml_str("issue:\n  tags:\n    - color: red\n").is_err());
}

#[test]
fn validation_warns_about_unknown_colours_and_unlisted_tags() {
    let cfg = parse_global_from_yaml_str(
        "issue:\n  tags: [bug, { name: ui, color: chartreuse }]\n  tag_properties:\n    api: { color: red }\n",
    )
    .unwrap();
    let result = ConfigValidator::new(std::path::Path::new(".")).validate_global_config(&cfg);
    let messages: Vec<String> = result.warnings.iter().map(|w| w.message.clone()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("unknown colour 'chartreuse'")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("'api' has properties but is not in the tag list")),
        "{messages:?}"
    );
}
