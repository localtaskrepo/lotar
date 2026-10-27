//! Small in-memory caches for the web server (attachment thumbnails, the
//! hash index, attachment info). Nothing here is ever written to disk.
//!
//! Each cache has a byte budget (least recently used entries go first when it
//! is full) and an idle limit: a background sweeper drops entries nobody has
//! touched for that long, so a server left running for weeks gives the memory
//! back.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

/// How often the background sweeper looks for idle entries.
const SWEEP_INTERVAL: Duration = Duration::from_secs(5 * 60);

struct Entry<V> {
    value: Arc<V>,
    bytes: usize,
    touched: Instant,
}

struct Inner<K, V> {
    entries: HashMap<K, Entry<V>>,
    bytes: usize,
}

pub struct MemoryCache<K, V> {
    inner: Mutex<Inner<K, V>>,
    max_bytes: usize,
    idle: Duration,
}

impl<K: Eq + Hash + Clone, V> MemoryCache<K, V> {
    pub fn new(max_bytes: usize, idle: Duration) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                bytes: 0,
            }),
            max_bytes,
            idle,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner<K, V>> {
        // A panic while holding the lock leaves plain data behind; keep going.
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The cached value, marking it as recently used.
    pub fn get(&self, key: &K) -> Option<Arc<V>> {
        let mut inner = self.lock();
        let entry = inner.entries.get_mut(key)?;
        entry.touched = Instant::now();
        Some(entry.value.clone())
    }

    /// Cache `value` (costing `bytes`), evicting the least recently used
    /// entries to stay within budget. Values larger than the whole budget are
    /// returned without being kept.
    pub fn insert(&self, key: K, value: V, bytes: usize) -> Arc<V> {
        let value = Arc::new(value);
        if bytes > self.max_bytes {
            return value;
        }
        let mut inner = self.lock();
        if let Some(old) = inner.entries.remove(&key) {
            inner.bytes -= old.bytes;
        }
        while inner.bytes + bytes > self.max_bytes {
            let Some(oldest) = inner
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(evicted) = inner.entries.remove(&oldest) {
                inner.bytes -= evicted.bytes;
            }
        }
        inner.bytes += bytes;
        inner.entries.insert(
            key,
            Entry {
                value: value.clone(),
                bytes,
                touched: Instant::now(),
            },
        );
        value
    }

    pub fn remove(&self, key: &K) {
        let mut inner = self.lock();
        if let Some(old) = inner.entries.remove(key) {
            inner.bytes -= old.bytes;
        }
    }

    /// Drop entries not touched within the idle limit, as of `now`.
    pub fn sweep_at(&self, now: Instant) -> usize {
        let mut inner = self.lock();
        let before = inner.entries.len();
        let idle = self.idle;
        let mut freed = 0;
        inner.entries.retain(|_, entry| {
            let keep = now.saturating_duration_since(entry.touched) < idle;
            if !keep {
                freed += entry.bytes;
            }
            keep
        });
        inner.bytes -= freed;
        before - inner.entries.len()
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn total_bytes(&self) -> usize {
        self.lock().bytes
    }
}

/// Something the background sweeper can clean.
pub trait Sweep: Send + Sync {
    fn sweep_idle(&self, now: Instant);
}

impl<K: Eq + Hash + Clone + Send, V: Send + Sync> Sweep for MemoryCache<K, V> {
    fn sweep_idle(&self, now: Instant) {
        self.sweep_at(now);
    }
}

fn registry() -> &'static Mutex<Vec<Weak<dyn Sweep>>> {
    static REGISTRY: OnceLock<Mutex<Vec<Weak<dyn Sweep>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

/// Have the background sweeper clean `cache` every few minutes. The sweeper
/// thread starts with the first registered cache and holds only weak
/// references, so dropped caches simply fall out of the list.
pub fn register_for_sweeping(cache: &Arc<dyn Sweep>) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if let Ok(mut caches) = registry().lock() {
        caches.push(Arc::downgrade(cache));
    }
    STARTED.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("lotar-cache-sweeper".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(SWEEP_INTERVAL);
                    sweep_registered(Instant::now());
                }
            });
    });
}

/// Sweep every registered cache once (the sweeper thread's body).
pub fn sweep_registered(now: Instant) {
    let live: Vec<Arc<dyn Sweep>> = match registry().lock() {
        Ok(mut caches) => {
            caches.retain(|weak| weak.strong_count() > 0);
            caches.iter().filter_map(Weak::upgrade).collect()
        }
        Err(_) => return,
    };
    for cache in live {
        cache.sweep_idle(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_least_recently_used_to_stay_in_budget() {
        let cache = MemoryCache::new(10, Duration::from_secs(60));
        cache.insert("a", 1, 4);
        cache.insert("b", 2, 4);
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(cache.get(&"a").as_deref(), Some(&1)); // a is now newer than b
        cache.insert("c", 3, 4);
        assert!(cache.get(&"b").is_none(), "b was least recently used");
        assert!(cache.get(&"a").is_some() && cache.get(&"c").is_some());
        assert_eq!(cache.total_bytes(), 8);
    }

    #[test]
    fn oversized_values_are_not_kept() {
        let cache = MemoryCache::new(10, Duration::from_secs(60));
        assert_eq!(*cache.insert("big", 7, 11), 7);
        assert!(cache.is_empty());
    }

    #[test]
    fn replacing_a_key_keeps_the_byte_count_right() {
        let cache = MemoryCache::new(10, Duration::from_secs(60));
        cache.insert("a", 1, 6);
        cache.insert("a", 2, 3);
        assert_eq!(cache.total_bytes(), 3);
        assert_eq!(cache.get(&"a").as_deref(), Some(&2));
        cache.remove(&"a");
        assert_eq!(cache.total_bytes(), 0);
    }

    #[test]
    fn sweeping_drops_only_idle_entries() {
        let cache = MemoryCache::new(100, Duration::from_secs(3600));
        cache.insert("old", 1, 5);
        std::thread::sleep(Duration::from_millis(10));
        let between = Instant::now();
        std::thread::sleep(Duration::from_millis(10));
        cache.insert("fresh", 2, 5);
        assert_eq!(cache.sweep_at(Instant::now()), 0);
        // An hour after `between`, "old" has been idle too long, "fresh" not.
        assert_eq!(cache.sweep_at(between + Duration::from_secs(3600)), 1);
        assert!(cache.get(&"fresh").is_some());
        assert_eq!(cache.total_bytes(), 5);
    }

    #[test]
    fn registered_caches_are_swept_and_dropped_ones_forgotten() {
        let cache: Arc<MemoryCache<&str, i32>> =
            Arc::new(MemoryCache::new(100, Duration::from_secs(1)));
        cache.insert("a", 1, 1);
        let as_sweep: Arc<dyn Sweep> = cache.clone();
        register_for_sweeping(&as_sweep);
        sweep_registered(Instant::now() + Duration::from_secs(2));
        assert!(cache.is_empty());
        drop(as_sweep);
        drop(cache);
        sweep_registered(Instant::now());
    }
}
