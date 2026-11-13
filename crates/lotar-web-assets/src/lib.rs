//! The web UI, embedded from `target/web-embed` (DEV-497). Only the `lotar`
//! binary depends on this crate, so a web build rebuilds this crate and the
//! binary but not the library and its tests.

use include_dir::{Dir, include_dir};

static FILES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../target/web-embed");

/// The bytes of an embedded file, by its path inside `target/web-embed`
/// (text assets carry a `.gz` suffix).
pub fn get(path: &str) -> Option<&'static [u8]> {
    FILES.get_file(path).map(|file| file.contents())
}
