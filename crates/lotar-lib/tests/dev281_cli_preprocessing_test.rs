//! DEV-281: help/version words and dash-leading values are data, not
//! commands or global flags.

use crate::common;
use common::TestFixtures;

fn run(fixtures: &TestFixtures, args: &[&str]) -> std::process::Output {
    common::lotar_cmd().unwrap().current_dir(fixtures.get_temp_path()).args(args).output().unwrap()
}

fn text(out: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn help_and_version_words_are_values() {
    let fixtures = TestFixtures::new();
    for args in [&["add", "version", "-p", "ALPH"][..], &["add", "Bump", "--tag", "version"], &["add", "help"]] {
        let out = run(&fixtures, args);
        assert!(text(&out).contains("Created task"), "{args:?}: {}", text(&out));
    }
    let out = run(&fixtures, &["comment", "ALPH-1", "-m", "help"]);
    assert!(text(&out).contains("Comment added"), "{}", text(&out));
    let out = run(&fixtures, &["edit", "ALPH-1", "--description", "help"]);
    assert!(out.status.success(), "{}", text(&out));
    let shown = text(&run(&fixtures, &["show", "ALPH-1"]));
    assert!(shown.contains("help"), "{shown}");

    // Real requests still work, also after global options.
    assert!(text(&run(&fixtures, &["--version"])).contains("lotar "));
    assert!(text(&run(&fixtures, &["-p", "ALPH", "list", "--help"])).contains("lotar list"));
    assert!(text(&run(&fixtures, &["ls", "--help"])).contains("lotar list"));
    assert!(text(&run(&fixtures, &["help", "add"])).contains("lotar add"));
    let stats = run(&fixtures, &["stats"]);
    assert!(stats.status.success() && text(&stats).contains("lotar stats"));
}

#[test]
fn dash_leading_values_are_not_global_flags() {
    let fixtures = TestFixtures::new();
    assert!(run(&fixtures, &["add", "one", "-p", "ALPH"]).status.success());
    for args in [
        &["comment", "ALPH-1", "-m", "-lower the timeout"][..],
        &["comment", "ALPH-1", "-m", "-fix: flaky test"],
        &["add", "Title", "--description", "-f flag broken"],
        &["edit", "ALPH-1", "--title", "-fix title"],
    ] {
        let out = run(&fixtures, args);
        assert!(out.status.success(), "{args:?}: {}", text(&out));
    }
    let out = run(&fixtures, &["-fjson", "list"]);
    assert!(String::from_utf8_lossy(&out.stdout).trim_start().starts_with('{'));
}
