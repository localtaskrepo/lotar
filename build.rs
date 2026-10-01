use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    // Declare the custom `no_git_tests` cfg so `check-cfg` doesn't flag it.
    println!("cargo::rustc-check-cfg=cfg(no_git_tests)");
    println!("cargo::rustc-check-cfg=cfg(lotar_require_git)");
    // Printed before any conditional return so a warm target directory still
    // re-evaluates the probe when only this environment variable flips.
    println!("cargo::rerun-if-env-changed=LOTAR_REQUIRE_GIT");

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

    // Some sandboxed runtimes (e.g. certain agent harnesses) forbid creating
    // anything named `.git`, which makes every integration test that runs
    // `git init` fail with "Operation not permitted". Probe for that here so
    // LOTAR_REQUIRE_GIT=1 can refuse to build on a designated runner that
    // lost the capability. DEV-79: the historical `no_git_tests` cfg is no
    // longer emitted - Git-dependent tests always compile and runtime
    // selection (scripts/rust-test-runner.mjs + the gitless nextest profile)
    // together with the fail-closed require_git() probe own exclusion.
    //
    // Probe the system temp dir (where tempfile-based tests actually create
    // their repos) in addition to OUT_DIR: some sandboxes allow `.git` inside
    // the workspace but deny it elsewhere, which OUT_DIR alone would miss.
    let mut git_denied = false;
    if let Ok(out_dir) = env::var("OUT_DIR") {
        let probe = PathBuf::from(&out_dir).join(".git");
        if fs::create_dir(&probe).is_err() {
            git_denied = true;
        } else {
            let _ = fs::remove_dir(&probe);
        }
    }
    let temp_probe = env::temp_dir().join(format!(".git-probe-{}", std::process::id()));
    if fs::create_dir(&temp_probe).is_err() {
        git_denied = true;
    } else {
        let _ = fs::remove_dir(&temp_probe);
    }
    // `LOTAR_REQUIRE_GIT=1` marks a runner as designated to exercise the real
    // git-dependent tests (CI): refuse to build if the capability probe failed,
    // instead of silently compiling them out. Any other non-empty value is a
    // configuration error, mirroring LOTAR_SMOKE_REQUIRE_GIT's strictness.
    let require_git = match env::var("LOTAR_REQUIRE_GIT") {
        Ok(value) if value == "1" => true,
        Ok(value) if value.is_empty() => false,
        Ok(value) => panic!("LOTAR_REQUIRE_GIT must be set to '1' or left unset; got {value:?}"),
        Err(env::VarError::NotPresent) => false,
        Err(err) => panic!("LOTAR_REQUIRE_GIT is not valid unicode: {err}"),
    };
    if require_git {
        println!("cargo::rustc-cfg=lotar_require_git");
    }
    if git_denied && require_git {
        panic!(
            "LOTAR_REQUIRE_GIT=1 requires real git capability, but creating a '.git' directory is denied in this environment; refusing to build on a designated full-coverage runner"
        );
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
