use lotar::config::normalization::{parse_global_from_yaml_str, to_canonical_global_yaml};
use lotar::config::types::GlobalConfig;

#[test]
fn parse_and_render_sprint_defaults_and_notifications() {
    let yaml = r#"
    sprints:
      defaults:
        capacity_points: 40
        capacity_hours: 320
        length: "2w"
        overdue_after: "24h"
      notifications:
        enabled: false
    "#;

    let cfg = parse_global_from_yaml_str(yaml).expect("parse config");
    assert_eq!(cfg.sprints.defaults.capacity_points, Some(40));
    assert_eq!(cfg.sprints.defaults.capacity_hours, Some(320));
    assert_eq!(cfg.sprints.defaults.length.as_deref(), Some("2w"));
    assert_eq!(cfg.sprints.defaults.overdue_after.as_deref(), Some("24h"));
    assert!(!cfg.sprints.notifications.enabled);

    let rendered = to_canonical_global_yaml(&cfg);
    assert!(rendered.contains("sprints:"));
    assert!(rendered.contains("capacity_points: 40"));
    assert!(rendered.contains("capacity_hours: 320"));
    assert!(rendered.contains("length: 2w"));
    assert!(rendered.contains("overdue_after: 24h"));
    assert!(rendered.contains("enabled: false"));
}

#[test]
fn sprint_section_omitted_when_defaults_unused() {
    let cfg = GlobalConfig::default();
    assert!(cfg.sprints.notifications.enabled);

    let rendered = to_canonical_global_yaml(&cfg);
    assert!(!rendered.contains("sprints:"));
}
