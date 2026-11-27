use std::env;
use std::path::PathBuf;

// The default agent instructions come from docs/help/agent-instructions.md.
// Inside the workspace that is the repository's docs/; a packaged crate
// carries a copy in its own docs/, which scripts/stage-crates.mjs makes.
fn main() {
    // Rerun when another checkout built into this target/ (.cargo/config.toml).
    println!("cargo::rerun-if-env-changed=LOTAR_CHECKOUT_DIR");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
    let docs_dir = if manifest_dir.ends_with("crates/lotar-agent") {
        println!("cargo:rerun-if-changed=../../docs/help/agent-instructions.md");
        manifest_dir.join("../../docs")
    } else {
        println!("cargo:rerun-if-changed=docs/help/agent-instructions.md");
        manifest_dir.join("docs")
    };
    assert!(
        docs_dir.join("help/agent-instructions.md").is_file(),
        "{} has no help/agent-instructions.md: run `node scripts/stage-crates.mjs` before `cargo package`",
        docs_dir.display()
    );
    println!("cargo::rustc-env=LOTAR_DOCS_DIR={}", docs_dir.display());
}
