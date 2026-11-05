//! DEV-268: unknown keys nested in task and sprint files survive edits.

use crate::common;
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

#[test]
fn nested_task_extensions_survive_an_edit() {
    let fixtures = TestFixtures::new();
    let dir = fixtures.tasks_root.join("NX");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  project: NX\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("1.yml"),
        "title: Nested extras\nstatus: Todo\npriority: Medium\ntype: Feature\n\
         created: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\n\
         relationships:\n  depends_on: [NX-2]\n  implements: [NX-3]\n\
         comments:\n  - date: 2026-01-02T00:00:00Z\n    text: Hello\n    author: alice\n\
         references:\n  - link: https://example.com\n    title: Example site\n\
         history:\n  - at: 2026-01-01T00:00:00Z\n    source: import\n\
         x_top: keep-me\n",
    )
    .unwrap();

    lotar(&fixtures, &["edit", "NX-1", "--title", "Nested extras 2"]);

    let yaml: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&std::fs::read_to_string(dir.join("1.yml")).unwrap()).unwrap();
    assert_eq!(yaml["title"], "Nested extras 2");
    assert_eq!(yaml["x_top"], "keep-me");
    assert_eq!(yaml["relationships"]["implements"][0], "NX-3");
    assert_eq!(yaml["comments"][0]["author"], "alice");
    assert_eq!(yaml["references"][0]["title"], "Example site");
    assert_eq!(yaml["history"][0]["source"], "import");
}

#[test]
fn sprint_extensions_survive_an_update() {
    let fixtures = TestFixtures::new();
    let sprints = fixtures.tasks_root.join("@sprints");
    std::fs::create_dir_all(&sprints).unwrap();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  project: HD\n",
    )
    .unwrap();
    std::fs::write(
        sprints.join("1.yml"),
        "plan:\n  label: One\n  color: red\ntasks:\n  - id: HD-1\n    note: first\n",
    )
    .unwrap();

    lotar(&fixtures, &["sprint", "update", "1", "--goal", "Ship it"]);

    let yaml: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&std::fs::read_to_string(sprints.join("1.yml")).unwrap()).unwrap();
    assert_eq!(yaml["plan"]["goal"], "Ship it");
    assert_eq!(yaml["plan"]["color"], "red");
    assert_eq!(yaml["tasks"][0]["id"], "HD-1");
    assert_eq!(yaml["tasks"][0]["note"], "first");
}
