use std::path::PathBuf;

use crate::cli::handlers::sprint::shared::{emit_cleanup_summary, emit_missing_report};
use crate::output::OutputRenderer;
use crate::services::sprint_integrity::{self, MissingSprintReport, SprintCleanupOutcome};
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::storage::manager::Storage;

pub(crate) struct AssignmentContext {
    pub(crate) storage: Storage,
    pub(crate) records: Vec<SprintRecord>,
    baseline_integrity: MissingSprintReport,
    integrity: MissingSprintReport,
}

impl AssignmentContext {
    pub(crate) fn for_mutation(tasks_root: PathBuf) -> Result<Self, String> {
        let storage = Storage::new(tasks_root);
        Self::from_storage(storage)
    }

    pub(crate) fn try_open(tasks_root: PathBuf) -> Result<Option<Self>, String> {
        match Storage::try_open(tasks_root) {
            Some(storage) => Self::from_storage(storage).map(Some),
            None => Ok(None),
        }
    }

    pub(crate) fn baseline_integrity(&self) -> &MissingSprintReport {
        &self.baseline_integrity
    }

    pub(crate) fn integrity(&self) -> &MissingSprintReport {
        &self.integrity
    }

    pub(crate) fn reconcile_missing(
        &mut self,
        cleanup_requested: bool,
        renderer: &OutputRenderer,
        context_label: &str,
    ) -> Result<Option<SprintCleanupOutcome>, String> {
        if self.integrity.missing_sprints.is_empty() {
            return Ok(None);
        }

        if cleanup_requested {
            let outcome = sprint_integrity::cleanup_missing_sprint_refs(
                &mut self.storage,
                &mut self.records,
                None,
            )
            .map_err(|err| err.to_string())?;
            emit_cleanup_summary(renderer, &outcome, context_label);
            self.integrity = sprint_integrity::detect_missing_sprints(&self.storage, &self.records);
            Ok(Some(outcome))
        } else {
            emit_missing_report(renderer, &self.integrity, context_label);
            Ok(None)
        }
    }

    fn from_storage(storage: Storage) -> Result<Self, String> {
        let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
        let integrity = sprint_integrity::detect_missing_sprints(&storage, &records);
        Ok(Self {
            storage,
            records,
            baseline_integrity: integrity.clone(),
            integrity,
        })
    }
}
