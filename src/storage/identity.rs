//! Canonical task identity and location resolution (DEV-56).
//!
//! A task ID is `<PROJECT>-<NUMBER>` where the numeric suffix is the FINAL
//! dash-separated segment. The project prefix may itself contain dashes
//! (`ABC-OPS-12` -> project `ABC-OPS`, number `12`), may start with a digit,
//! and may contain Unicode letters; its grammar is exactly
//! [`crate::storage::safety::is_valid_project_prefix`]. Prefixes are exact
//! case: IDs are never folded, and lookups do not match `abc` against `ABC`.
//!
//! Leading zeros in the numeric suffix are a deliberate padded-alias spelling:
//! `TP-001` and `TP-1` denote the same task file `TP/1.yml`. Malformed IDs
//! (empty, no dash, non-numeric suffix, `+`-prefixed numbers, traversal, or a
//! number that overflows `u64`) fail closed.
//!
//! Resolution searches the primary tasks root plus sibling workspace roots
//! (see [`crate::storage::locator::StorageLocator::candidate_task_roots`]) and
//! fails closed when the same ID exists in more than one location: an
//! ambiguous identity is never silently mapped to an arbitrary task.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::storage::locator::StorageLocator;
use crate::storage::safety::is_valid_project_prefix;

/// A canonically parsed task ID: validated project prefix plus numeric value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId {
    /// Exact-case project prefix (a valid folder name under a tasks root).
    pub project: String,
    /// Canonical numeric value; `TP-001` and `TP-1` both yield `1`.
    pub number: u64,
}

/// Why a task ID string is not canonically parseable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskIdError {
    Empty,
    MissingNumericSuffix,
    InvalidPrefix(String),
    InvalidNumericSuffix(String),
    NumericOverflow(String),
}

impl fmt::Display for TaskIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskIdError::Empty => write!(f, "task ID is empty"),
            TaskIdError::MissingNumericSuffix => {
                write!(f, "expected PROJECT-NUMBER with a final numeric suffix")
            }
            TaskIdError::InvalidPrefix(prefix) => write!(
                f,
                "project prefix '{prefix}' is invalid: must be 1-64 alphanumeric (or '_'/'-') \
                 characters without path separators"
            ),
            TaskIdError::InvalidNumericSuffix(suffix) => write!(
                f,
                "numeric suffix '{suffix}' is invalid: expected decimal digits only"
            ),
            TaskIdError::NumericOverflow(suffix) => {
                write!(f, "numeric suffix '{suffix}' overflows the supported range")
            }
        }
    }
}

impl std::error::Error for TaskIdError {}

impl TaskId {
    /// Parse a task ID canonically: the FINAL dash-separated segment must be
    /// the numeric suffix and the remaining prefix must be a valid project
    /// prefix. Parsing performs no trimming, folding, or normalization beyond
    /// collapsing padded numeric spellings to their numeric value.
    pub fn parse(input: &str) -> Result<Self, TaskIdError> {
        if input.is_empty() {
            return Err(TaskIdError::Empty);
        }
        let (prefix, suffix) = input
            .rsplit_once('-')
            .ok_or(TaskIdError::MissingNumericSuffix)?;
        if !is_valid_project_prefix(prefix) {
            return Err(TaskIdError::InvalidPrefix(prefix.to_string()));
        }
        if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()) {
            return Err(TaskIdError::InvalidNumericSuffix(suffix.to_string()));
        }
        let number = suffix
            .parse::<u64>()
            .map_err(|_| TaskIdError::NumericOverflow(suffix.to_string()))?;
        Ok(Self {
            project: prefix.to_string(),
            number,
        })
    }

    /// Canonical spelling: unpadded number, exact-case prefix.
    pub fn canonical(&self) -> String {
        format!("{}-{}", self.project, self.number)
    }

    /// Every candidate location whose `<root>/<project>/<number>.yml` exists.
    /// Candidates are the provided root plus sibling workspace roots, in the
    /// sorted order of [`StorageLocator::candidate_task_roots`] (no priority:
    /// a single match is required for resolution). Multiple results mean the
    /// same ID is stored in more than one root and identity is ambiguous.
    pub fn locate(&self, root: &Path) -> Vec<TaskLocation> {
        let mut out = Vec::new();
        for candidate in StorageLocator::candidate_task_roots(root) {
            let file = candidate
                .join(&self.project)
                .join(format!("{}.yml", self.number));
            if file.is_file() {
                out.push(TaskLocation {
                    root: candidate,
                    id: self.clone(),
                    file,
                });
            }
        }
        out
    }
}

/// Canonical alias equality: both spellings must be canonically parseable
/// and denote the same ticket (`TP-001` == `TP-1`). Anything unparseable
/// matches nothing, so unrelated or malformed identifiers fail closed.
pub fn aliases_match(a: &str, b: &str) -> bool {
    match (TaskId::parse(a.trim()), TaskId::parse(b.trim())) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

/// Where a task file actually lives: which tasks root, which project folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLocation {
    /// Canonical tasks root holding the task (`<workspace>/.tasks`).
    pub root: PathBuf,
    /// Parsed identity of the task.
    pub id: TaskId,
    /// Absolute path of the task YAML file.
    pub file: PathBuf,
}

impl TaskLocation {
    pub fn full_id(&self) -> String {
        self.id.canonical()
    }

    /// True when this location's tasks root is `root` (canonical comparison).
    pub fn is_in_root(&self, root: &Path) -> bool {
        let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        self.root == canonical
    }
}

/// Error resolving a task identifier to exactly one storage location.
#[derive(Debug)]
pub enum TaskLookupError {
    /// The identifier is not canonically parseable.
    Invalid(TaskIdError),
    /// No candidate root holds the task.
    NotFound(String),
    /// More than one root/project holds this identifier; picking one would
    /// risk mutating the wrong task, so resolution fails closed instead.
    Ambiguous {
        request: String,
        matches: Vec<TaskLocation>,
    },
}

impl fmt::Display for TaskLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskLookupError::Invalid(err) => write!(f, "Invalid task ID: {err}"),
            TaskLookupError::NotFound(request) => {
                write!(f, "Task '{request}' not found in any workspace tasks root")
            }
            TaskLookupError::Ambiguous { request, matches } => {
                write!(
                    f,
                    "Task '{}' matches multiple storage locations: {}; refusing to pick one arbitrarily",
                    request,
                    matches
                        .iter()
                        .map(|location| format!(
                            "{} ({})",
                            location.file.display(),
                            location.full_id()
                        ))
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            }
        }
    }
}

impl std::error::Error for TaskLookupError {}

impl From<TaskIdError> for TaskLookupError {
    fn from(err: TaskIdError) -> Self {
        TaskLookupError::Invalid(err)
    }
}

/// Resolve a full task ID to its single storage location across the primary
/// and sibling workspace roots.
pub fn resolve(root: &Path, raw: &str) -> Result<TaskLocation, TaskLookupError> {
    let id = TaskId::parse(raw)?;
    single(id.locate(root), raw)
}

/// Resolve a bare numeric identifier (`"12"`) across every project folder of
/// every candidate root. A number stored by more than one project is
/// ambiguous and fails closed.
pub fn resolve_numeric(root: &Path, raw: &str) -> Result<TaskLocation, TaskLookupError> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TaskLookupError::Invalid(TaskIdError::InvalidNumericSuffix(
            raw.to_string(),
        )));
    }
    let number = raw
        .parse::<u64>()
        .map_err(|_| TaskLookupError::Invalid(TaskIdError::NumericOverflow(raw.to_string())))?;
    let mut matches = Vec::new();
    for candidate in StorageLocator::candidate_task_roots(root) {
        for (project, dir) in crate::utils::filesystem::list_visible_subdirs(&candidate) {
            if !is_valid_project_prefix(&project) {
                continue;
            }
            let file = dir.join(format!("{}.yml", number));
            if file.is_file() {
                matches.push(TaskLocation {
                    root: candidate.clone(),
                    id: TaskId { project, number },
                    file,
                });
            }
        }
    }
    single(matches, raw)
}

fn single(mut matches: Vec<TaskLocation>, request: &str) -> Result<TaskLocation, TaskLookupError> {
    match matches.len() {
        1 => Ok(matches.swap_remove(0)),
        0 => Err(TaskLookupError::NotFound(request.to_string())),
        _ => Err(TaskLookupError::Ambiguous {
            request: request.to_string(),
            matches,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_uppercase_id() {
        let id = TaskId::parse("TP-1").unwrap();
        assert_eq!(id.project, "TP");
        assert_eq!(id.number, 1);
        assert_eq!(id.canonical(), "TP-1");
    }

    #[test]
    fn parse_hyphenated_prefix_uses_final_numeric_suffix() {
        let id = TaskId::parse("ABC-OPS-12").unwrap();
        assert_eq!(id.project, "ABC-OPS");
        assert_eq!(id.number, 12);
        assert_eq!(id.canonical(), "ABC-OPS-12");
    }

    #[test]
    fn parse_multi_hyphen_prefix_with_numeric_tail_segment() {
        // The final segment is numeric, so the prefix keeps earlier numerics.
        let id = TaskId::parse("ABC-OPS-12-3").unwrap();
        assert_eq!(id.project, "ABC-OPS-12");
        assert_eq!(id.number, 3);
    }

    #[test]
    fn parse_digit_leading_prefix() {
        let id = TaskId::parse("42-7").unwrap();
        assert_eq!(id.project, "42");
        assert_eq!(id.number, 7);
    }

    #[test]
    fn parse_lowercase_and_unicode_prefixes_exact_case() {
        let lower = TaskId::parse("dev-ops-4").unwrap();
        assert_eq!(lower.project, "dev-ops");
        assert_eq!(lower.canonical(), "dev-ops-4");

        let unicode = TaskId::parse("ÜBER-2").unwrap();
        assert_eq!(unicode.project, "ÜBER");
    }

    #[test]
    fn parse_padded_alias_collapses_to_number() {
        let id = TaskId::parse("TP-001").unwrap();
        assert_eq!(id.number, 1);
        assert_eq!(id.canonical(), "TP-1");
        assert_eq!(TaskId::parse("TP-001"), TaskId::parse("TP-1"));
    }

    #[test]
    fn parse_accepts_underscore_in_prefix() {
        let id = TaskId::parse("My_Project-2").unwrap();
        assert_eq!(id.project, "My_Project");
    }

    #[test]
    fn parse_rejects_empty_and_separatorless_ids() {
        assert_eq!(TaskId::parse(""), Err(TaskIdError::Empty));
        assert_eq!(
            TaskId::parse("TP123"),
            Err(TaskIdError::MissingNumericSuffix)
        );
    }

    #[test]
    fn parse_rejects_malformed_suffixes() {
        for bad in [
            "TP-ABC", "TP-", "TP--", "TP-1x", "TP-x1", "TP-1.5", "TP-1 2", "TP- 1", "TP-1 ",
            "TP-１", // fullwidth digit is not ASCII
        ] {
            assert!(
                matches!(
                    TaskId::parse(bad),
                    Err(TaskIdError::InvalidNumericSuffix(_))
                ),
                "expected numeric-suffix rejection for {bad:?}"
            );
        }
    }

    #[test]
    fn parse_rejects_plus_digit_suffix() {
        assert!(matches!(
            TaskId::parse("TP-+12"),
            Err(TaskIdError::InvalidNumericSuffix(_))
        ));
        assert!(matches!(
            TaskId::parse("TP-12+"),
            Err(TaskIdError::InvalidNumericSuffix(_))
        ));
    }

    #[test]
    fn parse_rejects_traversal_and_unsafe_prefixes() {
        for bad in [
            "../evil-1",
            "..-1",
            ".-1",
            "a/b-1",
            "a\\b-1",
            "/tmp/abs-1",
            "@sprints-1",
            ".hidden-1",
            "-flag-1",
            "a b-1",
            "a..b-1",
        ] {
            assert!(
                matches!(TaskId::parse(bad), Err(TaskIdError::InvalidPrefix(_))),
                "expected prefix rejection for {bad:?}"
            );
        }
    }

    #[test]
    fn parse_rejects_numeric_overflow() {
        assert!(matches!(
            TaskId::parse("TP-99999999999999999999999"),
            Err(TaskIdError::NumericOverflow(_))
        ));
        assert_eq!(
            TaskId::parse("TP-18446744073709551615").unwrap().number,
            u64::MAX
        );
    }

    #[test]
    fn parse_rejects_overlong_prefix() {
        let long = "A".repeat(65);
        assert!(matches!(
            TaskId::parse(&format!("{long}-1")),
            Err(TaskIdError::InvalidPrefix(_))
        ));
        let max = format!("{}-1", "A".repeat(64));
        assert!(TaskId::parse(&max).is_ok());
    }

    #[test]
    fn parse_no_case_folding_between_ids() {
        let upper = TaskId::parse("ABC-OPS-12").unwrap();
        let lower = TaskId::parse("abc-ops-12").unwrap();
        assert_ne!(upper.project, lower.project);
        assert_ne!(upper, lower);
    }

    #[test]
    fn resolve_numeric_rejects_non_digits_and_overflow() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(".tasks");
        std::fs::create_dir_all(root.join("TP")).unwrap();
        std::fs::write(root.join("TP").join("1.yml"), "x: 1\n").unwrap();

        assert!(matches!(
            resolve_numeric(&root, "abc"),
            Err(TaskLookupError::Invalid(_))
        ));
        assert!(matches!(
            resolve_numeric(&root, "99999999999999999999999"),
            Err(TaskLookupError::Invalid(_))
        ));
        assert!(matches!(
            resolve_numeric(&root, "2"),
            Err(TaskLookupError::NotFound(_))
        ));
        assert_eq!(resolve_numeric(&root, "1").unwrap().full_id(), "TP-1");
    }
}
