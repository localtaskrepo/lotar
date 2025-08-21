//! Utilities for working with field names across built-in and custom fields.
//! Provides a collision guard between custom fields and reserved built-in names.

/// Return the canonical built-in field name if the provided name collides with a reserved field.
/// Matching is case-insensitive and ignores underscores and dashes for convenience.
pub fn is_reserved_field(name: &str) -> Option<&'static str> {
    let k = normalize(name);
    match k.as_str() {
        // Identity
        "id" => Some("id"),
        "title" => Some("title"),
        "subtitle" => Some("subtitle"),
        "description" => Some("description"),
        // Built-in properties
        "status" => Some("status"),
        "priority" => Some("priority"),
        "type" | "tasktype" => Some("type"),
        "assignee" => Some("assignee"),
        "reporter" => Some("reporter"),
        "category" => Some("category"),
        "project" => Some("project"),
        "tag" | "tags" => Some("tags"),
        "effort" => Some("effort"),
        // Dates
        "due" | "duedate" => Some("due_date"),
        "created" => Some("created"),
        "modified" => Some("modified"),
        // Structured fields
        "comments" => Some("comments"),
        "relationships" => Some("relationships"),
        // Internal/config namespaces
        "customfields" | "custom.fields" => Some("custom_fields"),
        _ => None,
    }
}

/// Normalize a field name for matching (lowercase, remove '_' and '-').
fn normalize(s: &str) -> String {
    s.to_lowercase().replace(['_', '-'], "")
}
