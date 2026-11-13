pub mod conflicts;
pub mod errors;
pub mod validator;

pub use errors::ValidationSeverity;
pub use validator::ConfigValidator;
// Note: ValidationError, ValidationResult, PrefixConflictDetector are available but not re-exported
// unless specifically needed to avoid unused import warnings
