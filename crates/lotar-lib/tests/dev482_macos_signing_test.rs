//! DEV-482: the macOS release binaries ship Developer ID-signed and notarized
//! under a fixed identifier, and the job that signs them runs no build code.

fn workflow(source: &str) -> serde_yaml_ng::Value {
    serde_yaml_ng::from_str(source).unwrap()
}

fn runs(job: &serde_yaml_ng::Value) -> Vec<&str> {
    job["steps"].as_sequence().unwrap().iter().filter_map(|step| step["run"].as_str()).collect()
}

#[test]
fn macos_archives_reach_the_release_only_through_sign_macos() {
    let release = workflow(include_str!("../../../.github/workflows/release.yml"));
    let upload = release["jobs"]["build"]["steps"].as_sequence().unwrap().iter().find(|step| step["name"].as_str() == Some("Upload build output")).unwrap();
    assert_eq!(upload["with"]["name"].as_str(), Some("${{ matrix.target == 'macos' && 'unsigned' || 'release' }}-${{ matrix.artifact }}"));

    let sign = &release["jobs"]["sign-macos"];
    assert_eq!(sign["needs"].as_str(), Some("build"));
    assert_eq!(sign["permissions"]["contents"].as_str(), Some("read"));
    assert!(sign["permissions"]["id-token"].is_null());
    let artifacts: Vec<_> = sign["strategy"]["matrix"]["artifact"].as_sequence().unwrap().iter().filter_map(|a| a.as_str()).collect();
    assert_eq!(artifacts, ["macos-arm64", "macos-x64"]);
    for step in sign["steps"].as_sequence().unwrap() {
        let uses = step["uses"].as_str().unwrap_or_default();
        assert!(!uses.starts_with("actions/checkout@"), "sign-macos checks out code");
        let run = step["run"].as_str().unwrap_or_default();
        assert!(!run.contains("npm") && !run.contains("cargo"), "sign-macos runs build code: {run}");
    }
    let uploads: Vec<_> = sign["steps"].as_sequence().unwrap().iter().filter_map(|step| step["with"]["name"].as_str()).collect();
    assert_eq!(uploads, ["unsigned-${{ matrix.artifact }}", "release-${{ matrix.artifact }}"]);
}

#[test]
fn sign_macos_signs_with_the_fixed_identity_and_fails_without_secrets() {
    let release = workflow(include_str!("../../../.github/workflows/release.yml"));
    let sign = &release["jobs"]["sign-macos"];
    // macOS ties firewall and TCC grants to these: they never change.
    assert_eq!(sign["env"]["BUNDLE_ID"].as_str(), Some("net.pyxzl.lotar"));
    assert_eq!(sign["env"]["TEAM_ID"].as_str(), Some("3HKD37M482"));
    assert_eq!(sign["env"]["SIGN_IDENTITY"].as_str(), Some("Developer ID Application: Ravi Gairola (3HKD37M482)"));

    let scripts = runs(sign).join("\n");
    for expected in [
        "codesign --force --options runtime --timestamp --identifier \"$BUNDLE_ID\"",
        "codesign --verify --strict",
        "grep -qx \"TeamIdentifier=$TEAM_ID\"",
        "xcrun notarytool submit",
        "--wait",
        // --type execute rejects a bare Mach-O as "does not seem to be an app".
        "spctl --assess --type install -vv dist/lotar",
        "source=Notarized Developer ID",
        "shasum -a 256 \"$NAME.tar.gz\" > \"$NAME.sha256\"",
    ] {
        assert!(scripts.contains(expected), "sign-macos lacks {expected}");
    }

    let steps = sign["steps"].as_sequence().unwrap();
    let check = &steps[0];
    assert_eq!(check["name"].as_str(), Some("Check signing secrets"));
    for secret in
        ["MACOS_DEVELOPER_ID_P12_BASE64", "MACOS_DEVELOPER_ID_P12_PASSWORD", "APPLE_NOTARY_KEY_P8_BASE64", "APPLE_NOTARY_KEY_ID", "APPLE_NOTARY_ISSUER_ID"]
    {
        let referenced = check["env"].as_mapping().unwrap().values().any(|v| v.as_str() == Some(&format!("${{{{ secrets.{secret} }}}}")));
        assert!(referenced, "secret check misses {secret}");
    }
    assert!(check["run"].as_str().unwrap().contains("exit 1"));

    let cleanup = steps.iter().find(|step| step["run"].as_str().is_some_and(|run| run.contains("security delete-keychain"))).unwrap();
    assert_eq!(cleanup["if"].as_str(), Some("always()"));
}

#[test]
fn homebrew_verify_checks_the_developer_id_signature() {
    let verify = workflow(include_str!("../../../.github/workflows/homebrew-verify.yml"));
    let scripts = runs(&verify["jobs"]["homebrew-install-test"]).join("\n");
    assert!(scripts.contains("Identifier=net.pyxzl.lotar"), "{scripts}");
    assert!(scripts.contains("TeamIdentifier=3HKD37M482"), "{scripts}");
}
