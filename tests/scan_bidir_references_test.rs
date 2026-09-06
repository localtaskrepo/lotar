use predicates::prelude::*;
use std::fs;

mod common;
use common::TestFixtures;

fn request(path: &std::path::Path) -> lotar::api_types::ScanRequest {
    lotar::api_types::ScanRequest {
        paths: vec![path.display().to_string()],
        include: vec![],
        exclude: vec![],
        project: Some("SAFE".to_string()),
        dry_run: false,
        strip_attributes: Some(true),
        reanchor: true,
        modified_only: false,
        targets: vec![],
    }
}

#[test]
fn quoted_comment_tokens_never_create_tasks_or_edit_literals() {
    let fixtures = [
        ("main.js", r#"const example = "/* TODO: document this */";"#),
        ("main.js", r#"const example = '/* TODO: document this */';"#),
        (
            "main.js",
            r#"const example = "escaped \" // TODO: literal [priority=high]";"#,
        ),
        (
            "main.js",
            r#"const example = 'escaped \' /* TODO: literal */';"#,
        ),
        ("main.py", "example = \"\"\"\n# TODO: literal\n\"\"\"\n"),
        (
            "main.rs",
            "let example = r###\"\n\" /* TODO: raw literal */\n\"###;\n",
        ),
        (
            "main.js",
            "const example = `\n// TODO: template literal\n`;\n",
        ),
        (
            "main.js",
            "const example = \"unterminated\n// TODO: ambiguous\n",
        ),
        (
            "main.ps1",
            r#"$example = "escaped `" # TODO: literal [priority=high]""#,
        ),
        (
            "main.html",
            r#"<div a='slash\' b='<!-- TODO: literal -->'></div>"#,
        ),
    ];
    for service in [false, true] {
        for strip in [false, true] {
            for (file, original) in fixtures {
                let tf = TestFixtures::new();
                let source = tf.temp_dir.path().join(file);
                fs::write(&source, original).unwrap();
                if service {
                    let resolver = lotar::workspace::TasksDirectoryResolver::resolve(
                        Some(tf.tasks_root.to_str().unwrap()),
                        None,
                    )
                    .unwrap();
                    let mut scan = request(&source);
                    scan.strip_attributes = Some(strip);
                    let response =
                        lotar::services::scan_service::ScanService::run(&resolver, scan).unwrap();
                    assert!(response.entries.is_empty(), "{original}");
                } else {
                    common::cargo_bin_in(&tf)
                        .args([
                            "scan",
                            if strip {
                                "--strip-attributes=true"
                            } else {
                                "--strip-attributes=false"
                            },
                        ])
                        .assert()
                        .success();
                }
                assert_eq!(fs::read_to_string(&source).unwrap(), original);
                assert!(
                    tf.create_storage().search(&Default::default()).is_empty(),
                    "{original}"
                );
            }
        }
    }
}

#[test]
fn metadata_and_ticket_detection_ignore_literals_before_real_comment() {
    let fixtures = [
        (
            "main.js",
            r#"const example = "/* TODO: fake [priority=low] [ticket=FAKE-42] */"; "#,
            "//",
        ),
        (
            "main.js",
            r#"const example = 'escaped \' // TODO: fake [priority=low]'; "#,
            "//",
        ),
        (
            "main.py",
            r#"example = "escaped \" # TODO: fake [priority=low]"; "#,
            "#",
        ),
        (
            "main.sql",
            "SELECT 'escaped '' -- TODO: fake [priority=low]'; ",
            "--",
        ),
        (
            "main.ps1",
            r#"$example = "escaped `" # TODO: fake [priority=low]"; "#,
            "#",
        ),
    ];
    for service in [false, true] {
        for strip in [false, true] {
            for (file, prefix, token) in fixtures {
                let tf = TestFixtures::new();
                let source = tf.temp_dir.path().join(file);
                let original = format!("{prefix}{token} TODO: real [priority=high]\n");
                fs::write(&source, &original).unwrap();
                if service {
                    let resolver = lotar::workspace::TasksDirectoryResolver::resolve(
                        Some(tf.tasks_root.to_str().unwrap()),
                        None,
                    )
                    .unwrap();
                    let mut scan = request(&source);
                    scan.strip_attributes = Some(strip);
                    let response =
                        lotar::services::scan_service::ScanService::run(&resolver, scan).unwrap();
                    assert_eq!(response.summary.created, 1, "{response:?}");
                    assert_eq!(response.summary.failed, 0);
                } else {
                    common::cargo_bin_in(&tf)
                        .args([
                            "scan",
                            if strip {
                                "--strip-attributes=true"
                            } else {
                                "--strip-attributes=false"
                            },
                        ])
                        .assert()
                        .success();
                }
                let tasks = tf.create_storage().search(&Default::default());
                assert_eq!(tasks.len(), 1);
                assert!(tasks[0].1.priority.as_str().eq_ignore_ascii_case("high"));
                assert_eq!(
                    fs::read_to_string(&source).unwrap(),
                    format!(
                        "{prefix}{token} TODO ({}): real {}\n",
                        tasks[0].0,
                        if strip { "" } else { "[priority=high]" }
                    )
                );
            }
        }
    }
}

#[test]
fn bare_and_dot_relative_source_paths_work_in_cli_and_service() {
    let original_cwd = std::env::current_dir().unwrap();
    for service in [false, true] {
        for path in ["main.rs", "./main.rs"] {
            let tf = TestFixtures::new();
            let source = tf.temp_dir.path().join("main.rs");
            fs::write(&source, "// TODO: relative path\n").unwrap();
            if service {
                let resolver = lotar::workspace::TasksDirectoryResolver::resolve(
                    Some(tf.tasks_root.to_str().unwrap()),
                    None,
                )
                .unwrap();
                std::env::set_current_dir(tf.temp_dir.path()).unwrap();
                let response = lotar::services::scan_service::ScanService::run(
                    &resolver,
                    request(std::path::Path::new(path)),
                )
                .unwrap();
                std::env::set_current_dir(&original_cwd).unwrap();
                assert_eq!(response.summary.created, 1, "{response:?}");
                assert_eq!(response.summary.failed, 0);
            } else {
                common::cargo_bin_in(&tf)
                    .args(["scan", path])
                    .assert()
                    .success();
            }
            assert_eq!(tf.create_storage().search(&Default::default()).len(), 1);
            assert!(fs::read_to_string(source).unwrap().contains("TODO ("));
        }
    }
}

#[test]
fn json_and_text_apply_equally_and_dry_run_never_writes() {
    for format in ["text", "json"] {
        let tf = TestFixtures::new();
        let source = tf.temp_dir.path().join("main.js");
        let original = "const [value = 1] = values; // TODO: repair [priority=high]\n";
        fs::write(&source, original).unwrap();
        let mut cmd = common::cargo_bin_in(&tf);
        let preview = cmd
            .args(["--format", format, "scan", "--dry-run"])
            .assert()
            .success();
        if format == "json" {
            let entries: serde_json::Value =
                serde_json::from_slice(&preview.get_output().stdout).unwrap();
            assert_eq!(entries.as_array().unwrap().len(), 1);
        }
        assert_eq!(fs::read_to_string(&source).unwrap(), original);
        assert!(tf.create_storage().search(&Default::default()).is_empty());
        let mut cmd = common::cargo_bin_in(&tf);
        let applied = cmd.args(["--format", format, "scan"]).assert().success();
        if format == "json" {
            let entries: serde_json::Value =
                serde_json::from_slice(&applied.get_output().stdout).unwrap();
            assert_eq!(entries.as_array().unwrap().len(), 1);
        }
        let tasks = tf.create_storage().search(&Default::default());
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].1.priority.as_str(), "High");
        assert_eq!(
            fs::read_to_string(&source).unwrap(),
            format!(
                "const [value = 1] = values; // TODO ({}): repair \n",
                tasks[0].0
            )
        );
    }
}

#[test]
fn cli_and_service_reanchor_preserve_mixed_and_repeated_references() {
    use lotar::types::ReferenceEntry;
    for service in [false, true] {
        let tf = TestFixtures::new();
        let source = tf.temp_dir.path().join("main.rs");
        let mut storage = tf.create_storage();
        let task = lotar::Task::new(
            tf.tasks_root.clone(),
            "anchors".to_string(),
            lotar::types::Priority::new("medium"),
        );
        let id = storage.add(&task, "SAFE", None).unwrap();
        fs::write(
            &source,
            format!("// TODO ({id}): first\n// TODO ({id}): second\n"),
        )
        .unwrap();
        let file = if service {
            lotar::utils::paths::repo_relative_display(&source)
        } else {
            "main.rs".to_string()
        };
        let mut task = storage.get(&id, "SAFE").unwrap();
        task.references = vec![
            ReferenceEntry {
                link: Some("https://example.test/spec".into()),
                ..Default::default()
            },
            ReferenceEntry {
                code: Some("elsewhere.rs#9".into()),
                github: Some("owner/repo#1".into()),
                ..Default::default()
            },
            ReferenceEntry {
                code: Some(format!("{file}#1")),
                ..Default::default()
            },
            ReferenceEntry {
                code: Some(format!("{file}#2")),
                ..Default::default()
            },
            ReferenceEntry {
                code: Some(format!("{file}#99")),
                jira: Some("EXT-1".into()),
                file: Some("design.pdf".into()),
                ..Default::default()
            },
        ];
        storage.edit(&id, &task).unwrap();
        for _ in 0..2 {
            if service {
                let resolver = lotar::workspace::TasksDirectoryResolver::resolve(
                    Some(tf.tasks_root.to_str().unwrap()),
                    None,
                )
                .unwrap();
                let result =
                    lotar::services::scan_service::ScanService::run(&resolver, request(&source))
                        .unwrap();
                assert_eq!(result.summary.failed, 0);
            } else {
                common::cargo_bin_in(&tf)
                    .args(["scan", "--reanchor"])
                    .assert()
                    .success();
            }
        }
        let updated = tf.create_storage().get(&id, "SAFE").unwrap();
        assert_eq!(updated.references.len(), 5);
        assert_eq!(&updated.references[..4], &task.references[..4]);
        assert_eq!(
            updated.references[4].code.as_deref(),
            Some(format!("{file}#1").as_str())
        );
        assert_eq!(updated.references[4].jira, task.references[4].jira);
        assert_eq!(updated.references[4].file, task.references[4].file);
    }
}

#[test]
fn read_only_source_fails_without_task_or_source_mutation() {
    for service in [false, true] {
        let tf = TestFixtures::new();
        let source = tf.temp_dir.path().join("readonly.rs");
        let original = "// TODO: must not create an orphan\n";
        fs::write(&source, original).unwrap();
        let mut permissions = fs::metadata(&source).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&source, permissions).unwrap();
        if service {
            let resolver = lotar::workspace::TasksDirectoryResolver::resolve(
                Some(tf.tasks_root.to_str().unwrap()),
                None,
            )
            .unwrap();
            let response =
                lotar::services::scan_service::ScanService::run(&resolver, request(&source))
                    .unwrap();
            assert_eq!(response.summary.created, 0);
            assert_eq!(response.summary.failed, 1);
            assert!(response.entries[0].task_id.is_none());
        } else {
            common::cargo_bin_in(&tf)
                .args(["--format", "json", "scan"])
                .assert()
                .failure();
        }
        assert_eq!(fs::read_to_string(&source).unwrap(), original);
        assert!(tf.create_storage().search(&Default::default()).is_empty());
    }
}

#[test]
fn failed_atomic_source_write_compensates_created_task() {
    let tf = TestFixtures::new();
    let mut storage = tf.create_storage();
    let existing = lotar::Task::new(
        tf.tasks_root.clone(),
        "untouched".to_string(),
        lotar::types::Priority::new("medium"),
    );
    let existing_id = storage.add(&existing, "SAFE", None).unwrap();
    let existing_path = tf.tasks_root.join("SAFE/1.yml");
    let existing_bytes = fs::read(&existing_path).unwrap();
    let source = tf.temp_dir.path().join("main.rs");
    let original = "// TODO: must roll back\n";
    fs::write(&source, original).unwrap();
    // Reserve the first staging name in this nextest process. Preflight succeeds,
    // but create_new fails only after the task and its reverse reference exist.
    let reserved = tf
        .temp_dir
        .path()
        .join(format!(".lotar-scan-{}-0", std::process::id()));
    fs::write(&reserved, "reserved").unwrap();
    let resolver = lotar::workspace::TasksDirectoryResolver::resolve(
        Some(tf.tasks_root.to_str().unwrap()),
        None,
    )
    .unwrap();
    let response =
        lotar::services::scan_service::ScanService::run(&resolver, request(&source)).unwrap();
    assert_eq!(response.status, "partial");
    assert_eq!(response.summary.created, 0);
    assert_eq!(response.summary.failed, 1);
    assert!(response.entries[0].task_id.is_none());
    assert!(
        response.entries[0]
            .message
            .as_deref()
            .unwrap()
            .contains("Failed to write")
    );
    let tasks = tf.create_storage().search(&Default::default());
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].0, existing_id);
    assert_eq!(fs::read(&existing_path).unwrap(), existing_bytes);
    assert_eq!(fs::read_to_string(&source).unwrap(), original);
    assert_eq!(fs::read_to_string(&reserved).unwrap(), "reserved");
}

#[test]
fn metadata_stripping_is_confined_to_comment_segment() {
    let scanner = lotar::scanner::Scanner::new(std::path::PathBuf::from("."));
    for (input, expected) in [
        (
            "const [value = 1] = xs; // TODO: fix [priority=high] [not metadata =]",
            "const [value = 1] = xs; // TODO: fix  [not metadata =]",
        ),
        (
            "/* TODO: fix [tag=scan] */ const [value = 1] = xs;",
            "/* TODO: fix  */ const [value = 1] = xs;",
        ),
        (
            "// TODO: fix [broken=[value=1]] [tag=scan]",
            "// TODO: fix [broken=[value=1]] ",
        ),
    ] {
        assert_eq!(scanner.strip_scan_attributes(input), expected);
    }
}

#[test]
fn scan_creates_task_with_source_reference() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Create a simple source file with a TODO missing a key
    let src = r#"// TODO: connect bi-dir link test"#;
    let file_path = root.join("main.rs");
    fs::write(&file_path, src).unwrap();
    let canon_path = fs::canonicalize(&file_path).unwrap();
    let canon_str = canon_path.display().to_string();

    // Run scan (apply-by-default)
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));

    // Determine project folder (default)
    let tasks_dir = root.join(".tasks");
    // The effective default project folder is derived; list dirs under .tasks and pick one
    let mut projects = std::fs::read_dir(&tasks_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    projects.sort();
    assert!(
        !projects.is_empty(),
        "expected a project folder under .tasks"
    );
    let project = &projects[0];

    // Find the created task file (1.yml)
    let task_file = tasks_dir.join(project).join("1.yml");
    assert!(
        task_file.exists(),
        "expected {} to exist",
        task_file.display()
    );
    let yaml = fs::read_to_string(&task_file).unwrap();

    // Verify references contains a code entry with file path and #1 anchor
    assert!(
        yaml.contains("references:"),
        "expected references in YAML: {yaml}"
    );
    // Path with anchor: accept canonical absolute form or just main.rs
    let anchor1 = format!("code: {canon_str}#1");
    assert!(
        yaml.contains(&anchor1) || yaml.contains("code: main.rs#1"),
        "expected code reference with #1 in YAML: {yaml}"
    );
}
