use std::path::Path;

use crate::cli::handlers::sprint::shared::{emit_cleanup_summary, emit_missing_report};
use crate::output::OutputRenderer;
use crate::services::sprint_integrity::{
    self, IntegrityCheck, MissingSprintReport, SprintCleanupOutcome,
};
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::storage::manager::Storage;

pub(crate) struct AssignmentContext {
    pub(crate) storage: Storage,
    pub(crate) records: Vec<SprintRecord>,
    integrity: IntegrityCheck,
}

impl AssignmentContext {
    pub(crate) fn for_mutation(tasks_root: &Path) -> Result<Self, String> {
        let storage = Storage::new(tasks_root);
        Self::from_storage(storage)
    }

    pub(crate) fn try_open(tasks_root: &Path) -> Result<Option<Self>, String> {
        match Storage::try_open(tasks_root) {
            Some(storage) => Self::from_storage(storage).map(Some),
            None => Ok(None),
        }
    }

    pub(crate) fn baseline_integrity(&self) -> &MissingSprintReport {
        &self.integrity.before
    }

    pub(crate) fn integrity(&self) -> &MissingSprintReport {
        &self.integrity.after
    }

    pub(crate) fn reconcile_missing(
        &mut self,
        cleanup_requested: bool,
        renderer: &OutputRenderer,
        context_label: &str,
    ) -> Result<Option<SprintCleanupOutcome>, String> {
        self.integrity = sprint_integrity::check_missing_sprints(
            &mut self.storage,
            &mut self.records,
            cleanup_requested,
            None,
        )
        .map_err(|err| err.to_string())?;
        match self.integrity.cleanup.as_ref() {
            Some(outcome) => emit_cleanup_summary(renderer, outcome, context_label),
            None if !self.integrity.after.missing_sprints.is_empty() => {
                emit_missing_report(renderer, &self.integrity.after, context_label);
            }
            None => {}
        }
        Ok(self.integrity.cleanup.clone())
    }

    fn from_storage(storage: Storage) -> Result<Self, String> {
        let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
        // Detection runs in `reconcile_missing`, which every caller invokes
        // before reading the integrity reports.
        Ok(Self {
            storage,
            records,
            integrity: IntegrityCheck::unchanged(MissingSprintReport::default()),
        })
    }
}
