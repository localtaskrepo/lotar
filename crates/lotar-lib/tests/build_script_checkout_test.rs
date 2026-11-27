//! DEV-919, DEV-927: a checkout that builds into another checkout's target/
//! reuses the build scripts' output (same package hash, same relative
//! rerun-if-changed paths), so LOTAR_DOCS_DIR kept pointing at the other,
//! maybe deleted checkout and `include_str!` failed. Every build script that
//! emits a path also watches its own build.rs by absolute path, which differs
//! per checkout, so cargo reruns it whatever directory it runs from.

use std::path::Path;
use std::process::Command;

const RERUN_PER_CHECKOUT: &str = "println!(\"cargo::rerun-if-changed={}\", manifest_dir.join(\"build.rs\").display());";

#[test]
fn build_scripts_that_emit_paths_rerun_per_checkout() {
    for (name, script) in [
        ("lotar-agent", include_str!("../../lotar-agent/build.rs")),
        ("lotar-lib", include_str!("../build.rs")),
        ("lotar-web-assets", include_str!("../../lotar-web-assets/build.rs")),
    ] {
        assert!(script.contains("cargo::rustc-env="), "{name}: no path to guard, drop it from this test");
        assert!(script.contains(RERUN_PER_CHECKOUT), "{name}/build.rs must watch its build.rs by absolute path");
    }
}

#[test]
fn the_built_library_points_at_this_checkout() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("checkout root");
    let repo_root = Path::new(env!("REPO_ROOT")).canonicalize().expect("REPO_ROOT exists");
    assert_eq!(repo_root, checkout);
    let docs = Path::new(env!("LOTAR_DOCS_DIR")).canonicalize().expect("LOTAR_DOCS_DIR exists");
    assert_eq!(docs, checkout.join("docs"));
}

/// The ticket's check on two copies of a probe crate with the same build
/// script pattern: building b with `--manifest-path` from a, then a, must
/// print each copy's own path.
#[test]
fn a_build_from_another_checkout_does_not_keep_its_path() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let root = workspace.path().canonicalize().expect("canonical tempdir");
    for copy in ["a", "b"] {
        let dir = root.join(copy);
        std::fs::create_dir_all(dir.join("src")).expect("create probe crate");
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n")
            .expect("write Cargo.toml");
        let build = format!(
            "fn main() {{\n    let manifest_dir = std::path::PathBuf::from(std::env::var(\"CARGO_MANIFEST_DIR\").unwrap());\n    {RERUN_PER_CHECKOUT}\n    println!(\"cargo::rustc-env=PROBE_DIR={{}}\", manifest_dir.display());\n}}\n"
        );
        std::fs::write(dir.join("build.rs"), build).expect("write build.rs");
        std::fs::write(dir.join("src/main.rs"), "fn main() { print!(\"{}\", env!(\"PROBE_DIR\")); }\n").expect("write main.rs");
    }
    let run = |args: &[&str]| -> String {
        let output = Command::new(env!("CARGO"))
            .current_dir(root.join("a"))
            .args(["run", "--quiet", "--offline", "--target-dir"])
            .arg(root.join("target"))
            .args(args)
            .env("RUSTC_WRAPPER", "")
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("CARGO_BUILD_TARGET_DIR")
            .output()
            .expect("run cargo");
        assert!(output.status.success(), "cargo run {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).expect("utf-8 path")
    };
    let b_manifest = root.join("b/Cargo.toml");
    assert_eq!(run(&["--manifest-path", b_manifest.to_str().expect("utf-8 path")]), root.join("b").display().to_string());
    assert_eq!(run(&[]), root.join("a").display().to_string());
}
