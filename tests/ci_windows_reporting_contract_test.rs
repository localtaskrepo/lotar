#[test]
fn attempted_windows_smoke_runs_still_require_a_report_but_skipped_runs_do_not() {
    let workflow: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(include_str!("../.github/workflows/ci.yml")).unwrap();
    let jobs = workflow["jobs"].as_mapping().unwrap();
    let steps = jobs
        .values()
        .filter_map(|job| job["steps"].as_sequence())
        .find(|steps| {
            steps.iter().any(|step| {
                step["name"].as_str()
                    == Some("Run portable agent + automation smoke selection (browser-free)")
            })
        })
        .expect("Windows portable smoke job");
    let run = steps
        .iter()
        .find(|step| {
            step["name"].as_str()
                == Some("Run portable agent + automation smoke selection (browser-free)")
        })
        .unwrap();
    let upload = steps
        .iter()
        .find(|step| step["name"].as_str() == Some("Upload Windows portable smoke report"))
        .unwrap();
    let id = run["id"]
        .as_str()
        .expect("report guard requires a named test step");
    let condition = upload["if"].as_str().unwrap();
    assert!(
        run["run"]
            .as_str()
            .unwrap()
            .contains("smoke/tests/harness.agent-launcher.smoke.spec.ts"),
        "portable launcher regressions must also execute on Windows"
    );
    assert!(
        condition.contains("always()"),
        "reports must survive a failed test run"
    );
    assert!(condition.contains(&format!("steps.{id}.outcome != 'skipped'")));
    assert!(condition.contains(&format!("steps.{id}.outcome != ''")));
    assert_eq!(upload["with"]["if-no-files-found"].as_str(), Some("error"));
    assert!(
        run["continue-on-error"].is_null(),
        "smoke failures remain failures"
    );
    assert!(
        run["run"]
            .as_str()
            .unwrap()
            .contains("if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }")
    );
}
