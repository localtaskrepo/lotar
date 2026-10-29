//! Where task files live inside a project folder (DEV-4).
//!
//! A task's identity is its project folder plus its numeric file name
//! (`DEV/12.yml` is `DEV-12`); the folders in between carry no meaning.
//! Tasks may sit in any subfolder of their project, so teams can group them
//! by hand. Folders whose names start with `.` or `@` are not scanned
//! (`@attachments`, editor folders), with one exception: `<project>/@archive`
//! holds archived tasks. Normal scans skip it, lookups by ID still find it.
//!
//! Every place that maps a task ID to a file, lists a project's tasks, or
//! reads a task ID back from a path goes through this module.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Folder inside a project that holds archived tasks.
pub const ARCHIVE_DIR: &str = "@archive";

/// Which tasks a scan includes, by whether they are archived.
///
/// Query value `archived=no` (default), `yes` (only archived) or `all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ArchiveFilter {
    #[default]
    #[serde(rename = "no")]
    Exclude,
    #[serde(rename = "yes")]
    Only,
    #[serde(rename = "all")]
    All,
}

impl ArchiveFilter {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "no" | "false" | "" => Ok(Self::Exclude),
            "yes" | "true" | "only" => Ok(Self::Only),
            "all" => Ok(Self::All),
            other => Err(format!(
                "Invalid archived value '{other}'. Use yes, no or all"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exclude => "no",
            Self::Only => "yes",
            Self::All => "all",
        }
    }

    fn includes(self, archived: bool) -> bool {
        match self {
            Self::Exclude => !archived,
            Self::Only => archived,
            Self::All => true,
        }
    }
}

/// One task file found in a project folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFile {
    pub number: u64,
    pub path: PathBuf,
    pub archived: bool,
}

/// Every task file of a project matching `filter`, sorted by number then path.
/// Symlinked folders are not followed, so a link cannot pull in another tree
/// or loop.
pub fn task_files(project_dir: &Path, filter: ArchiveFilter) -> Vec<TaskFile> {
    let mut found = Vec::new();
    if filter.includes(false) {
        walk(project_dir, false, &mut |_| true, &mut found);
    }
    if filter.includes(true) {
        walk(
            &project_dir.join(ARCHIVE_DIR),
            true,
            &mut |_| true,
            &mut found,
        );
    }
    found.sort_by(|a, b| a.number.cmp(&b.number).then_with(|| a.path.cmp(&b.path)));
    found
}

/// One file per number within a project: a number stored twice (two
/// folders, or a folder and the archive) is listed once, the first path
/// wins, and a warning names the ID. Lookups refuse such an ID instead.
/// Copies of an ID in different workspace roots are not affected.
pub fn unique_numbers(project: &str, files: Vec<TaskFile>) -> Vec<TaskFile> {
    let mut out: Vec<TaskFile> = Vec::with_capacity(files.len());
    for file in files {
        if out.last().is_some_and(|prev| prev.number == file.number) {
            crate::storage::safety::warn_duplicate_once(&format!("{project}-{}", file.number));
            continue;
        }
        out.push(file);
    }
    out
}

/// Every file of a project, archived or not, stored as task `number`. More
/// than one result means the number is duplicated (for example after a
/// merge), and callers must refuse to pick one.
pub fn find(project_dir: &Path, number: u64) -> Vec<TaskFile> {
    let mut found = Vec::new();
    let mut wanted = |n: u64| n == number;
    walk(project_dir, false, &mut wanted, &mut found);
    walk(
        &project_dir.join(ARCHIVE_DIR),
        true,
        &mut wanted,
        &mut found,
    );
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// Highest task number stored anywhere in the project, archive included, so a
/// new task never reuses the number of a nested or archived one.
pub fn max_number(project_dir: &Path) -> u64 {
    task_files(project_dir, ArchiveFilter::All)
        .last()
        .map(|file| file.number)
        .unwrap_or(0)
}

/// Where a task file moves when archived (`archive == true`) or brought back.
/// The path below the project folder is kept, so `DEV/web/12.yml` archives to
/// `DEV/@archive/web/12.yml` and comes back to the same place.
pub fn archive_target(project_dir: &Path, file: &Path, archive: bool) -> Option<PathBuf> {
    let rel = file.strip_prefix(project_dir).ok()?;
    let in_archive = rel.starts_with(ARCHIVE_DIR);
    match (archive, in_archive) {
        (true, false) => Some(project_dir.join(ARCHIVE_DIR).join(rel)),
        (false, true) => Some(project_dir.join(rel.strip_prefix(ARCHIVE_DIR).ok()?)),
        _ => None,
    }
}

/// A task path relative to the tasks root, read back as an identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelativeTask {
    pub project: String,
    pub number: u64,
    pub archived: bool,
}

impl RelativeTask {
    pub fn id(&self) -> String {
        format!("{}-{}", self.project, self.number)
    }
}

/// Read `<project>/[@archive/][folders/]<number>.yml` (relative to the tasks
/// root) back as a task. Returns `None` for anything that is not a task file
/// under the rules above: configs, sprint files, attachments, hidden folders.
pub fn parse_relative(rel: &Path) -> Option<RelativeTask> {
    let mut parts: Vec<&str> = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            _ => return None,
        }
    }
    let (file, dirs) = parts.split_last()?;
    let (project, folders) = dirs.split_first()?;
    if !crate::storage::safety::is_valid_project_prefix(project) {
        return None;
    }
    let (stem, ext) = file.rsplit_once('.')?;
    if !ext.eq_ignore_ascii_case("yml") {
        return None;
    }
    let number = crate::utils::filesystem::parse_numeric_stem(stem)?;
    let archived = folders.first() == Some(&ARCHIVE_DIR);
    let nested = if archived { &folders[1..] } else { folders };
    if nested.iter().any(|name| is_skipped_dir(name)) {
        return None;
    }
    Some(RelativeTask {
        project: project.to_string(),
        number,
        archived,
    })
}

/// True when `file` is inside its project's archive folder.
pub fn is_archived(project_dir: &Path, file: &Path) -> bool {
    file.strip_prefix(project_dir)
        .is_ok_and(|rel| rel.starts_with(ARCHIVE_DIR))
}

fn is_skipped_dir(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('@')
}

fn walk(
    dir: &Path,
    archived: bool,
    wanted: &mut dyn FnMut(u64) -> bool,
    found: &mut Vec<TaskFile>,
) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                let name = entry.file_name();
                if !name.to_str().is_some_and(is_skipped_dir) {
                    stack.push(path);
                }
                continue;
            }
            let is_yml = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("yml"));
            // Symlinked task files keep working as before (`is_file` follows them).
            if !is_yml || !path.is_file() {
                continue;
            }
            if let Some(number) = crate::utils::filesystem::file_numeric_stem(&path)
                && wanted(number)
            {
                found.push(TaskFile {
                    number,
                    path,
                    archived,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "title: x\n").unwrap();
    }

    #[test]
    fn scans_nested_folders_and_keeps_archive_apart() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = tmp.path().join("DEV");
        touch(&dev.join("1.yml"));
        touch(&dev.join("web").join("2.yml"));
        touch(&dev.join("web").join("deep").join("3.yml"));
        touch(&dev.join("@archive").join("4.yml"));
        touch(&dev.join("@archive").join("2025").join("5.yml"));
        touch(&dev.join("@attachments").join("6.yml"));
        touch(&dev.join(".git").join("7.yml"));
        touch(&dev.join("config.yml"));

        let numbers = |filter| {
            task_files(&dev, filter)
                .into_iter()
                .map(|file| file.number)
                .collect::<Vec<_>>()
        };
        assert_eq!(numbers(ArchiveFilter::Exclude), [1, 2, 3]);
        assert_eq!(numbers(ArchiveFilter::Only), [4, 5]);
        assert_eq!(numbers(ArchiveFilter::All), [1, 2, 3, 4, 5]);
        assert_eq!(max_number(&dev), 5);
        assert!(find(&dev, 5)[0].archived);
        assert_eq!(find(&dev, 3)[0].path, dev.join("web/deep/3.yml"));
        assert!(find(&dev, 6).is_empty());
    }

    #[test]
    fn find_reports_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = tmp.path().join("DEV");
        touch(&dev.join("a").join("1.yml"));
        touch(&dev.join("@archive").join("1.yml"));
        assert_eq!(find(&dev, 1).len(), 2);
    }

    #[test]
    fn archive_target_keeps_the_subfolder() {
        let dev = Path::new("/t/DEV");
        assert_eq!(
            archive_target(dev, &dev.join("web/12.yml"), true).unwrap(),
            dev.join("@archive/web/12.yml")
        );
        assert_eq!(
            archive_target(dev, &dev.join("@archive/web/12.yml"), false).unwrap(),
            dev.join("web/12.yml")
        );
        assert!(archive_target(dev, &dev.join("12.yml"), false).is_none());
        assert!(archive_target(dev, &dev.join("@archive/12.yml"), true).is_none());
    }

    #[test]
    fn parse_relative_reads_ids_from_paths() {
        let parse = |p: &str| parse_relative(Path::new(p));
        assert_eq!(parse("DEV/12.yml").unwrap().id(), "DEV-12");
        assert_eq!(parse("DEV/web/12.yml").unwrap().id(), "DEV-12");
        let archived = parse("DEV/@archive/2025/12.yml").unwrap();
        assert!(archived.archived);
        assert_eq!(archived.id(), "DEV-12");
        for not_task in [
            "DEV/config.yml",
            "@sprints/1.yml",
            "DEV/@attachments/1.yml",
            "DEV/.hidden/1.yml",
            "DEV/web/@archive/1.yml",
            "12.yml",
            "DEV/12.txt",
            "../DEV/12.yml",
        ] {
            assert!(parse(not_task).is_none(), "{not_task}");
        }
    }
}
