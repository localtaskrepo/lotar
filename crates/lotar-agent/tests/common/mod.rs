// Shared environment mutex for tests that mutate global env vars.
#[path = "../../../lotar-core/tests/common/env_mutex.rs"]
pub mod env_mutex;

// The task services reach automation only once the hooks are installed.
#[ctor::ctor(unsafe)]
fn install_hooks_for_tests() {
    lotar_agent::install_hooks();
}
