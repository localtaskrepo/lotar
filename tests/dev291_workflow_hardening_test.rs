//! DEV-291: release build code runs without the write token or signing
//! identity, checks-only workflows get a read-only token, and no workflow
//! expands `${{ }}` expressions inside a run script.

fn workflow(source: &str) -> serde_yaml_ng::Value {
    serde_yaml_ng::from_str(source).unwrap()
}

#[test]
fn release_build_job_has_no_write_token_or_signing_identity() {
    let release = workflow(include_str!("../.github/workflows/release.yml"));
    let build = &release["jobs"]["build"];
    assert_eq!(build["permissions"]["contents"].as_str(), Some("read"));
    assert!(build["permissions"]["id-token"].is_null());
    let steps = build["steps"].as_sequence().unwrap();
    let checkout = steps.iter().find(|step| step["uses"].as_str().is_some_and(|u| u.starts_with("actions/checkout@"))).unwrap();
    assert_eq!(checkout["with"]["persist-credentials"].as_bool(), Some(false));
    for step in steps {
        let uses = step["uses"].as_str().unwrap_or_default();
        assert!(!uses.starts_with("sigstore/"), "build job signs: {uses}");
        assert!(!uses.starts_with("softprops/"), "build job uploads: {uses}");
    }
    let install_cross = steps.iter().filter_map(|step| step["run"].as_str()).find(|run| run.contains("cargo install cross")).unwrap();
    assert!(install_cross.contains("--rev "), "{install_cross}");

    let sign = &release["jobs"]["build-and-release"];
    // DEV-482: the macOS archives come from sign-macos.
    let needs: Vec<_> = sign["needs"].as_sequence().unwrap().iter().filter_map(|n| n.as_str()).collect();
    assert_eq!(needs, ["build", "sign-macos"]);
    assert_eq!(sign["permissions"]["id-token"].as_str(), Some("write"));
    for step in sign["steps"].as_sequence().unwrap() {
        let run = step["run"].as_str().unwrap_or_default();
        assert!(!run.contains("npm") && !run.contains("cargo"), "signing job runs build code: {run}");
        assert!(step["uses"].as_str() != Some("actions/checkout"));
    }

    let cross = include_str!("../Cross.toml");
    assert!(cross.contains("aarch64-unknown-linux-musl@sha256:"), "{cross}");
    // DEV-309: the armv7 image is pinned the same way.
    assert!(cross.contains("armv7-unknown-linux-musleabihf@sha256:"), "{cross}");
}

#[test]
fn checks_only_workflows_have_a_read_only_token() {
    for (name, source) in [
        ("ci.yml", include_str!("../.github/workflows/ci.yml")),
        ("homebrew-verify.yml", include_str!("../.github/workflows/homebrew-verify.yml")),
        ("scoop-verify.yml", include_str!("../.github/workflows/scoop-verify.yml")),
    ] {
        let parsed = workflow(source);
        assert_eq!(parsed["permissions"]["contents"].as_str(), Some("read"), "{name}");
    }
}

#[test]
fn run_scripts_take_expressions_through_env() {
    for (name, source) in [
        ("ci.yml", include_str!("../.github/workflows/ci.yml")),
        ("codeql.yml", include_str!("../.github/workflows/codeql.yml")),
        ("coverage.yml", include_str!("../.github/workflows/coverage.yml")),
        ("homebrew-verify.yml", include_str!("../.github/workflows/homebrew-verify.yml")),
        ("install-verify.yml", include_str!("../.github/workflows/install-verify.yml")),
        ("pages.yml", include_str!("../.github/workflows/pages.yml")),
        ("performance.yml", include_str!("../.github/workflows/performance.yml")),
        ("release.yml", include_str!("../.github/workflows/release.yml")),
        ("scoop-verify.yml", include_str!("../.github/workflows/scoop-verify.yml")),
    ] {
        let parsed = workflow(source);
        for (job, body) in parsed["jobs"].as_mapping().unwrap() {
            for step in body["steps"].as_sequence().into_iter().flatten() {
                let run = step["run"].as_str().unwrap_or_default();
                assert!(!run.contains("${{"), "{name} job {job:?} step {:?} expands an expression in its script", step["name"]);
            }
        }
    }
}
