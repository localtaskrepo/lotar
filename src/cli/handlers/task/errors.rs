use crate::errors::LoTaRError;

/// Storage operation categories for consistent error messaging.
#[derive(Debug, Clone, Copy)]
pub enum TaskStorageAction {
    Create,
    Update,
    Delete,
}

impl TaskStorageAction {
    fn activity(self, subject: &str) -> String {
        match self {
            TaskStorageAction::Create => format!("creating task in {}", subject),
            TaskStorageAction::Update => format!("updating task {}", subject),
            TaskStorageAction::Delete => format!("deleting task {}", subject),
        }
    }

    /// Format a user-facing error message for a storage failure.
    pub fn format_error(self, subject: &str, err: LoTaRError) -> String {
        let permission_hint = match &err {
            LoTaRError::IoError(io_err)
                if io_err.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                Some(" (check file and directory permissions)")
            }
            _ => None,
        };

        let mut detail = err.to_string();
        if let Some(hint) = permission_hint {
            detail.push_str(hint);
        }

        format!("Storage error while {}: {}", self.activity(subject), detail)
    }

    /// Convenience helper to build a mapper for `Result::map_err`.
    pub fn map_err<'a>(self, subject: &'a str) -> impl FnOnce(LoTaRError) -> String + 'a {
        move |err| self.format_error(subject, err)
    }
}
