pub mod validator;
pub mod errors;
pub mod conflicts;

pub use validator::ConfigValidator;
pub use errors::{ValidationSeverity};
// Note: ValidationError, ValidationResult, PrefixConflictDetector are available but not re-exported 
// unless specifically needed to avoid unused import warnings
