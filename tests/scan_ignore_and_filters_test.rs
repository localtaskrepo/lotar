use std::fs;
use std::io::Write;
use std::path::PathBuf;

use tempfile::tempdir;

use lotar::scanner::Scanner;

fn write(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut f = fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn filenames(results: &[lotar::scanner::Reference]) -> Vec<String> {
    let mut v: Vec<String> = results
        .iter()
        .map(|r| {
            r.file_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    v.sort();
    v
}

#[test]
fn include_filter_limits_extensions() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    write(&root.join("a.rs"), "// TODO: in rust");
    write(&root.join("b.py"), "# TODO: in python");

    let mut scanner = Scanner::new(PathBuf::from(root)).with_include_ext(&["rs".into()]);
    let results = scanner.scan();

    assert_eq!(results.len(), 1, "only .rs should be scanned");
    assert_eq!(filenames(&results), vec!["a.rs".to_string()]);
}

#[test]
fn exclude_overrides_include() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    write(&root.join("a.rs"), "// TODO: rust");
    write(&root.join("b.py"), "# TODO: py");

    let mut scanner = Scanner::new(PathBuf::from(root))
        .with_include_ext(&["rs".into(), "py".into()])
        .with_exclude_ext(&["py".into()]);
    let results = scanner.scan();

    assert_eq!(results.len(), 1, "py should be excluded");
    assert_eq!(filenames(&results), vec!["a.rs".to_string()]);
}

#[test]
fn gitignore_is_respected_when_no_lotarignore() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    write(&root.join(".gitignore"), "nested/\n*.log\n");
    write(
        &root.join("nested/ignored.js"),
        "// TODO: hidden by gitignore",
    );
    write(&root.join("visible.rs"), "// TODO: visible");

    let mut scanner = Scanner::new(PathBuf::from(root));
    let results = scanner.scan();

    let names = filenames(&results);
    assert!(names.contains(&"visible.rs".to_string()));
    assert!(
        !names.contains(&"ignored.js".to_string()),
        ".gitignore should hide nested/"
    );
}

#[test]
fn lotarignore_overrides_gitignore() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Gitignore hides nested, but .lotarignore exists and does not hide it
    write(&root.join(".gitignore"), "nested/\n");
    write(&root.join(".lotarignore"), "# custom rules (none)\n");
    write(
        &root.join("nested/scan.js"),
        "// TODO: should be scanned when .lotarignore present",
    );

    let mut scanner = Scanner::new(PathBuf::from(root));
    let results = scanner.scan();

    let names = filenames(&results);
    assert!(
        names.contains(&"scan.js".to_string()),
        ".lotarignore present => fallback to gitignore disabled"
    );
}

#[test]
fn lotarignore_can_exclude() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    write(&root.join(".lotarignore"), "nested/\n");
    write(
        &root.join("nested/skip.ts"),
        "// TODO: should be excluded by .lotarignore",
    );
    write(&root.join("keep.rs"), "// TODO: keep");

    let mut scanner = Scanner::new(PathBuf::from(root));
    let results = scanner.scan();

    let names = filenames(&results);
    assert!(names.contains(&"keep.rs".to_string()));
    assert!(!names.contains(&"skip.ts".to_string()));
}
