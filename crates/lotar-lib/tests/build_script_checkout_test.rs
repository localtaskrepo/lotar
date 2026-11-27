//! DEV-919: a checkout that builds into another checkout's target/ reuses
//! the build scripts' output (same package hash, same relative
//! rerun-if-changed paths), so LOTAR_DOCS_DIR kept pointing at the other,
//! deleted checkout and `include_str!` failed. `.cargo/config.toml` sets
//! LOTAR_CHECKOUT_DIR to the checkout's root and every build script that
//! emits a path reruns when it changes.

#[test]
fn cargo_config_sets_the_checkout_dir_relative_to_the_checkout() {
    let config = include_str!("../../../.cargo/config.toml");
    assert!(config.contains("LOTAR_CHECKOUT_DIR = { value = \"\", relative = true }"), "{config}");
}

#[test]
fn build_scripts_that_emit_paths_rerun_when_the_checkout_changes() {
    for (name, script) in [
        ("lotar-agent", include_str!("../../lotar-agent/build.rs")),
        ("lotar-lib", include_str!("../build.rs")),
        ("lotar-web-assets", include_str!("../../lotar-web-assets/build.rs")),
    ] {
        assert!(script.contains("cargo::rustc-env="), "{name}: no path to guard, drop it from this test");
        assert!(script.contains("println!(\"cargo::rerun-if-env-changed=LOTAR_CHECKOUT_DIR\");"), "{name}/build.rs must rerun when LOTAR_CHECKOUT_DIR changes");
    }
}

#[test]
fn the_built_library_points_at_this_checkout() {
    let checkout = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("checkout root");
    let repo_root = std::path::Path::new(env!("REPO_ROOT")).canonicalize().expect("REPO_ROOT exists");
    assert_eq!(repo_root, checkout);
    let docs = std::path::Path::new(env!("LOTAR_DOCS_DIR")).canonicalize().expect("LOTAR_DOCS_DIR exists");
    assert_eq!(docs, checkout.join("docs"));
}
