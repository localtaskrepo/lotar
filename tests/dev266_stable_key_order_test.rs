//! DEV-266: maps in task and config files are written in a stable, sorted
//! order, so unrelated edits do not reorder them.

mod common;

use common::TestFixtures;

fn lotar(fixtures: &TestFixtures, args: &[&str]) {
    let output = common::lotar_cmd()
        .unwrap()
        .current_dir(fixtures.get_temp_path())
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn keys_under(yaml: &str, section: &str) -> Vec<String> {
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml).unwrap();
    let mut node = &value;
    for part in section.split('.') {
        node = &node[part];
    }
    node.as_mapping()
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn custom_fields_keep_a_sorted_order_across_edits() {
    let fixtures = TestFixtures::new();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  project: CF\ncustom:\n  fields: ['*']\n",
    )
    .unwrap();
    let mut args = vec!["add", "CF"];
    let fields = [
        "zeta=6",
        "alpha=1",
        "gamma=3",
        "beta=2",
        "epsilon=5",
        "delta=4",
    ];
    for field in &fields {
        args.push("--field");
        args.push(field);
    }
    lotar(&fixtures, &args);
    let file = fixtures.tasks_root.join("CF").join("1.yml");
    let first = std::fs::read_to_string(&file).unwrap();
    let expected = ["alpha", "beta", "delta", "epsilon", "gamma", "zeta"];
    assert_eq!(keys_under(&first, "custom_fields"), expected);
    for i in 0..4 {
        lotar(&fixtures, &["edit", "CF-1", "--title", &format!("CF {i}")]);
        let now = std::fs::read_to_string(&file).unwrap();
        assert_eq!(keys_under(&now, "custom_fields"), expected, "{now}");
    }
}

#[test]
fn config_alias_maps_keep_a_sorted_order_across_writes() {
    let fixtures = TestFixtures::new();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  project: AL\nbranch:\n  type_aliases: {spike: Feature, fix: Bug, feat: Feature, epic: Feature, chore: Chore}\n",
    )
    .unwrap();
    let mut seen = Vec::new();
    for priority in ["High", "Low", "Medium", "High"] {
        lotar(
            &fixtures,
            &["config", "set", "default_priority", priority, "--global"],
        );
        let text = std::fs::read_to_string(fixtures.tasks_root.join("config.yml")).unwrap();
        seen.push(keys_under(&text, "branch.type_aliases"));
    }
    assert_eq!(seen[0], ["chore", "epic", "feat", "fix", "spike"]);
    assert!(seen.windows(2).all(|w| w[0] == w[1]), "{seen:?}");
}
