//! Read-only access to repository files for the web UI's file viewer
//! (DEV-549): file names in descriptions and comments link to the files they
//! name. Only files git would track are visible: ignored files (`.env`,
//! build output) and `.git` stay out, and nothing outside the repository is
//! reachable, symlinks included.

use ignore::WalkBuilder;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Largest file the viewer reads.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Most references one resolve request may ask for.
pub const MAX_RESOLVE_REFERENCES: usize = 500;

/// How long a listing of the repository is reused. A rendered task asks once
/// per description and comment, so this spares a walk per block.
const LISTING_TTL: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
pub enum RepoFileError {
    /// Not a visible file of the repository: missing, ignored, a directory,
    /// or outside it.
    NotFound,
    TooLarge {
        size: u64,
    },
    Io(String),
}

struct Listing {
    root: PathBuf,
    at: Instant,
    files: Arc<Vec<String>>,
}

static LISTING: Mutex<Option<Listing>> = Mutex::new(None);

pub struct RepoFileService;

impl RepoFileService {
    /// Repository-relative paths (with `/`) of every file git would track,
    /// sorted. Cached for a few seconds per repository root.
    pub fn list(repo_root: &Path) -> Arc<Vec<String>> {
        let root = canonical(repo_root);
        if let Ok(guard) = LISTING.lock()
            && let Some(listing) = guard.as_ref()
            && listing.root == root
            && listing.at.elapsed() < LISTING_TTL
        {
            return listing.files.clone();
        }
        let files = Arc::new(Self::walk(&root));
        if let Ok(mut guard) = LISTING.lock() {
            *guard = Some(Listing { root, at: Instant::now(), files: files.clone() });
        }
        files
    }

    fn walk(root: &Path) -> Vec<String> {
        let mut builder = WalkBuilder::new(root);
        // Dot directories such as `.github` hold files worth reading; only
        // what git ignores stays out, and `.git` itself.
        builder.hidden(false).require_git(false).filter_entry(|entry| entry.file_name() != ".git");
        let mut files = Vec::new();
        let mut links = Vec::new();
        for entry in builder.build().filter_map(Result::ok) {
            let Some(kind) = entry.file_type() else { continue };
            let Ok(relative) = entry.path().strip_prefix(root) else { continue };
            if kind.is_file() {
                files.push(display_path(relative));
            } else if kind.is_symlink() {
                links.push((display_path(relative), entry.into_path()));
            }
        }
        files.sort();
        // A symlink counts when it points at a visible file, so it cannot
        // reach outside the repository or into an ignored file.
        let linked: Vec<String> = links
            .into_iter()
            .filter(|(_, path)| {
                path.canonicalize()
                    .ok()
                    .and_then(|target| target.strip_prefix(root).ok().map(display_path))
                    .is_some_and(|target| files.binary_search(&target).is_ok())
            })
            .map(|(relative, _)| relative)
            .collect();
        files.extend(linked);
        files.sort();
        files
    }

    /// Repository path of the file `reference` names, or `None` when it names
    /// no visible file. `base` is the directory of the file the reference
    /// appears in, for relative links inside repository markdown.
    ///
    /// Tried in order: relative to `base`, relative to the repository root
    /// (absolute paths inside it count), then the one file whose path ends
    /// with the reference: `api_types.rs` or `src/routes/files.rs` when only
    /// one file matches.
    pub fn resolve(repo_root: &Path, files: &[String], base: Option<&str>, reference: &str) -> Option<String> {
        let reference = reference.trim();
        if reference.is_empty() {
            return None;
        }
        let path = Path::new(reference);
        let relative = if path.is_absolute() {
            let root = canonical(repo_root);
            let inside = path.strip_prefix(&root).or_else(|_| path.strip_prefix(repo_root)).ok()?;
            display_path(inside)
        } else {
            reference.to_string()
        };
        let known = |candidate: &str| files.binary_search_by(|file| file.as_str().cmp(candidate)).is_ok();

        if !path.is_absolute()
            && let Some(base) = base.map(str::trim).filter(|base| !base.is_empty())
            && let Some(joined) = normalize(&format!("{base}/{relative}"))
            && known(&joined)
        {
            return Some(joined);
        }
        let normalized = normalize(&relative)?;
        if known(&normalized) {
            return Some(normalized);
        }
        if path.is_absolute() {
            return None;
        }
        let suffix = format!("/{normalized}");
        let mut matches = files.iter().filter(|file| file.ends_with(&suffix));
        let first = matches.next()?;
        matches.next().is_none().then(|| first.clone())
    }

    /// Bytes of a visible repository file.
    pub fn read(repo_root: &Path, files: &[String], path: &str) -> Result<Vec<u8>, RepoFileError> {
        let normalized = normalize(path).ok_or(RepoFileError::NotFound)?;
        if files.binary_search(&normalized).is_err() {
            return Err(RepoFileError::NotFound);
        }
        let root = canonical(repo_root);
        let target = root.join(&normalized).canonicalize().map_err(|_| RepoFileError::NotFound)?;
        if !target.starts_with(&root) || !target.is_file() {
            return Err(RepoFileError::NotFound);
        }
        let size = std::fs::metadata(&target).map_err(|err| RepoFileError::Io(err.to_string()))?.len();
        if size > MAX_FILE_BYTES {
            return Err(RepoFileError::TooLarge { size });
        }
        std::fs::read(&target).map_err(|err| RepoFileError::Io(err.to_string()))
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Resolve `.` and `..` in a relative `/` path; `None` when it climbs above
/// its start or is absolute.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path();
        for (path, text) in [
            (".gitignore", "target/\n.env\n"),
            (".env", "SECRET=1\n"),
            ("target/debug/out.rs", "fn main() {}\n"),
            ("README.md", "# Readme\n"),
            ("docs/help/serve.md", "serve\n"),
            ("docs/help/sse.md", "sse\n"),
            ("src/api_types.rs", "pub struct A;\n"),
            ("src/routes/mod.rs", "mod files;\n"),
            ("tests/mod.rs", "mod a;\n"),
            (".github/skills/release/SKILL.md", "skill\n"),
            (".git/config", "[core]\n"),
        ] {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().expect("parent")).expect("dirs");
            std::fs::write(full, text).expect("write");
        }
        tmp
    }

    #[test]
    fn lists_visible_files_only() {
        let tmp = repo();
        let files = RepoFileService::walk(&canonical(tmp.path()));
        assert!(files.contains(&".github/skills/release/SKILL.md".to_string()));
        assert!(files.contains(&"README.md".to_string()));
        for hidden in [".env", "target/debug/out.rs", ".git/config"] {
            assert!(!files.contains(&hidden.to_string()), "{hidden} should stay out: {files:?}");
        }
    }

    #[test]
    fn resolves_paths_names_and_relative_links() {
        let tmp = repo();
        let root = canonical(tmp.path());
        let files = RepoFileService::walk(&root);
        let resolve = |base: Option<&str>, reference: &str| RepoFileService::resolve(&root, &files, base, reference);

        assert_eq!(resolve(None, "docs/help/serve.md").as_deref(), Some("docs/help/serve.md"));
        assert_eq!(resolve(None, "./README.md").as_deref(), Some("README.md"));
        assert_eq!(resolve(None, "api_types.rs").as_deref(), Some("src/api_types.rs"));
        assert_eq!(resolve(None, "routes/mod.rs").as_deref(), Some("src/routes/mod.rs"));
        // Two files end in `mod.rs`: ambiguous names stay unlinked.
        assert_eq!(resolve(None, "mod.rs"), None);
        assert_eq!(resolve(Some("docs/help"), "sse.md").as_deref(), Some("docs/help/sse.md"));
        assert_eq!(resolve(Some("docs/help"), "../../README.md").as_deref(), Some("README.md"));
        let absolute = root.join("docs/help/serve.md");
        assert_eq!(resolve(None, &absolute.to_string_lossy()).as_deref(), Some("docs/help/serve.md"));

        for missing in [".env", "missing.md", "../outside.md", "/etc/hosts", "docs/help", "e.g."] {
            assert_eq!(resolve(None, missing), None, "{missing}");
        }
    }

    #[test]
    fn reads_visible_files_and_refuses_the_rest() {
        let tmp = repo();
        let root = canonical(tmp.path());
        let files = RepoFileService::walk(&root);
        assert_eq!(RepoFileService::read(&root, &files, "README.md").as_deref(), Ok(b"# Readme\n".as_slice()));
        for refused in [".env", ".git/config", "../README.md", "/etc/hosts", "docs"] {
            assert_eq!(RepoFileService::read(&root, &files, refused), Err(RepoFileError::NotFound), "{refused}");
        }
    }

    #[test]
    fn refuses_files_over_the_size_cap() {
        let tmp = repo();
        let root = canonical(tmp.path());
        let big = vec![b'a'; usize::try_from(MAX_FILE_BYTES).expect("fits") + 1];
        std::fs::write(root.join("big.txt"), big).expect("write");
        let files = RepoFileService::walk(&root);
        assert_eq!(RepoFileService::read(&root, &files, "big.txt"), Err(RepoFileError::TooLarge { size: MAX_FILE_BYTES + 1 }));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_count_only_inside_the_repository() {
        let tmp = repo();
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret.txt"), "x").expect("write");
        let root = canonical(tmp.path());
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), root.join("escape.txt")).expect("symlink");
        std::os::unix::fs::symlink(root.join("README.md"), root.join("alias.md")).expect("symlink");
        std::os::unix::fs::symlink(root.join(".env"), root.join("env.txt")).expect("symlink");
        let files = RepoFileService::walk(&root);
        assert!(files.contains(&"alias.md".to_string()));
        assert!(!files.contains(&"escape.txt".to_string()));
        assert!(!files.contains(&"env.txt".to_string()), "a link to an ignored file stays out");
        assert_eq!(RepoFileService::read(&root, &files, "escape.txt"), Err(RepoFileError::NotFound));
    }
}
