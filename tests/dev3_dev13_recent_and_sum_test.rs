//! DEV-3: `recent:` takes any days or weeks, measured from `modified`, and
//! `-recent:<N>d` lists tasks not changed for that long (also `stats stale`).
//! DEV-13: `lotar list --sum <field>` adds totals over every matching task.

use chrono::{Duration, Utc};
use serde_json::Value;

mod common;

struct Workspace {
    _tmp: tempfile::TempDir,
    tasks_dir: std::path::PathBuf,
}

fn workspace() -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks_dir.join("RS")).unwrap();
    std::fs::write(
        tasks_dir.join("config.yml"),
        "default:\n  project: RS\nissue:\n  custom_fields: [estimate]\n",
    )
    .unwrap();
    // (number, days since modified, extra YAML)
    for (num, age, extra) in [
        (1, 2, "effort: 4h\ncustom_fields:\n  estimate: '2.5'\n"),
        (2, 10, "effort: 1d\ncustom_fields:\n  estimate: '3'\n"),
        (3, 40, "effort: 3pt\ncustom_fields:\n  estimate: lots\n"),
        (4, 60, "status: Done\n"),
    ] {
        let modified = (Utc::now() - Duration::days(age)).to_rfc3339();
        let status = if extra.contains("status:") {
            ""
        } else {
            "status: Todo\n"
        };
        std::fs::write(
            tasks_dir.join("RS").join(format!("{num}.yml")),
            format!(
                "title: Task {num}\n{status}priority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\nmodified: {modified}\n{extra}"
            ),
        )
        .unwrap();
    }
    Workspace {
        _tmp: tmp,
        tasks_dir,
    }
}

fn cli(ws: &Workspace, args: &[&str]) -> Result<Value, String> {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_TASKS_DIR", &ws.tasks_dir);
    cmd.current_dir(ws.tasks_dir.parent().unwrap());
    cmd.args(["--format", "json"]).args(args);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

fn ids(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn recent_takes_any_window_and_its_negation_lists_older_tasks() {
    let ws = workspace();
    let list = |args: &[&str]| {
        let mut all = vec!["list"];
        all.extend_from_slice(args);
        ids(&cli(&ws, &all).unwrap()["tasks"])
    };
    assert_eq!(list(&["recent:7d"]), ["RS-1"]);
    assert_eq!(list(&["recent:2w"]), ["RS-1", "RS-2"]);
    assert_eq!(list(&["--recent", "30d"]), ["RS-1", "RS-2"]);
    // Not changed for 30 days; with -status:done, open work only.
    assert_eq!(list(&["-recent:30d"]), ["RS-3", "RS-4"]);
    assert_eq!(list(&["-recent:30d", "-status:done"]), ["RS-3"]);
    for bad in ["recent:0d", "recent:7", "recent:7h"] {
        let err = cli(&ws, &["list", bad]).unwrap_err();
        assert!(err.contains("Invalid recent"), "{bad}: {err}");
    }

    // stats stale uses the same rule, from `modified` (no git needed).
    let stale = cli(&ws, &["stats", "stale", "--threshold", "30d", "--global"]).unwrap();
    assert_eq!(ids(&stale["items"]), ["RS-4", "RS-3"]);
    let stale_open = cli(
        &ws,
        &[
            "stats",
            "stale",
            "--threshold",
            "30d",
            "--global",
            "--filter",
            "-status:done",
        ],
    )
    .unwrap();
    assert_eq!(ids(&stale_open["items"]), ["RS-3"]);
}

#[test]
fn sum_totals_every_matching_task() {
    let ws = workspace();
    // One task per page, but the sum covers all four matching tasks.
    let out = cli(
        &ws,
        &[
            "list",
            "--page-size",
            "1",
            "--sum",
            "effort",
            "--sum",
            "estimate",
        ],
    )
    .unwrap();
    assert_eq!(out["tasks"].as_array().unwrap().len(), 1);
    let sums = out["sums"].as_array().unwrap();
    assert_eq!(sums[0]["field"], "effort");
    assert_eq!(sums[0]["hours"], 12.0);
    assert_eq!(sums[0]["points"], 3.0);
    assert_eq!(sums[0]["counted"], 3);
    assert_eq!(sums[0]["missing"], 1);
    assert_eq!(sums[1]["field"], "field:estimate");
    assert_eq!(sums[1]["total"], 5.5);
    assert_eq!(sums[1]["counted"], 2);
    assert_eq!(sums[1]["invalid"], 1);
    assert_eq!(sums[1]["missing"], 1);

    // Sums follow the filter.
    let open = cli(
        &ws,
        &["list", "-status:done", "recent:2w", "--sum", "effort"],
    )
    .unwrap();
    assert_eq!(open["sums"][0]["hours"], 12.0);
    assert_eq!(open["sums"][0]["counted"], 2);

    // Text output prints one line per sum.
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_TASKS_DIR", &ws.tasks_dir);
    cmd.current_dir(ws.tasks_dir.parent().unwrap());
    let text = cmd.args(["list", "--sum", "effort"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(
        stdout.contains("Sum of effort: 12h + 3pt over 3 task(s) (1 without a value)"),
        "{stdout}"
    );

    // Fields that are not numbers cannot be summed.
    let err = cli(&ws, &["list", "--sum", "title"]).unwrap_err();
    assert!(err.contains("Cannot sum 'title'"), "{err}");
}
