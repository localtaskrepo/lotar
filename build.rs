use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set");
    let manifest_dir_path = PathBuf::from(manifest_dir);
    let web_dir = manifest_dir_path.join("target").join("web");

    // `include_dir!` in `src/web_server.rs` requires this directory to exist at compile-time.
    // Create it proactively so `cargo build` works even before running `npm run build:web`.
    let _ = fs::create_dir_all(&web_dir);

    // The embedded web UI is sourced from `target/web`. Cargo otherwise has no idea that a
    // `vite build` changed those files, so we explicitly mark them as inputs to the build.
    println!("cargo:rerun-if-changed=target/web");

    if let Ok(entries) = walk_files(&web_dir) {
        for entry in entries {
            let path = entry
                .strip_prefix(&manifest_dir_path)
                .unwrap_or(entry.as_path());
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn walk_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    if !root.exists() {
        return Ok(files);
    }

    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                files.push(path);
            }
        }
    }

    Ok(files)
}
