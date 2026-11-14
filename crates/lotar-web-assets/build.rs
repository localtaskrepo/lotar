use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));

    // A packaged crate (cargo package, crates.io) sits in a folder named
    // lotar-web-assets-<version> with no repository around it, so it carries
    // the UI in its own web-embed/, which scripts/stage-crates.mjs copies from
    // target/web-embed (DEV-507). Without it the crate would embed no UI.
    if !manifest_dir.ends_with("crates/lotar-web-assets") {
        let embed_dir = manifest_dir.join("web-embed");
        assert!(
            embed_dir.join("index.html.gz").is_file(),
            "{} has no index.html.gz: run `npm run build:web` and `node scripts/stage-crates.mjs` before `cargo package`",
            embed_dir.display()
        );
        println!("cargo:rerun-if-changed=web-embed");
        println!("cargo::rustc-env=LOTAR_WEB_EMBED_DIR={}", embed_dir.display());
        return;
    }

    let target_dir = manifest_dir.join("..").join("..").join("target");

    // `include_dir!` in src/lib.rs reads `target/web-embed` at compile time,
    // and the library's dev-mode filesystem fallback reads `target/web`.
    // Create both proactively so `cargo build` works even before running
    // `npm run build:web` (vite build + scripts/compress-web.mjs).
    let _ = fs::create_dir_all(target_dir.join("web"));
    let _ = fs::create_dir_all(target_dir.join("web-embed"));

    // scripts/compress-web.mjs generates `target/web-embed` from `target/web`
    // (gz for text assets, byte-exact raw copies for everything else). Cargo
    // has no idea that an `npm run build:web` changed those files, so mark
    // the embed tree, which is what `include_dir!` reads, as an input; cargo
    // scans a watched directory recursively.
    println!("cargo:rerun-if-changed=../../target/web-embed");
    println!("cargo::rustc-env=LOTAR_WEB_EMBED_DIR={}", target_dir.join("web-embed").display());
}
