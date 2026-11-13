//! Which tasks this thread wrote while a log is open (DEV-410). A manual
//! automation run matches every task against one listing read up front
//! (DEV-400); an action on one task can change another (closing it fires
//! its dependents' `unblocked` rules), so the run re-reads the tasks this
//! log names before matching them.

use crate::storage::identity::TaskId;
use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    static LOG: RefCell<Option<HashSet<String>>> = const { RefCell::new(None) };
}

/// Records task writes on this thread until it drops. A log opened inside
/// another hands its entries on to the outer one when it drops.
pub struct WriteLog {
    outer: Option<HashSet<String>>,
}

impl WriteLog {
    pub fn open() -> Self {
        let outer = LOG.with(|log| log.borrow_mut().replace(HashSet::new()));
        Self { outer }
    }

    /// Whether a task with this ID was written since the log opened.
    pub fn wrote(&self, id: &str) -> bool {
        let id = canonical(id);
        LOG.with(|log| log.borrow().as_ref().is_some_and(|written| written.contains(&id)))
    }
}

impl Drop for WriteLog {
    fn drop(&mut self) {
        LOG.with(|log| {
            let mut log = log.borrow_mut();
            let inner = log.take().unwrap_or_default();
            *log = self.outer.take().map(|mut outer| {
                outer.extend(inner);
                outer
            });
        });
    }
}

/// Note a write to the task with this ID, when a log is open.
pub(crate) fn record(id: &str) {
    LOG.with(|log| {
        if let Some(written) = log.borrow_mut().as_mut() {
            written.insert(canonical(id));
        }
    });
}

/// Note a write to the file at this `/`-separated path under a tasks root
/// (`PROJECT/[folders/]N.yml`); other files are ignored.
pub(crate) fn record_path(relative: &str) {
    let Some((project, rest)) = relative.split_once('/') else {
        return;
    };
    let Some(number) = rest.rsplit('/').next().and_then(|file| file.strip_suffix(".yml")) else {
        return;
    };
    if !project.starts_with('@') && !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
        record(&format!("{project}-{number}"));
    }
}

fn canonical(id: &str) -> String {
    TaskId::parse(id).map(|parsed| parsed.canonical()).unwrap_or_else(|_| id.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_only_while_open_and_hands_on_to_the_outer_log() {
        record("A-1");
        let outer = WriteLog::open();
        assert!(!outer.wrote("A-1"));
        record_path("A/2.yml");
        {
            let inner = WriteLog::open();
            record_path("B/sub/3.yml");
            record_path("@sprints/4.yml");
            record_path("A/config.yml");
            assert!(inner.wrote("B-3"));
            assert!(!inner.wrote("A-2"));
        }
        assert!(outer.wrote("A-2"));
        assert!(outer.wrote("B-3"));
        assert!(!outer.wrote("@sprints-4"));
        drop(outer);
        record("C-5");
        assert!(!WriteLog::open().wrote("C-5"));
    }
}
