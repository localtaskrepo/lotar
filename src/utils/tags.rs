/// Normalize a collection of tags by trimming whitespace and removing empty entries.
///
/// The iterator order is preserved for retained tags. Tags that become empty after trimming
/// are discarded. This helper deliberately keeps duplicate tags to avoid unexpected
/// deduplication side effects for callers who rely on ordering or counts upstream.
pub fn normalize_tags(tags: impl IntoIterator<Item = String>) -> Vec<String> {
    tags.into_iter()
        .filter_map(|tag| {
            let trimmed = tag.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}
