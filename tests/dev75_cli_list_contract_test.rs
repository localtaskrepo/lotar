//! DEV-75: executable list examples and explicit errors for malformed terms.
//! DEV-231: field equalities are written in the web search syntax
//! (`key=value`, `field:name=value`) instead of the removed `--where` flag.

mod common;

use common::TestFixtures;
use serde_json::Value;

fn fixture() -> TestFixtures {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default.project: LIST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug]\nissue.priorities: [Low, Medium, High, Critical]\ncustom_fields: [component, sprint]\n",
    );
    let project = fixtures.tasks_root.join("LIST");
    std::fs::create_dir_all(&project).unwrap();
    for (id, title, extra) in [
        (
            1,
            "TitleQuartz",
            "assignee: Alice\nreporter: ReportOwner\ntags: [release]\ncustom_fields: {component: editor, sprint: 2025-W35, assignee: Shadow}\n",
        ),
        (
            2,
            "Second",
            "subtitle: SubtitleAmber\nassignee: Bob\ncustom_fields: {component: api}\n",
        ),
        (3, "Third", "description: DescriptionCobalt\n"),
        (4, "Fourth", "tags: [TagIndigo]\n"),
        (
            5,
            "Control",
            "assignee: Alice\ncustom_fields: {component: api}\n",
        ),
    ] {
        std::fs::write(
            project.join(format!("{id}.yml")),
            format!("title: {title}\nstatus: Todo\npriority: High\ntype: Bug\ncreated: '2026-01-01T00:00:00Z'\n{extra}"),
        )
        .unwrap();
    }
    fixtures
}

fn list_json(fixtures: &TestFixtures, args: &[&str]) -> Value {
    let mut cmd = common::cargo_bin_in(fixtures);
    let output = cmd
        .args(["--format", "json", "list", "--sort-by", "id", "order=asc"])
        .args(args)
        .assert()
        .success();
    serde_json::from_slice(&output.get_output().stdout).expect("list JSON")
}

fn ids(body: &Value) -> Vec<&str> {
    body["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_str().unwrap())
        .collect()
}

#[test]
fn positional_search_matches_every_documented_text_surface() {
    let fixtures = fixture();
    for (query, expected) in [
        ("list-1", "LIST-1"),
        ("titlequartz", "LIST-1"),
        ("subtitleamber", "LIST-2"),
        ("descriptioncobalt", "LIST-3"),
        ("tagindigo", "LIST-4"),
    ] {
        let body = list_json(&fixtures, &[query, "--priority", "high"]);
        assert_eq!(ids(&body), vec![expected], "query: {query}");
        assert_eq!(body["total"], 1);
    }
}

#[test]
fn documented_custom_and_builtin_equalities_filter_exact_task_sets() {
    let fixtures = fixture();
    let body = list_json(
        &fixtures,
        // `sprint` is also a builtin key, so the custom field is quoted.
        &["component=editor", "field:\"sprint\"=2025-W35"],
    );
    assert_eq!(ids(&body), vec!["LIST-1"]);
    let body = list_json(&fixtures, &["assignee=alice"]);
    assert_eq!(ids(&body), vec!["LIST-1", "LIST-5"]);
    // Values of one field are ORed; a repeated `key=value` replaces.
    let body = list_json(&fixtures, &["component=editor,api"]);
    assert_eq!(ids(&body), vec!["LIST-1", "LIST-2", "LIST-5"]);
    for (predicate, expected) in [
        ("reporter=reportowner", vec!["LIST-1"]),
        ("tag=release", vec!["LIST-1"]),
        ("status=Done", vec![]),
        ("priority=Low", vec![]),
        ("type=Feature", vec![]),
        ("project=OTHER", vec![]),
    ] {
        assert_eq!(ids(&list_json(&fixtures, &[predicate])), expected);
    }
}

#[test]
fn reserved_where_fields_are_not_custom_fields_under_wildcard_config() {
    let fixtures = fixture();
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default.project: LIST\ncustom_fields: ['*']\n",
    );
    let body = list_json(&fixtures, &["assignee=alice"]);
    assert_eq!(ids(&body), vec!["LIST-1", "LIST-5"]);
}

#[test]
fn explicit_custom_fields_keep_literal_reserved_names_and_case_insensitive_keys() {
    let fixtures = fixture();
    // A custom field named like a builtin is quoted, as in the web UI.
    assert_eq!(
        ids(&list_json(&fixtures, &["field:\"ASSIGNEE\"=shadow"])),
        vec!["LIST-1"]
    );
    assert_eq!(
        ids(&list_json(&fixtures, &["AssIgNee=alice"])),
        vec!["LIST-1", "LIST-5"]
    );
    assert_eq!(
        ids(&list_json(
            &fixtures,
            &["Component=editor", "field:COMPONENT=api"]
        )),
        vec!["LIST-1", "LIST-2", "LIST-5"]
    );
}

#[test]
fn malformed_terms_fail_even_when_the_workspace_is_empty() {
    let fixtures = fixture();
    std::fs::remove_dir_all(fixtures.tasks_root.join("LIST")).unwrap();
    for (term, expected_error) in [
        ("assignee=", "Add a value for \"assignee\""),
        ("=Alice", "Missing key before \"=\""),
        ("field:=editor", "Invalid filter name"),
        ("status:\"open", "Unmatched quote"),
    ] {
        let mut cmd = common::cargo_bin_in(&fixtures);
        let output = cmd.args(["list", term]).assert().failure();
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&output.get_output().stdout),
            String::from_utf8_lossy(&output.get_output().stderr),
        );
        assert!(diagnostic.contains(expected_error), "{term}: {diagnostic}");
    }
    common::cargo_bin_in(&fixtures)
        .args(["list", "--where", "assignee=alice"])
        .assert()
        .failure();
}

#[test]
fn unsupported_search_flag_is_not_a_silent_filter() {
    let fixtures = fixture();
    common::cargo_bin_in(&fixtures)
        .args(["list", "--search=TitleQuartz"])
        .assert()
        .failure();
}

#[test]
fn list_help_matches_the_actual_predicate_and_format_contract() {
    let fixtures = fixture();
    let output = common::cargo_bin_in(&fixtures)
        .args(["list", "--help"])
        .assert()
        .success();
    let text = String::from_utf8_lossy(&output.get_output().stdout);
    for fragment in [
        "same syntax as the web UI search box",
        // DEV-167 search rules and the fields they cover.
        "Every word must match",
        "ID, title, subtitle, description, acceptance criteria, tags, references, and comments",
        "`key:value` adds to a list filter; `key=value` replaces it",
        "no aligned table or Markdown table is generated",
    ] {
        assert!(text.contains(fragment), "missing help contract: {fragment}");
    }
}

#[test]
fn valid_and_rejected_list_queries_do_not_rewrite_tasks_or_configuration() {
    let fixtures = fixture();
    let mut paths: Vec<_> = std::fs::read_dir(fixtures.tasks_root.join("LIST"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.push(fixtures.tasks_root.join("config.yml"));
    let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800);
    let before: Vec<_> = paths
        .iter()
        .map(|path| {
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(old)
                .unwrap();
            (
                std::fs::read(path).unwrap(),
                std::fs::metadata(path).unwrap().modified().unwrap(),
            )
        })
        .collect();
    list_json(&fixtures, &["titlequartz", "component=editor"]);
    common::cargo_bin_in(&fixtures)
        .args(["list", "assignee="])
        .assert()
        .failure();
    for (path, expected) in paths.iter().zip(before) {
        assert_eq!(
            (
                std::fs::read(path).unwrap(),
                std::fs::metadata(path).unwrap().modified().unwrap()
            ),
            expected,
            "list query rewrote {}",
            path.display()
        );
    }
}

#[test]
fn table_and_markdown_aliases_produce_the_same_text_output() {
    let fixtures = fixture();
    let mut baseline = None;
    for format in ["text", "table", "markdown", "md"] {
        let mut cmd = common::cargo_bin_in(&fixtures);
        let output = cmd
            .args([
                "--format",
                format,
                "list",
                "--sort-by",
                "id",
                "order=asc",
                "--page-size",
                "2",
            ])
            .assert()
            .success();
        let stdout = output.get_output().stdout.clone();
        let text = String::from_utf8_lossy(&stdout);
        assert!(text.contains("LIST-1 - TitleQuartz"));
        assert!(text.contains("page 1 of 3"));
        assert!(!text.contains("LIST-3"));
        if let Some(expected) = &baseline {
            assert_eq!(&stdout, expected, "format alias: {format}");
        } else {
            baseline = Some(stdout);
        }
    }
}
