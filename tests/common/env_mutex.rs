use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::{env, ffi::OsString};

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

/// RAII guard for temporarily setting an environment variable in a serialized fashion.
/// Acquires a per-variable mutex, sets the variable, and restores the previous value on drop.
#[allow(dead_code)]
pub struct EnvVarGuard {
    var: String,
    prev: Option<OsString>,
    _guard: MutexGuard<'static, ()>,
}

#[allow(dead_code)]
impl EnvVarGuard {
    /// Temporarily set an env var. The previous value will be restored when the guard is dropped.
    pub fn set(var: &str, value: &str) -> Self {
        let guard = lock_var(var);
        let prev = env::var_os(var);
        unsafe {
            env::set_var(var, value);
        }
        Self {
            var: var.to_string(),
            prev,
            _guard: guard,
        }
    }

    /// Temporarily clear/unset an env var. The previous value will be restored when the guard is dropped.
    /// Holds the per-var mutex for the lifetime of the guard to prevent concurrent mutation
    /// while the variable is expected to be absent.
    pub fn clear(var: &str) -> Self {
        let guard = lock_var(var);
        let prev = env::var_os(var);
        unsafe {
            env::remove_var(var);
        }
        Self {
            var: var.to_string(),
            prev,
            _guard: guard,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.prev.as_ref() {
            Some(v) => unsafe { env::set_var(&self.var, v) },
            None => unsafe { env::remove_var(&self.var) },
        }
        // _guard drops last, releasing the per-var mutex
    }
}
