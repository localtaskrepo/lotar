//! DEV-562: `add` names the open tickets with a similar title, and files anyway.
use crate::common::{TestFixtures, cargo_bin_in};

const CONFIG: &str = "default:\n  project: LOCA\nissue:\n  states: [Todo, InProgress, Done]\n  done_states: [Done]\n";

fn add(fixtures: &TestFixtures, args: &[&str]) -> (String, String) {
    let output = cargo_bin_in(fixtures).args(["--tasks-dir", fixtures.tasks_root.to_str().unwrap()]).args(args).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    (String::from_utf8_lossy(&output.stdout).into_owned(), String::from_utf8_lossy(&output.stderr).into_owned())
}

#[test]
fn add_warns_about_open_tickets_with_a_similar_title() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    let (_, stderr) = add(&fixtures, &["add", "opgate restart while the Mac is locked drops the signing key", "--finding", "opgate:locked-keychain-restart"]);
    assert!(!stderr.contains("similar title"), "{stderr}");
    add(&fixtures, &["add", "Unrelated: CI is red on main"]);

    // A plain add with mostly the same words: filed, with a warning naming LOCA-1.
    let (stdout, stderr) = add(&fixtures, &["add", "opgate restart on a locked Mac drops the signing key"]);
    assert!(stdout.contains("LOCA-3"), "{stdout}");
    assert!(stderr.contains("similar title: LOCA-1 \"opgate restart while the Mac is locked drops the signing key\""), "{stderr}");
    assert!(!stderr.contains("LOCA-2"), "{stderr}");

    // A finding with a new subject and a near-identical title: the same.
    let (stdout, stderr) =
        add(&fixtures, &["--format", "json", "add", "opgate restart while the Mac is locked drops signing key", "--finding", "opgate:locked-restart"]);
    assert!(stdout.contains("\"LOCA-4\""), "{stdout}");
    assert!(stderr.contains("LOCA-1") && stderr.contains("LOCA-3"), "{stderr}");

    // Done tickets don't count.
    for id in ["LOCA-1", "LOCA-3", "LOCA-4"] {
        add(&fixtures, &["status", id, "Done"]);
    }
    let (_, stderr) = add(&fixtures, &["add", "opgate restart while the Mac is locked drops the signing key"]);
    assert!(!stderr.contains("similar title"), "{stderr}");
}

#[test]
fn a_finding_names_only_open_tickets_of_the_same_check() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    add(&fixtures, &["add", "launch agent last exit was -15", "--finding", "live:launch-agent-exit"]);
    add(&fixtures, &["add", "launch agent last exit was -15", "--finding", "health:launch-agent-exit"]);

    // Same check, new subject: LOCA-1 is named; LOCA-2 is another check's problem.
    let (_, stderr) = add(&fixtures, &["add", "launch agent whose last exit was -15", "--finding", "live:launch-agents"]);
    assert!(stderr.contains("LOCA-1") && !stderr.contains("LOCA-2"), "{stderr}");

    // A plain add has no check: both are named.
    let (_, stderr) = add(&fixtures, &["add", "launch agent whose last exit was -15"]);
    assert!(stderr.contains("LOCA-1") && stderr.contains("LOCA-2") && stderr.contains("LOCA-3"), "{stderr}");
}
