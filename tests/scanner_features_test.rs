// Scanner consolidation suite: block_comments, custom_patterns, inline_effort, insertion_suggestion,
// signal_words, ticket_extraction, words_toggle

mod common;
use common::{TestFixtures, cargo_bin_in};

mod block_comments {
    use predicates::prelude::*;
    use std::fs;

    use super::{TestFixtures, cargo_bin_in};

    #[test]
    fn scan_js_block_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(
            root.join("file.js"),
            "/*\n * TODO: inside block js\n */\n const x = 1;",
        )
        .unwrap();

        let _cmd = cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_rust_block_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(
            root.join("file.rs"),
            "/* TODO: block in rust */\nfn main() {}",
        )
        .unwrap();

        let _cmd = cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }
}

mod custom_patterns {
    use lotar::scanner::Scanner;
    use std::fs;
    use std::path::PathBuf;

    fn write_file(root: &std::path::Path, rel: &str, contents: &str) -> std::path::PathBuf {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, contents).unwrap();
        p
    }

    #[test]
    fn custom_ticket_pattern_captures_group_1() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_file(root, "src/a.rs", "// TODO: BLZ_2025-999 do something");

        let patterns = vec![r"\b(BLZ_\d{4}-\d{3})\b".to_string()];
        let mut scanner =
            Scanner::new(PathBuf::from(root)).with_ticket_detection(Some(&patterns), false);
        let refs = scanner.scan();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].uuid, "BLZ_2025-999");
    }

    #[test]
    fn invalid_patterns_are_ignored_gracefully() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_file(root, "src/c.rs", "// TODO: AUTH-321 fix");

        let patterns = vec!["[".to_string()];
        let mut scanner =
            Scanner::new(PathBuf::from(root)).with_ticket_detection(Some(&patterns), false);
        let refs = scanner.scan();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].uuid, "AUTH-321");
    }
}

mod inline_effort {
    use serde_yaml::Value as Y;
    use std::fs;

    use super::{TestFixtures, cargo_bin_in};

    fn read_task_yaml_effort(root: &std::path::Path) -> String {
        let tasks_dir = root.join(".tasks");
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

        let task_file = tasks_dir.join(project).join("1.yml");
        assert!(
            task_file.exists(),
            "expected {} to exist",
            task_file.display()
        );
        let yaml = fs::read_to_string(&task_file).unwrap();
        let parsed: Y = serde_yaml::from_str(&yaml).expect("valid yaml");
        parsed
            .get("effort")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn scan_inline_effort_minutes_normalized_to_hours() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        let src = r#"// TODO: implement foo [effort=90m]"#;
        fs::write(root.join("main.rs"), src).unwrap();

        let _cmd = cargo_bin_in(&tf)
            .env("LOTAR_TEST_SILENT", "1")
            .arg("scan")
            .assert()
            .success();

        let effort = read_task_yaml_effort(root);
        assert_eq!(effort, "1.50h");
    }

    #[test]
    fn scan_inline_effort_combined_tokens() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        let src = r#"// TODO: handle bar [effort=1h 30m]"#;
        fs::write(root.join("lib.rs"), src).unwrap();

        let _cmd = cargo_bin_in(&tf)
            .env("LOTAR_TEST_SILENT", "1")
            .arg("scan")
            .assert()
            .success();

        let effort = read_task_yaml_effort(root);
        assert_eq!(effort, "1.50h");
    }

    #[test]
    fn scan_inline_effort_points_preserved() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        let src = r#"// TODO: estimate baz [effort=3pt]"#;
        fs::write(root.join("mod.rs"), src).unwrap();

        let _cmd = cargo_bin_in(&tf)
            .env("LOTAR_TEST_SILENT", "1")
            .arg("scan")
            .assert()
            .success();

        let effort = read_task_yaml_effort(root);
        assert_eq!(effort, "3pt");
    }
}

mod insertion_suggestion {
    use lotar::scanner::Scanner;

    #[test]
    fn suggest_insertion_after_signal_word() {
        let s = Scanner::new(std::path::PathBuf::from("."));
        let line = "// TODO: implement later";
        let edited = s.suggest_insertion_for_line(line, "DEMO-123").unwrap();
        assert!(edited.contains("TODO (DEMO-123):"), "{}", edited);
    }

    #[test]
    fn idempotent_no_duplicate_insertion() {
        let s = Scanner::new(std::path::PathBuf::from("."));
        let line = "// TODO (DEMO-123): implement later";
        assert!(s.suggest_insertion_for_line(line, "DEMO-123").is_none());
    }

    #[test]
    fn insert_before_colon_or_dash() {
        let s = Scanner::new(std::path::PathBuf::from("."));
        let line_colon = "// TODO Feature: thing";
        let edited_colon = s
            .suggest_insertion_for_line(line_colon, "DEMO-123")
            .unwrap();
        assert!(
            edited_colon.contains("TODO (DEMO-123) Feature:"),
            "{}",
            edited_colon
        );

        let line_dash = "# TODO Chore - tidy";
        let edited_dash = s.suggest_insertion_for_line(line_dash, "DEMO-123").unwrap();
        assert!(
            edited_dash.contains("TODO (DEMO-123) Chore -"),
            "{}",
            edited_dash
        );
    }
}

mod signal_words {
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;

    use lotar::scanner::Scanner;

    fn write(path: &std::path::Path, content: &str) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn detects_common_signal_words_by_default() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        write(
            &root.join("file.rs"),
            "
// TODO: one
// FIXME: two
// HACK: three
// BUG: four
// NOTE: five
",
        );

        let mut scanner = Scanner::new(std::path::PathBuf::from(root));
        let results = scanner.scan();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn custom_signal_words_can_be_provided() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        write(
            &root.join("file.rs"),
            "
// FOO: hidden
// BAR: visible
",
        );

        let mut scanner =
            Scanner::new(std::path::PathBuf::from(root)).with_signal_words(&["bar".into()]);
        let results = scanner.scan();
        assert_eq!(results.len(), 1);
    }
}

mod ticket_extraction {
    use super::{TestFixtures, cargo_bin_in};
    // use assert_cmd::Command; // not used after cargo_bin_in refactor

    #[test]
    fn extracts_ticket_from_bracket_attr() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        std::fs::write(
            root.join("a.js"),
            "// TODO [ticket=DEMO-123] implement thing",
        )
        .unwrap();

        let mut cmd = cargo_bin_in(&tf);
        let out = cmd
            .arg("--format")
            .arg("json")
            .arg("scan")
            .output()
            .unwrap();
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("DEMO-123"), "expected DEMO-123 in JSON: {s}");
    }

    #[test]
    fn extracts_ticket_from_bare_key() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        std::fs::write(root.join("b.rs"), "// TODO DEMO-999: implement more").unwrap();

        let mut cmd = cargo_bin_in(&tf);
        let out = cmd
            .arg("--format")
            .arg("json")
            .arg("scan")
            .output()
            .unwrap();
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("DEMO-999"), "expected DEMO-999 in JSON: {s}");
    }
}

mod words_toggle {
    use lotar::scanner::Scanner;
    use std::fs;
    use std::path::PathBuf;

    fn write_file(root: &std::path::Path, rel: &str, contents: &str) -> PathBuf {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, contents).unwrap();
        p
    }

    #[test]
    fn bare_ticket_key_is_not_detected_without_toggle() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_file(root, "src/main.rs", "// AUTH-123 implement login");

        let mut scanner = Scanner::new(PathBuf::from(root));
        scanner = scanner.with_ticket_detection(None, false);
        let refs = scanner.scan();
        assert_eq!(refs.len(), 0, "expected no detections without toggle");

        assert_eq!(
            scanner.extract_ticket_key_from_line("// AUTH-123 implement login"),
            Some("AUTH-123".to_string())
        );
    }

    #[test]
    fn issue_type_word_is_detected_as_signal() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_file(root, "src/lib.rs", "// Feature: implement signup");

        let mut scanner = Scanner::new(PathBuf::from(root))
            .with_signal_words(&["TODO".into(), "FIXME".into(), "Feature".into()])
            .with_ticket_detection(None, false);
        let refs = scanner.scan();
        assert_eq!(refs.len(), 1, "expected one detection with issue-type word");
        assert_eq!(refs[0].title.to_lowercase(), "implement signup");
    }
}

// Additional languages coverage consolidated from scanner_more_languages_test.rs
mod more_languages {
    use predicates::prelude::*;
    use std::fs;

    use super::{TestFixtures, cargo_bin_in};

    #[test]
    fn scan_html_block_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(
            root.join("index.html"),
            "<!DOCTYPE html>\n<!-- TODO: fix header layout -->\n<html><head></head><body></body></html>\n",
        )
        .unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_css_block_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(
            root.join("styles.css"),
            "/* TODO: replace color palette */\nbody { color: #333; }\n",
        )
        .unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_sql_single_line_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(
            root.join("schema.sql"),
            "-- TODO: add indexes\nCREATE TABLE t(id INT);\n",
        )
        .unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_ini_semicolon_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(
            root.join("config.ini"),
            "; TODO: verify defaults\nkey=value\n",
        )
        .unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_toml_hash_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(
            root.join("config.toml"),
            "# TODO: refine config\nname = \"app\"\n",
        )
        .unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_hcl_hash_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(root.join("infra.hcl"), "# TODO: pin provider versions\n").unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_tf_hash_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(root.join("main.tf"), "# TODO: split modules\n").unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_lua_double_dash_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(root.join("script.lua"), "-- TODO: optimize loop\n").unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_powershell_hash_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(root.join("script.ps1"), "# TODO: handle errors\n").unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_yaml_hash_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(root.join("config.yaml"), "# TODO: adjust vars\nkey: val\n").unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_tsx_line_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(
            root.join("App.tsx"),
            "// TODO: wire props\nexport default function App(){ return null }\n",
        )
        .unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn scan_markdown_html_comments() {
        let tf = TestFixtures::new();
        let root = tf.temp_dir.path();

        fs::write(
            root.join("README.md"),
            "# Doc\n<!-- TODO: refine docs -->\n",
        )
        .unwrap();

        cargo_bin_in(&tf)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }
}
