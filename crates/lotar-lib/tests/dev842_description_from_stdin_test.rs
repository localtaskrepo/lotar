//! DEV-842: `-D -` reads the description from stdin, like `comment -F -`
//! (DEV-776), in `add`, `task add` and `task edit`.

use crate::common;
use common::TestFixtures;

fn run(fixtures: &TestFixtures, args: &[&str], stdin: &str) -> std::process::Output {
    let out = common::lotar_cmd().unwrap().current_dir(fixtures.get_temp_path()).write_stdin(stdin).args(args).output().unwrap();
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    out
}

fn description(fixtures: &TestFixtures, id: u32) -> String {
    let yaml = std::fs::read_to_string(fixtures.tasks_root.join("DESC").join(format!("{id}.yml"))).unwrap();
    let task: serde_yaml_ng::Value = serde_yaml_ng::from_str(&yaml).unwrap();
    task["description"].as_str().unwrap_or_default().to_string()
}

#[test]
fn a_dash_description_reads_stdin_in_add_and_edit() {
    let fixtures = TestFixtures::new();
    std::fs::write(fixtures.tasks_root.join("config.yml"), "default:\n  project: DESC\n").unwrap();

    run(&fixtures, &["add", "one", "-D", "-"], "from add\n\n## Done when\n");
    assert_eq!(description(&fixtures, 1), "from add\n\n## Done when");
    run(&fixtures, &["task", "add", "two", "--description", "-"], "from task add\n");
    assert_eq!(description(&fixtures, 2), "from task add");

    run(&fixtures, &["task", "edit", "DESC-1", "-D", "-"], "from edit\n");
    assert_eq!(description(&fixtures, 1), "from edit");

    // `--select`: stdin is read once and every match gets it.
    run(&fixtures, &["edit", "--select", "status:todo", "-D", "-"], "for all\n");
    assert_eq!(description(&fixtures, 1), "for all");
    assert_eq!(description(&fixtures, 2), "for all");

    // Other values, also ones starting with `-`, stay as given.
    run(&fixtures, &["task", "edit", "DESC-2", "-D", "- a list item"], "ignored\n");
    assert_eq!(description(&fixtures, 2), "- a list item");
}

#[test]
fn an_unreadable_stdin_fails_and_writes_nothing() {
    let fixtures = TestFixtures::new();
    std::fs::write(fixtures.tasks_root.join("config.yml"), "default:\n  project: DESC\n").unwrap();
    run(&fixtures, &["add", "one", "-D", "orig"], "");

    let not_utf8: &[u8] = &[0xff, 0xfe, b'\n'];
    for args in [&["add", "two", "-D", "-"][..], &["task", "edit", "DESC-1", "-D", "-"][..]] {
        let out = common::lotar_cmd().unwrap().current_dir(fixtures.get_temp_path()).write_stdin(not_utf8).args(args).output().unwrap();
        assert!(!out.status.success(), "{args:?} should fail");
        assert!(String::from_utf8_lossy(&out.stderr).contains("stdin"), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
    assert_eq!(description(&fixtures, 1), "orig");
    assert!(!fixtures.tasks_root.join("DESC").join("2.yml").exists());
}
