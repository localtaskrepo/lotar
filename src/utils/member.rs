//! Member value normalization utilities.
//!
//! The `@` prefix is reserved for **directives** — special assignments that
//! trigger operational behavior:
//! - `@me`              → resolves to current user identity
//! - `@<agent-profile>` → queues an agent job
//! - Future: `@least`, `@random`, etc.
//!
//! Normal usernames (`alice`, `john.doe`) and emails (`user@example.com`)
//! are stored without a leading `@`.  For backward compatibility, an input
//! like `@alice` (where `alice` is *not* a directive) is silently normalized
//! to `alice`.

/// Built-in directives that are always preserved with their `@` prefix.
const BUILTIN_DIRECTIVES: &[&str] = &["@me"];

/// Returns `true` when `value` is a recognised built-in directive.
pub fn is_builtin_directive(value: &str) -> bool {
    BUILTIN_DIRECTIVES.contains(&value)
}

/// Normalize a member value (assignee / reporter) for **storage**.
///
/// * Built-in directives (`@me`) → returned as-is (resolved elsewhere).
/// * Agent profile names (`@copilot`) → returned as-is (directive).
/// * Any other `@name` → the `@` is stripped (backward-compat normalization).
/// * Bare usernames, emails → returned as-is.
pub fn normalize_member_value(value: &str, is_agent_profile: impl Fn(&str) -> bool) -> String {
    let trimmed = value.trim();

    // Not @-prefixed → nothing to normalize.
    if !trimmed.starts_with('@') {
        return trimmed.to_string();
    }

    // Built-in directives are always kept.
    if is_builtin_directive(trimmed) {
        return trimmed.to_string();
    }

    let name = &trimmed[1..];

    // Agent profile names keep their `@` prefix (they are directives).
    if !name.is_empty() && is_agent_profile(name) {
        return trimmed.to_string();
    }

    // Everything else: strip the `@`.
    if name.is_empty() {
        trimmed.to_string()
    } else {
        name.to_string()
    }
}

/// Strip the `@` prefix (if any) and lowercase for **comparison** purposes.
///
/// This allows matching stored `@alice` (old data) against `alice` (new data)
/// and vice-versa, as well as case-insensitive member list checks.
pub fn member_for_comparison(value: &str) -> String {
    value.trim().trim_start_matches('@').to_ascii_lowercase()
}

/// Returns `true` when `c` is valid in a bare username
/// (letters, digits, underscore, dash, period).
pub fn is_username_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// Returns `true` when `s` looks like a valid bare username.
pub fn is_valid_username(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(is_username_char)
        && s.chars().next().is_some_and(|c| c.is_alphanumeric())
        && s.chars().last().is_some_and(|c| c.is_alphanumeric())
}

/// Returns `true` when `s` looks like an email address (single `@` with a dot).
pub fn is_email_like(s: &str) -> bool {
    s.contains('@')
        && s.matches('@').count() == 1
        && s.contains('.')
        && !s.starts_with('@')
        && !s.ends_with('@')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_directives() {
        assert!(is_builtin_directive("@me"));
        assert!(!is_builtin_directive("@alice"));
        assert!(!is_builtin_directive("me"));
    }

    #[test]
    fn normalize_strips_at_for_normal_names() {
        let no_agent = |_: &str| false;
        assert_eq!(normalize_member_value("@alice", no_agent), "alice");
        assert_eq!(normalize_member_value("@John-Doe", no_agent), "John-Doe");
        assert_eq!(normalize_member_value("@john.doe", no_agent), "john.doe");
    }

    #[test]
    fn normalize_keeps_bare_names() {
        let no_agent = |_: &str| false;
        assert_eq!(normalize_member_value("alice", no_agent), "alice");
        assert_eq!(normalize_member_value("john.doe", no_agent), "john.doe");
        assert_eq!(
            normalize_member_value("user@example.com", no_agent),
            "user@example.com"
        );
    }

    #[test]
    fn normalize_preserves_directives() {
        let is_agent = |name: &str| name == "copilot";
        assert_eq!(normalize_member_value("@me", is_agent), "@me");
        assert_eq!(normalize_member_value("@copilot", is_agent), "@copilot");
    }

    #[test]
    fn normalize_strips_at_for_non_agent() {
        let is_agent = |name: &str| name == "copilot";
        assert_eq!(normalize_member_value("@alice", is_agent), "alice");
    }

    #[test]
    fn comparison_normalizes() {
        assert_eq!(member_for_comparison("@Alice"), "alice");
        assert_eq!(member_for_comparison("alice"), "alice");
        assert_eq!(member_for_comparison("  @BOB  "), "bob");
        assert_eq!(
            member_for_comparison("user@example.com"),
            "user@example.com"
        );
    }

    #[test]
    fn username_validation() {
        assert!(is_valid_username("alice"));
        assert!(is_valid_username("john.doe"));
        assert!(is_valid_username("a-b_c"));
        assert!(is_valid_username("A1"));
        assert!(!is_valid_username(""));
        assert!(!is_valid_username(".alice"));
        assert!(!is_valid_username("alice."));
        assert!(!is_valid_username("-bad"));
        assert!(!is_valid_username("has space"));
    }

    #[test]
    fn email_detection() {
        assert!(is_email_like("user@example.com"));
        assert!(!is_email_like("@alice"));
        assert!(!is_email_like("alice"));
        assert!(!is_email_like("john@"));
        assert!(!is_email_like("@"));
    }
}
