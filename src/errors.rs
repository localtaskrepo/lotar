use std::error::Error;
use std::fmt;

/// Error types for LoTaR operations
#[derive(Debug)]
pub enum LoTaRError {
    IoError(std::io::Error),
    SerializationError(String),
    TaskNotFound(String),
    InvalidTaskId(String),
    ProjectNotFound(String),
    ValidationError(String),
    IndexError(String),
}

impl fmt::Display for LoTaRError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LoTaRError::IoError(err) => write!(f, "IO error: {}", err),
            LoTaRError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            LoTaRError::TaskNotFound(id) => write!(f, "Task not found: {}", id),
            LoTaRError::InvalidTaskId(id) => write!(f, "Invalid task ID: {}", id),
            LoTaRError::ProjectNotFound(name) => write!(f, "Project not found: {}", name),
            LoTaRError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            LoTaRError::IndexError(msg) => write!(f, "Index error: {}", msg),
        }
    }
}

impl Error for LoTaRError {}

impl From<std::io::Error> for LoTaRError {
    fn from(error: std::io::Error) -> Self {
        LoTaRError::IoError(error)
    }
}

impl From<serde_yaml::Error> for LoTaRError {
    fn from(error: serde_yaml::Error) -> Self {
        LoTaRError::SerializationError(error.to_string())
    }
}

pub type LoTaRResult<T> = Result<T, LoTaRError>;
