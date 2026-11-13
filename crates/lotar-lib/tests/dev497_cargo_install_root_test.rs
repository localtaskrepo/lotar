//! DEV-497: `cargo install --path .` (and `cargo install --git <repo> lotar`)
//! installs the root package's binaries, so `lotar` and `lotar-agent-wrapper`
//! (which `lotar agent list-running` needs) stay in the root package.

#[test]
fn the_root_package_installs_lotar_and_the_agent_wrapper() {
    let root = std::path::Path::new(env!("REPO_ROOT"));
    let output = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--offline", "--format-version", "1", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .unwrap();
    assert!(output.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr));
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let root_manifest = root.join("Cargo.toml").canonicalize().unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| {
            package["manifest_path"].as_str().and_then(|path| std::path::Path::new(path).canonicalize().ok()).as_deref() == Some(root_manifest.as_path())
        })
        .expect("the root Cargo.toml is a package, not only a workspace");
    assert_eq!(package["name"], "lotar");
    let mut bins: Vec<&str> = package["targets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|target| target["kind"].as_array().is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin")))
        .filter_map(|target| target["name"].as_str())
        .collect();
    bins.sort_unstable();
    assert_eq!(bins, ["lotar", "lotar-agent-wrapper"], "cargo install --path . installs the root package's binaries");
}
