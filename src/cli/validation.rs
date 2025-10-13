use crate::config::types::ResolvedConfig;
use crate::types::{Priority, TaskStatus, TaskType};
use chrono::Datelike;

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
        let normalized = tag.trim();
        if normalized.is_empty() {
            return Err("Tag cannot be empty or whitespace".to_string());
        }

        if self.config.tags.has_wildcard() {
            // Any tag is allowed
            Ok(normalized.to_string())
        } else if self.config.tags.values.contains(&normalized.to_string()) {
            Ok(normalized.to_string())
        } else {
            let suggestion = find_closest_match(normalized, &self.config.tags.values);
            let suggestion_text = match suggestion {
                Some(s) => format!(" Did you mean '{}'?", s),
                None => String::new(),
            };

            Err(format!(
                "Tag '{}' is not allowed in this project. Valid tags: {}.{}",
                normalized,
                self.config.tags.values.join(", "),
                suggestion_text
            ))
        }
    }

    /// Validate custom field name against project configuration
    pub fn validate_custom_field_name(&self, field_name: &str) -> Result<String, String> {
        // M4: Collision guard - prevent using reserved built-in field names as custom fields
        if let Some(canonical) = crate::utils::fields::is_reserved_field(field_name) {
            return Err(format!(
                "Field name '{}' collides with built-in field '{}'. Use the built-in option instead (e.g., --{}), or pick a different custom field name.",
                field_name,
                canonical,
                canonical.replace('_', "-")
            ));
        }
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
            // Allow @me alias; will be resolved at use sites
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

    /// Parse and validate due date/time. Normalizes to RFC3339 (UTC) string.
    ///
    /// Supported:
    /// - Absolute date: YYYY-MM-DD (interpreted as local midnight, converted to UTC)
    /// - RFC3339 datetime: 2025-12-31T15:04:05Z or with offset
    /// - Local naive datetime: "YYYY-MM-DD HH:MM[:SS]" or "YYYY-MM-DDTHH:MM[:SS]" (assumed local tz)
    /// - Keywords: today, tomorrow, next week, next <weekday>
    /// - Shortcuts: in Nd/Nw, +Nd/+Nw, +Nbd (business days), next business day,
    ///   this/by <weekday>, <weekday>, next week <weekday>
    pub fn parse_due_date(&self, due_date: &str) -> Result<String, String> {
        use chrono::{Local, Utc};

        let s_raw = due_date.trim();
        let s = s_raw.to_lowercase();

        // Keywords (date-only -> local midnight implied, but we store YYYY-MM-DD)
        match s.as_str() {
            "today" => {
                let d = Local::now().date_naive();
                return Ok(d.format("%Y-%m-%d").to_string());
            }
            "tomorrow" => {
                let d = Local::now().date_naive() + chrono::Duration::days(1);
                return Ok(d.format("%Y-%m-%d").to_string());
            }
            "next week" | "nextweek" => {
                let d = Local::now().date_naive() + chrono::Duration::weeks(1);
                return Ok(d.format("%Y-%m-%d").to_string());
            }
            _ => {}
        }

        // Phrases like next monday, this friday, by friday, fri
        if let Some(next_day) = parse_weekday_phrases(&s) {
            return Ok(next_day.format("%Y-%m-%d").to_string());
        }

        // next week <weekday>
        if let Some(next_week_named) = parse_next_week_named(&s) {
            return Ok(next_week_named.format("%Y-%m-%d").to_string());
        }

        // Offsets: +Nd/+Nw and spaced, in Nd/Nw, business day variants
        if let Some(offset) = parse_simple_offset(&s) {
            let d = chrono::Local::now().date_naive() + offset;
            return Ok(d.format("%Y-%m-%d").to_string());
        }

        if let Some(offset) = parse_in_offset(&s) {
            let d = chrono::Local::now().date_naive() + offset;
            return Ok(d.format("%Y-%m-%d").to_string());
        }

        if let Some(days) = parse_business_days_offset(&s) {
            let base = chrono::Local::now().date_naive();
            let d = add_business_days(base, days);
            return Ok(d.format("%Y-%m-%d").to_string());
        }

        // RFC3339 datetime with timezone
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s_raw) {
            return Ok(dt.with_timezone(&Utc).to_rfc3339());
        }

        // Naive local datetime without timezone
        if let Some(dt_utc) = parse_local_naive_datetime_to_utc(s_raw) {
            return Ok(dt_utc.to_rfc3339());
        }

        // Absolute date YYYY-MM-DD (store as date-only)
        if let Ok(parsed) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
            return Ok(parsed.format("%Y-%m-%d").to_string());
        }

        Err(format!(
            "Invalid date format: '{}'. Try one of: YYYY-MM-DD, RFC3339 (2025-12-31T15:04:05Z), 'in 3 days', '+3d', '+2w', '+1bd', 'next business day', 'next monday', 'this friday', 'by fri', 'next week monday'",
            due_date
        ))
    }

    /// Validate effort estimate format
    pub fn validate_effort(&self, effort: &str) -> Result<String, String> {
        // Accept both time and points units as valid effort formats.
        let t = effort.trim().to_lowercase();
        match crate::utils::effort::parse_effort(&t) {
            Ok(_) => Ok(effort.to_string()),
            Err(_) => Err("Invalid effort format. Use a number followed by a valid unit (h, d, w, m, pt, points, etc.), e.g., 2h, 1.5d, 1w, 5pt, 3points".to_string()),
        }
    }
}

fn parse_weekday_name(name: &str) -> Option<chrono::Weekday> {
    let n = name.to_lowercase();
    match n.as_str() {
        "mon" | "monday" => Some(chrono::Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(chrono::Weekday::Tue),
        "wed" | "weds" | "wednesday" => Some(chrono::Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Some(chrono::Weekday::Thu),
        "fri" | "friday" => Some(chrono::Weekday::Fri),
        "sat" | "saturday" => Some(chrono::Weekday::Sat),
        "sun" | "sunday" => Some(chrono::Weekday::Sun),
        _ => None,
    }
}

/// Parse "+Nd" or "+Nw" (and variants like "+1 day", "+2 weeks") into a Duration.
fn parse_simple_offset(s: &str) -> Option<chrono::Duration> {
    let t = s.trim_start();
    if !t.starts_with('+') {
        return None;
    }
    let rest = &t[1..];
    // Try compact form: +10d, +2w
    if let Some(unit) = rest.chars().last() {
        if unit == 'd' || unit == 'w' {
            let num_part = &rest[..rest.len() - 1];
            if let Ok(n) = num_part.parse::<i64>() {
                return Some(if unit == 'd' {
                    chrono::Duration::days(n)
                } else {
                    chrono::Duration::weeks(n)
                });
            }
        }
    }
    // Try spaced form: +10 day(s), +2 week(s)
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() == 2 {
        if let Ok(n) = parts[0].parse::<i64>() {
            let unit = parts[1].to_lowercase();
            if unit.starts_with("day") {
                return Some(chrono::Duration::days(n));
            }
            if unit.starts_with("week") {
                return Some(chrono::Duration::weeks(n));
            }
        }
    }
    None
}

/// Parse "in Nd" or "in Nw" into a Duration.
fn parse_in_offset(s: &str) -> Option<chrono::Duration> {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("in ") {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() == 2 {
            if let Ok(n) = parts[0].parse::<i64>() {
                let unit = parts[1].to_lowercase();
                if unit.starts_with('d') || unit.starts_with("day") {
                    return Some(chrono::Duration::days(n));
                }
                if unit.starts_with('w') || unit.starts_with("week") {
                    return Some(chrono::Duration::weeks(n));
                }
            }
        }
    }
    None
}

/// Parse "+Nbd" or spaced form "+N business day(s)" into number of business days
fn parse_business_days_offset(s: &str) -> Option<i64> {
    let t = s.trim_start();
    if let Some(rest) = t.strip_prefix('+') {
        if let Some(rest2) = rest.strip_suffix("bd") {
            if let Ok(n) = rest2.parse::<i64>() {
                return Some(n);
            }
        }
        // spaced form: +N business day(s)
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() >= 2 {
            if let Ok(n) = parts[0].parse::<i64>() {
                let unit = parts[1].to_lowercase();
                if unit.starts_with("business") {
                    return Some(n);
                }
            }
        }
    }
    if s.eq_ignore_ascii_case("next business day") {
        return Some(1);
    }
    None
}

/// Add n business days (Mon-Fri) to a date
fn add_business_days(mut date: chrono::NaiveDate, mut days: i64) -> chrono::NaiveDate {
    while days > 0 {
        date += chrono::Duration::days(1);
        let wd = date.weekday();
        if wd != chrono::Weekday::Sat && wd != chrono::Weekday::Sun {
            days -= 1;
        }
    }
    date
}

/// Parse phrases like "next monday", "this friday", "by fri", or just "fri"
fn parse_weekday_phrases(s: &str) -> Option<chrono::NaiveDate> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("next ") {
        if let Some(wd) = parse_weekday_name(rest.trim()) {
            return Some(next_occurrence(wd));
        }
    }
    if let Some(rest) = s.strip_prefix("this ") {
        if let Some(wd) = parse_weekday_name(rest.trim()) {
            return Some(next_occurrence(wd));
        }
    }
    if let Some(rest) = s.strip_prefix("by ") {
        if let Some(wd) = parse_weekday_name(rest.trim()) {
            return Some(next_occurrence(wd));
        }
    }
    if let Some(wd) = parse_weekday_name(s) {
        return Some(next_occurrence(wd));
    }
    None
}

/// Next occurrence of weekday strictly in the future (today counts as +7)
fn next_occurrence(target: chrono::Weekday) -> chrono::NaiveDate {
    let today = chrono::Local::now().date_naive();
    let today_num = today.weekday().num_days_from_monday() as i64;
    let target_num = target.num_days_from_monday() as i64;
    let diff = (target_num - today_num).rem_euclid(7);
    let days_ahead = if diff == 0 { 7 } else { diff };
    today + chrono::Duration::days(days_ahead)
}

/// Parse "next week <weekday>"
fn parse_next_week_named(s: &str) -> Option<chrono::NaiveDate> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("next week ") {
        if let Some(wd) = parse_weekday_name(rest.trim()) {
            // Find next week's Monday
            let today = chrono::Local::now().date_naive();
            let mon_this_week =
                today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
            let mon_next_week = mon_this_week + chrono::Duration::weeks(1);
            let offset_days = wd.num_days_from_monday() as i64;
            return Some(mon_next_week + chrono::Duration::days(offset_days));
        }
    }
    None
}

/// Parse naive local datetime strings and convert to UTC
fn parse_local_naive_datetime_to_utc(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{Local, NaiveDateTime, TimeZone, Utc};
    let fmts = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ];
    for fmt in &fmts {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            if let Some(dt) = Local.from_local_datetime(&ndt).single() {
                return Some(dt.with_timezone(&Utc));
            }
        }
    }
    None
}

/// Parse stored due-date string into UTC instant (supports RFC3339 and YYYY-MM-DD)
pub fn parse_due_string_to_utc(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{Local, TimeZone, Utc};
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt_local = Local
            .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
            .single()?;
        return Some(dt_local.with_timezone(&Utc));
    }
    None
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
