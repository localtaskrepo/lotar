use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
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
}
