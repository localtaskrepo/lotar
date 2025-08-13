use once_cell::sync::Lazy;
use std::sync::Mutex;

// Global mutex for tests that mutate LOTAR_TASKS_DIR or other global env
pub static ENV_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
