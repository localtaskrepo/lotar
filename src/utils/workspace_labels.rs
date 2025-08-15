use std::path::Path;

/// Derive a simple label from a monorepo-like folder structure.
/// Heuristics:
/// - If path contains segments like packages/<name> or apps/<name>, return <name>
/// - Else if path includes examples/<name> or services/<name>, return <name>
/// - Else return the leaf directory name
pub fn derive_path_label_from(dir: &Path) -> Option<String> {
    let segs: Vec<String> = dir
        .components()
        .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
        .collect();
    if segs.is_empty() {
        return None;
    }
    // Normalize to unix-style semantics by lowercasing keys we match on
    let lower: Vec<String> = segs.iter().map(|s| s.to_lowercase()).collect();
    let candidate_keys = ["packages", "apps", "examples", "services", "libs"];
    for key in candidate_keys.iter() {
        if let Some(idx) = lower.iter().position(|s| s == key) {
            if idx + 1 < segs.len() {
                let name = &segs[idx + 1];
                if !name.trim().is_empty() && !name.starts_with('.') {
                    return Some(name.trim_matches('@').to_string());
                }
            }
        }
    }
    None
}

/// Convenience derived label from current working directory
pub fn derive_label_from_cwd() -> Option<String> {
    std::env::current_dir()
        .ok()
        .and_then(|p| derive_path_label_from(&p))
}
