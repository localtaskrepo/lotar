//! DEV-269: project prefixes resolve by exact folder names, a different
//! case refers to the existing folder, and generated prefixes don't merge
//! two differently named projects.

mod common;

use lotar::utils::project::resolve_project_input;

fn workspace_with(projects: &[(&str, Option<&str>)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (prefix, name) in projects {
        let dir = tmp.path().join(prefix);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(name) = name {
            std::fs::write(
                dir.join("config.yml"),
                format!("project:\n  name: {name}\n"),
            )
            .unwrap();
        }
    }
    tmp
}

#[test]
fn other_casing_resolves_to_the_existing_folder() {
    let tmp = workspace_with(&[("DEMO", None)]);
    assert_eq!(resolve_project_input("DEMO", tmp.path()), "DEMO");
    assert_eq!(resolve_project_input("demo", tmp.path()), "DEMO");
    assert_eq!(resolve_project_input("Demo", tmp.path()), "DEMO");
}

#[test]
fn generated_prefixes_do_not_merge_differently_named_projects() {
    let tmp = workspace_with(&[("ALPH", Some("Alpha"))]);
    assert_eq!(resolve_project_input("Alpha", tmp.path()), "ALPH");
    assert_eq!(resolve_project_input("alpha", tmp.path()), "ALPH");
    assert_eq!(resolve_project_input("Alphabet", tmp.path()), "ALPH2");

    let unnamed = workspace_with(&[("ALPH", None)]);
    assert_eq!(resolve_project_input("Alphabet", unnamed.path()), "ALPH");
}

#[test]
fn task_ids_only_match_the_exact_folder_case() {
    let fixtures = common::TestFixtures::new();
    common::lotar_cmd()
        .unwrap()
        .current_dir(fixtures.get_temp_path())
        .args(["add", "one", "--project", "DEMO"])
        .assert()
        .success();
    let located = lotar::storage::TaskId::parse("demo-1")
        .unwrap()
        .locate(&fixtures.tasks_root);
    assert!(
        located.is_empty(),
        "demo-1 must not alias DEMO-1: {located:?}"
    );
    let located = lotar::storage::TaskId::parse("DEMO-1")
        .unwrap()
        .locate(&fixtures.tasks_root);
    assert_eq!(located.len(), 1);

    let output = common::lotar_cmd()
        .unwrap()
        .current_dir(fixtures.get_temp_path())
        .args(["add", "two", "--project", "demo"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("DEMO-2"));
}
