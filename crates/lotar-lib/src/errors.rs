use std::error::Error;
use std::fmt;

/// Error types for LoTaR operations
#[derive(Debug)]
pub enum LoTaRError {
    IoError(std::io::Error),
    SerializationError(String),
    TaskNotFound(String),
    SprintNotFound(u32),
    ReleaseNotFound(u32),
    InvalidTaskId(String),
    ProjectNotFound(String),
    ValidationError(String),
    IndexError(String),
    /// The task changed since the caller read it (`expected_modified` no
    /// longer matches, DEV-301). Nothing was written.
    Conflict(String),
}

impl fmt::Display for LoTaRError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LoTaRError::IoError(err) => write!(f, "IO error: {}", err),
            LoTaRError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            LoTaRError::TaskNotFound(id) => write!(f, "Task not found: {}", id),
            LoTaRError::SprintNotFound(id) => write!(f, "Sprint not found: {}", id),
            LoTaRError::ReleaseNotFound(id) => write!(f, "Release not found: #{}", id),
            LoTaRError::InvalidTaskId(id) => write!(f, "Invalid task ID: {}", id),
            LoTaRError::ProjectNotFound(name) => write!(f, "Project not found: {}", name),
            LoTaRError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            LoTaRError::IndexError(msg) => write!(f, "Index error: {}", msg),
            LoTaRError::Conflict(msg) => write!(f, "Conflict: {}", msg),
        }
    }
}

impl Error for LoTaRError {}

impl From<std::io::Error> for LoTaRError {
    fn from(error: std::io::Error) -> Self {
        LoTaRError::IoError(error)
    }
}

impl From<serde_yaml_ng::Error> for LoTaRError {
    fn from(error: serde_yaml_ng::Error) -> Self {
        LoTaRError::SerializationError(error.to_string())
    }
}

pub type LoTaRResult<T> = Result<T, LoTaRError>;
