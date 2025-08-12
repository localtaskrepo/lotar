use crate::config::types::ResolvedConfig;
use crate::types::{Priority, TaskStatus, TaskType};

/// Configuration-aware validation for CLI inputs
pub struct CliValidator<'a> {
    config: &'a ResolvedConfig,
}

impl<'a> CliValidator<'a> {
    pub fn new(config: &'a ResolvedConfig) -> Self {
        Self { config }
    }

    /// Validate status against project configuration (case-insensitive, returns canonical form)
    pub fn validate_status(&self, status: &str) -> Result<TaskStatus, String> {
        TaskStatus::parse_with_config(status, self.config)
    }

    /// Validate task type against project configuration (case-insensitive, returns canonical form)
    pub fn validate_task_type(&self, task_type: &str) -> Result<TaskType, String> {
        TaskType::parse_with_config(task_type, self.config)
    }

    /// Validate priority against project configuration (case-insensitive, returns canonical form)
    pub fn validate_priority(&self, priority: &str) -> Result<Priority, String> {
        Priority::parse_with_config(priority, self.config)
    }

    /// Validate category against project configuration
    pub fn validate_category(&self, category: &str) -> Result<String, String> {
        if self.config.categories.has_wildcard() {
            // Any category is allowed
            Ok(category.to_string())
        } else if self
            .config
            .categories
            .values
            .contains(&category.to_string())
        {
            Ok(category.to_string())
        } else {
            let suggestion = find_closest_match(category, &self.config.categories.values);
            let suggestion_text = match suggestion {
                Some(s) => format!(" Did you mean '{}'?", s),
                None => String::new(),
            };

            Err(format!(
                "Category '{}' is not allowed in this project. Valid categories: {}.{}",
                category,
                self.config.categories.values.join(", "),
                suggestion_text
            ))
        }
    }

    /// Validate tag against project configuration
    pub fn validate_tag(&self, tag: &str) -> Result<String, String> {
        if self.config.tags.has_wildcard() {
            // Any tag is allowed
            Ok(tag.to_string())
        } else if self.config.tags.values.contains(&tag.to_string()) {
            Ok(tag.to_string())
        } else {
            let suggestion = find_closest_match(tag, &self.config.tags.values);
            let suggestion_text = match suggestion {
                Some(s) => format!(" Did you mean '{}'?", s),
                None => String::new(),
            };

            Err(format!(
                "Tag '{}' is not allowed in this project. Valid tags: {}.{}",
                tag,
                self.config.tags.values.join(", "),
                suggestion_text
            ))
        }
    }

    /// Validate custom field name against project configuration
    pub fn validate_custom_field_name(&self, field_name: &str) -> Result<String, String> {
        if self.config.custom_fields.has_wildcard() {
            // Any custom field is allowed
            Ok(field_name.to_string())
        } else if self
            .config
            .custom_fields
            .values
            .contains(&field_name.to_string())
        {
            Ok(field_name.to_string())
        } else {
            let suggestion = find_closest_match(field_name, &self.config.custom_fields.values);
            let suggestion_text = match suggestion {
                Some(s) => format!(" Did you mean '{}'?", s),
                None => String::new(),
            };

            Err(format!(
                "Custom field '{}' is not allowed in this project. Valid custom fields: {}.{}",
                field_name,
                self.config.custom_fields.values.join(", "),
                suggestion_text
            ))
        }
    }

    /// Validate custom field key-value pair
    pub fn validate_custom_field(
        &self,
        field_name: &str,
        field_value: &str,
    ) -> Result<(String, String), String> {
        // First validate the field name
        let validated_name = self.validate_custom_field_name(field_name)?;

        // For now, allow any value for custom fields
        // In the future, this could be extended to validate values based on field type
        Ok((validated_name, field_value.to_string()))
    }

    /// Validate assignee format (basic email validation)
    pub fn validate_assignee(&self, assignee: &str) -> Result<String, String> {
        // Handle special cases
        if assignee == "@me" {
            // Will be resolved to actual user later
            return Ok(assignee.to_string());
        }

        if let Some(username) = assignee.strip_prefix('@') {
            // Username format - validate it's a reasonable username
            if !username.is_empty()
                && username
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                Ok(assignee.to_string())
            } else {
                Err("Invalid username format. Usernames can only contain letters, numbers, underscore, and dash.".to_string())
            }
        } else if assignee.contains('@') {
            // Email format - basic validation
            if assignee.matches('@').count() == 1
                && assignee.contains('.')
                && !assignee.starts_with('@')
                && !assignee.ends_with('@')
            {
                Ok(assignee.to_string())
            } else {
                Err("Invalid email format".to_string())
            }
        } else {
            Err("Assignee must be an email address or username starting with @".to_string())
        }
    }

    /// Parse and validate due date (supports relative dates)
    pub fn parse_due_date(&self, due_date: &str) -> Result<String, String> {
        match due_date.to_lowercase().as_str() {
            "today" => {
                let today = chrono::Local::now().date_naive();
                Ok(today.format("%Y-%m-%d").to_string())
            }
            "tomorrow" => {
                let tomorrow = chrono::Local::now().date_naive() + chrono::Duration::days(1);
                Ok(tomorrow.format("%Y-%m-%d").to_string())
            }
            "next week" | "nextweek" => {
                let next_week = chrono::Local::now().date_naive() + chrono::Duration::weeks(1);
                Ok(next_week.format("%Y-%m-%d").to_string())
            }
            _ => {
                // Try to parse as YYYY-MM-DD
                if let Ok(parsed) = chrono::NaiveDate::parse_from_str(due_date, "%Y-%m-%d") {
                    Ok(parsed.format("%Y-%m-%d").to_string())
                } else {
                    Err(format!(
                        "Invalid date format: '{}'. Use YYYY-MM-DD or relative terms like 'today', 'tomorrow', 'next week'",
                        due_date
                    ))
                }
            }
        }
    }

    /// Validate effort estimate format
    pub fn validate_effort(&self, effort: &str) -> Result<String, String> {
        // Support formats like: 2h, 3d, 1w, 0.5d, etc.
        let effort_lower = effort.to_lowercase();

        if effort_lower.ends_with('h') || effort_lower.ends_with('d') || effort_lower.ends_with('w')
        {
            let number_part = &effort_lower[..effort_lower.len() - 1];
            if number_part.parse::<f64>().is_ok() {
                Ok(effort.to_string())
            } else {
                Err("Invalid effort format. Use number followed by h (hours), d (days), or w (weeks). Example: 2h, 1.5d, 1w".to_string())
            }
        } else {
            Err("Invalid effort format. Use number followed by h (hours), d (days), or w (weeks). Example: 2h, 1.5d, 1w".to_string())
        }
    }
}

/// Find the closest match for a string in a list (simple edit distance)
fn find_closest_match(input: &str, candidates: &[String]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }

    let input_lower = input.to_lowercase();
    let mut best_match = None;
    let mut best_distance = usize::MAX;

    for candidate in candidates {
        let candidate_lower = candidate.to_lowercase();
        let distance = edit_distance(&input_lower, &candidate_lower);

        // Only suggest if the edit distance is reasonable (less than half the input length)
        if distance < input.len() / 2 + 1 && distance < best_distance {
            best_distance = distance;
            best_match = Some(candidate.clone());
        }
    }

    best_match
}

/// Simple edit distance calculation (Levenshtein distance)
fn edit_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    // Initialize first row and column
    #[allow(clippy::needless_range_loop)]
    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }

    // Fill the matrix
    for (i, c1) in s1.chars().enumerate() {
        for (j, c2) in s2.chars().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[len1][len2]
}

// inline tests moved to tests/cli_validation_unit_test.rs
