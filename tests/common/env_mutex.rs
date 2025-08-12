use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

// Legacy global mutex (kept for backward compatibility in case some tests still import it)
#[allow(dead_code)]
pub static ENV_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// Registry of per-environment-variable mutexes.
// We store leaked &'static Mutex<()> pointers so we can return 'static guards safely.
#[allow(dead_code)]
static ENV_LOCKS: Lazy<Mutex<HashMap<String, &'static Mutex<()>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[allow(dead_code)]
fn get_mutex_for(var: &str) -> &'static Mutex<()> {
    let mut map = ENV_LOCKS.lock().expect("ENV_LOCKS poisoned");
    if let Some(m) = map.get(var) {
        return m;
    }
    // Leak a new Mutex so its reference is 'static for the duration of the test process.
    let leaked: &'static Mutex<()> = Box::leak(Box::new(Mutex::new(())));
    map.insert(var.to_string(), leaked);
    leaked
}

/// Acquire a lock specific to the given environment variable name.
/// Use like:
/// let _guard = lock_var("LOTAR_TASKS_DIR");
/// ... mutate env var safely within this scope ...
#[allow(dead_code)]
pub fn lock_var(var: &str) -> MutexGuard<'static, ()> {
    get_mutex_for(var).lock().expect("env var mutex poisoned")
}
