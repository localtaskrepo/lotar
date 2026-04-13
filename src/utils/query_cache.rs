use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::api_types::TaskDTO;

/// Cached result of a fully-filtered, sorted task query.
struct CachedQuery {
    tasks: Vec<(String, TaskDTO)>,
    created: Instant,
}

static CACHE: LazyLock<Mutex<HashMap<u64, CachedQuery>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Default TTL for cached queries (30 seconds).
const DEFAULT_TTL: Duration = Duration::from_secs(30);

/// Maximum number of cached queries (LRU-style eviction when exceeded).
const MAX_ENTRIES: usize = 32;

/// Compute a hash key from a set of query parameters.
pub fn cache_key(params: &HashMap<String, String>) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Sort keys for deterministic hashing.
    let mut sorted: Vec<(&String, &String)> = params.iter().collect();
    sorted.sort_by_key(|(k, _)| *k);
    for (k, v) in sorted {
        // Skip offset — we want to cache the full sorted result set,
        // then slice by offset/limit when serving.
        if k == "offset" || k == "limit" || k == "page_size" || k == "per_page" {
            continue;
        }
        k.hash(&mut hasher);
        v.hash(&mut hasher);
    }
    hasher.finish()
}

/// Try to get a cached query result.  Returns `None` if expired or absent.
pub fn get(key: u64) -> Option<Vec<(String, TaskDTO)>> {
    let mut cache = CACHE.lock().ok()?;
    let entry = cache.get(&key)?;
    if entry.created.elapsed() > DEFAULT_TTL {
        cache.remove(&key);
        return None;
    }
    Some(entry.tasks.clone())
}

/// Store a query result in the cache.
pub fn put(key: u64, tasks: Vec<(String, TaskDTO)>) {
    let Ok(mut cache) = CACHE.lock() else { return };
    // Simple eviction: if too many entries, remove the oldest.
    if cache.len() >= MAX_ENTRIES {
        let oldest = cache.iter().min_by_key(|(_, v)| v.created).map(|(k, _)| *k);
        if let Some(k) = oldest {
            cache.remove(&k);
        }
    }
    cache.insert(
        key,
        CachedQuery {
            tasks,
            created: Instant::now(),
        },
    );
}

/// Invalidate ALL cached queries.  Called from filesystem watcher events
/// and task mutation endpoints so caches don't serve stale data.
pub fn invalidate_all() {
    if let Ok(mut cache) = CACHE.lock() {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::TaskDTO;

    fn make_params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn cache_key_ignores_pagination_params() {
        let a = make_params(&[
            ("project", "P"),
            ("status", "Todo"),
            ("offset", "0"),
            ("limit", "50"),
        ]);
        let b = make_params(&[
            ("project", "P"),
            ("status", "Todo"),
            ("offset", "200"),
            ("limit", "200"),
        ]);
        assert_eq!(cache_key(&a), cache_key(&b));
    }

    #[test]
    fn cache_key_differs_for_different_filters() {
        let a = make_params(&[("project", "P"), ("status", "Todo")]);
        let b = make_params(&[("project", "P"), ("status", "Done")]);
        assert_ne!(cache_key(&a), cache_key(&b));
    }

    #[test]
    fn cache_key_is_order_independent() {
        let a = make_params(&[("project", "P"), ("status", "Todo")]);
        let b = make_params(&[("status", "Todo"), ("project", "P")]);
        assert_eq!(cache_key(&a), cache_key(&b));
    }

    #[test]
    fn put_and_get_roundtrip() {
        let key = 0xDEAD;
        let tasks: Vec<(String, TaskDTO)> = vec![];
        put(key, tasks);
        let result = get(key);
        assert!(result.is_some());
        invalidate_all();
    }

    #[test]
    fn invalidate_all_clears_cache() {
        let key = 0xBEEF;
        put(key, vec![]);
        invalidate_all();
        assert!(get(key).is_none());
    }
}
