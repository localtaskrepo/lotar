use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationSeverity {
    Error,
    Warning,
    #[allow(dead_code)]
    Info,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub severity: ValidationSeverity,
    pub field: Option<String>,
    pub message: String,
    pub fix_suggestion: Option<String>,
}

impl ValidationError {
    pub fn error(field: Option<String>, message: String) -> Self {
        Self {
            severity: ValidationSeverity::Error,
            field,
            message,
            fix_suggestion: None,
        }
    }

    pub fn warning(field: Option<String>, message: String) -> Self {
        Self {
            severity: ValidationSeverity::Warning,
            field,
            message,
            fix_suggestion: None,
        }
    }

    pub fn with_fix(mut self, fix: String) -> Self {
        self.fix_suggestion = Some(fix);
        self
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity_icon = match self.severity {
            ValidationSeverity::Error => "❌",
            ValidationSeverity::Warning => "⚠️ ",
            ValidationSeverity::Info => "ℹ️ ",
        };

        if let Some(field) = &self.field {
            write!(f, "{} {}: {}", severity_icon, field, self.message)?;
        } else {
            write!(f, "{} {}", severity_icon, self.message)?;
        }

        if let Some(fix) = &self.fix_suggestion {
            write!(f, "\n   💡 Suggestion: {}", fix)?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
    pub info: Vec<ValidationError>,
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            info: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: ValidationError) {
        match error.severity {
            ValidationSeverity::Error => self.errors.push(error),
            ValidationSeverity::Warning => self.warnings.push(error),
            ValidationSeverity::Info => self.info.push(error),
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    #[allow(dead_code)]
    pub fn has_issues(&self) -> bool {
        self.has_errors() || self.has_warnings()
    }

    #[allow(dead_code)]
    pub fn total_issues(&self) -> usize {
        self.errors.len() + self.warnings.len()
    }

    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
        self.info.extend(other.info);
    }
}

impl fmt::Display for ValidationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for error in &self.errors {
            writeln!(f, "{}", error)?;
        }
        for warning in &self.warnings {
            writeln!(f, "{}", warning)?;
        }
        for info in &self.info {
            writeln!(f, "{}", info)?;
        }
        Ok(())
    }
}
