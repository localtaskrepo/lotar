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
    let rust_run = steps
        .iter()
        .find(|step| {
            step["run"]
                .as_str()
                .is_some_and(|run| run.contains("cargo nextest run"))
        })
        .expect("Windows native Rust regressions");
    let rust_list = steps
        .iter()
        .find(|step| {
            step["run"]
                .as_str()
                .is_some_and(|run| run.contains("cargo nextest list"))
        })
        .expect("Windows selected-test manifest");
    let compile_check = steps
        .iter()
        .find(|step| step["name"].as_str() == Some("Compile-check all Windows targets"))
        .expect("narrowed linking must preserve compilation of every Windows target");
    let compile_command = compile_check["run"].as_str().unwrap();
    assert!(compile_command.contains("cargo check --profile ci --all-targets --all-features"));
    assert!(compile_command.contains("if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }"));
    assert!(compile_check["continue-on-error"].is_null());
    assert!(
        steps
            .iter()
            .position(|step| std::ptr::eq(step, compile_check))
            .unwrap()
            < steps
                .iter()
                .position(|step| std::ptr::eq(step, rust_run))
                .unwrap()
    );
    for step in [rust_run, rust_list] {
        let command = step["run"].as_str().unwrap();
        assert!(command.contains("--lib --bin lotar-agent-wrapper"));
        let flags: Vec<_> = command.split_whitespace().collect();
        assert!(!flags.contains(&"--tests"));
        assert!(!flags.contains(&"--bins"));
    }
    for selector in [
        "test(storage::safety::)",
        "test(utils::scan::)",
        "binary(lotar-agent-wrapper)",
        "test(services::automation_service::windows_async_stdio_tests::)",
        "test(services::automation_service::async_run_monitor_tests::)",
    ] {
        for step in [rust_run, rust_list] {
            assert!(
                step["run"].as_str().unwrap().contains(selector),
                "Windows run and coverage manifest must include {selector}"
            );
        }
    }
    assert!(rust_run["run"].as_str().unwrap().contains("--retries 0"));
    assert!(
        rust_run["run"]
            .as_str()
            .unwrap()
            .contains("--failure-output=immediate-final")
    );
    let upload = steps
        .iter()
        .find(|step| step["name"].as_str() == Some("Upload Windows portable smoke report"))
        .unwrap();
    let id = run["id"]
        .as_str()
        .expect("report guard requires a named test step");
    let condition = upload["if"].as_str().unwrap();
    for spec in [
        "fixtures.agent",
        "harness.agent-launcher",
        "api.agent-jobs",
        "cli.automation",
        "api.automation",
    ] {
        assert!(
            run["run"]
                .as_str()
                .unwrap()
                .contains(&format!("smoke/tests/{spec}.smoke.spec.ts")),
            "portable process regressions must also execute on Windows: {spec}"
        );
    }
    assert_eq!(run["env"]["LOTAR_SMOKE_REQUIRE_GIT"].as_str(), Some("1"));
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

#[test]
fn ci_uses_the_pinned_toolchain_and_keeps_profile_caches_independent() {
    let workflow: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(include_str!("../.github/workflows/ci.yml")).unwrap();
    let channel = include_str!("../rust-toolchain.toml")
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(key, value)| (key.trim() == "channel").then_some(value.trim()))
        .unwrap()
        .trim_matches('"');
    assert_eq!(workflow["env"]["RUST_TOOLCHAIN"].as_str(), Some(channel));
    for job in workflow["jobs"].as_mapping().unwrap().values() {
        for step in job["steps"].as_sequence().unwrap() {
            if step["uses"].as_str() == Some("dtolnay/rust-toolchain@stable") {
                assert_eq!(
                    step["with"]["toolchain"].as_str(),
                    Some("${{ env.RUST_TOOLCHAIN }}")
                );
            }
            if step["uses"].as_str() == Some("Swatinem/rust-cache@v2") {
                assert!(
                    step["with"]["shared-key"]
                        .as_str()
                        .unwrap()
                        .contains("github.job")
                );
                assert_eq!(step["with"]["cache-on-failure"].as_bool(), Some(true));
            }
            if step["name"].as_str() == Some("Cache sccache") {
                let key = step["with"]["key"].as_str().unwrap();
                assert!(key.contains("github.job"));
                assert!(key.contains("github.run_id"));
                assert!(
                    step["with"]["restore-keys"]
                        .as_str()
                        .unwrap()
                        .contains("github.job")
                );
            }
        }
    }
    let linux = workflow["jobs"]["rust_checks"]["steps"]
        .as_sequence()
        .unwrap();
    let full = linux
        .iter()
        .find(|step| {
            step["run"]
                .as_str()
                .is_some_and(|run| run.contains("cargo nextest run"))
        })
        .unwrap();
    assert!(
        full["run"]
            .as_str()
            .unwrap()
            .contains("--lib --bins --tests")
    );
}

#[test]
fn smoke_job_caches_the_headless_browser_per_playwright_version() {
    let workflow: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(include_str!("../.github/workflows/ci.yml")).unwrap();
    let steps = workflow["jobs"]["smoke_tests"]["steps"]
        .as_sequence()
        .unwrap();
    let step = |name: &str| {
        steps
            .iter()
            .find(|step| step["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("smoke_tests is missing step {name:?}"))
    };
    let cache = step("Cache Playwright browsers");
    assert_eq!(
        cache["with"]["path"].as_str(),
        Some("~/.cache/ms-playwright")
    );
    assert!(
        cache["with"]["key"]
            .as_str()
            .unwrap()
            .contains("steps.playwright_version.outputs.version")
    );
    let install = step("Install Playwright browser");
    assert_eq!(
        install["if"].as_str(),
        Some("steps.playwright_cache.outputs.cache-hit != 'true'")
    );
    assert!(install["run"].as_str().unwrap().contains("--only-shell"));
    assert!(
        step("Install Playwright system dependencies")["run"]
            .as_str()
            .unwrap()
            .contains("install-deps")
    );
}
