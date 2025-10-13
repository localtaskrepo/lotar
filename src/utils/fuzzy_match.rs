fn norm(s: &str) -> String {
    s.to_lowercase().replace(['_', '-'], "")
}

/// Utility for fuzzy property matching: ignores case, underscores, and dashes.
/// Use for property comparisons (e.g., assignee, tags, status, etc.)
pub fn fuzzy_match(a: &str, b: &str) -> bool {
    norm(a) == norm(b)
}

/// Partial fuzzy containment: returns true if `needle` appears within `haystack`
/// after case-folding and stripping separators.
pub fn fuzzy_contains(haystack: &str, needle: &str) -> bool {
    let h = norm(haystack);
    let n = norm(needle);
    !n.is_empty() && h.contains(&n)
}

/// Utility for fuzzy set matching: returns true if any value in `values` matches any in `allowed`.
pub fn fuzzy_set_match(values: &[String], allowed: &[String]) -> bool {
    for v in values {
        for a in allowed {
            if fuzzy_match(v, a) {
                return true;
            }
        }
    }
    false
}
