//! MCP wire-session lifecycle state (DEV-62).
//!
//! A wire session owned by [`super::run_stdio_server`] moves through
//! `Uninitialized -> AwaitInitialized -> Ready`:
//!
//! - `initialize` (request) transitions `Uninitialized -> AwaitInitialized`.
//! - `notifications/initialized` (notification) transitions
//!   `AwaitInitialized -> Ready`; it is ignored in every other state so an
//!   out-of-order `initialized` can never grant readiness.
//! - A second `initialize` never resets a live session.
//!
//! The session also coalesces deferred `notifications/tools/list_changed`
//! emissions: changes observed before `Ready` set a single pending flag and
//! are emitted exactly once right after the ready transition instead of being
//! dropped.

use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionState {
    Uninitialized,
    AwaitInitialized,
    Ready,
}

#[derive(Debug)]
struct SessionInner {
    state: SessionState,
    pending_list_changed: bool,
}

/// Lifecycle state for one MCP wire session.
///
/// Each `run_stdio_server` invocation owns exactly one instance; there is no
/// process-global readiness. The stateless in-process dispatch helper
/// (`handle_json_line`) never touches a session.
#[derive(Debug)]
pub(crate) struct McpSession {
    inner: Mutex<SessionInner>,
}

impl Default for McpSession {
    fn default() -> Self {
        Self::new()
    }
}

impl McpSession {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(SessionInner {
                state: SessionState::Uninitialized,
                pending_list_changed: false,
            }),
        }
    }

    /// Handle an `initialize` request. Returns `true` only when this call
    /// transitioned `Uninitialized -> AwaitInitialized`. Repeated `initialize`
    /// on a live session never moves the state backwards.
    pub(crate) fn begin_initialize(&self) -> bool {
        let mut inner = lock_inner(&self.inner);
        if inner.state == SessionState::Uninitialized {
            inner.state = SessionState::AwaitInitialized;
            true
        } else {
            false
        }
    }

    /// Handle a `notifications/initialized` notification. Returns `true` only
    /// when this call transitioned `AwaitInitialized -> Ready`; the
    /// notification is ignored in the other states.
    pub(crate) fn complete_initialize(&self) -> bool {
        let mut inner = lock_inner(&self.inner);
        if inner.state == SessionState::AwaitInitialized {
            inner.state = SessionState::Ready;
            true
        } else {
            false
        }
    }

    pub(crate) fn state(&self) -> SessionState {
        lock_inner(&self.inner).state
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.state() == SessionState::Ready
    }

    /// Record a tools-list change. Returns `true` when the change was
    /// deferred (session not ready yet); the caller should emit immediately
    /// otherwise. Deferred changes coalesce into a single pending flag.
    pub(crate) fn defer_list_changed_if_not_ready(&self) -> bool {
        let mut inner = lock_inner(&self.inner);
        if inner.state == SessionState::Ready {
            false
        } else {
            inner.pending_list_changed = true;
            true
        }
    }

    /// Take the coalesced deferred tools-list change, valid only once the
    /// session is ready. Returns `true` exactly once per deferral window.
    pub(crate) fn take_deferred_list_changed(&self) -> bool {
        let mut inner = lock_inner(&self.inner);
        if inner.state == SessionState::Ready && inner.pending_list_changed {
            inner.pending_list_changed = false;
            true
        } else {
            false
        }
    }
}

fn lock_inner(inner: &Mutex<SessionInner>) -> std::sync::MutexGuard<'_, SessionInner> {
    inner
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_starts_uninitialized_and_reaches_ready_in_order() {
        let session = McpSession::new();
        assert_eq!(session.state(), SessionState::Uninitialized);

        assert!(session.begin_initialize());
        assert_eq!(session.state(), SessionState::AwaitInitialized);
        assert!(!session.is_ready());

        assert!(session.complete_initialize());
        assert_eq!(session.state(), SessionState::Ready);
        assert!(session.is_ready());
    }

    #[test]
    fn out_of_order_initialized_never_grants_readiness() {
        let session = McpSession::new();
        // initialized before any initialize request is ignored.
        assert!(!session.complete_initialize());
        assert_eq!(session.state(), SessionState::Uninitialized);

        assert!(session.begin_initialize());
        assert!(!session.is_ready());
        assert!(session.complete_initialize());
        // Repeated initialized notifications are ignored once ready.
        assert!(!session.complete_initialize());
        assert!(session.is_ready());
    }

    #[test]
    fn second_initialize_does_not_reset_live_session() {
        let session = McpSession::new();
        assert!(session.begin_initialize());
        assert!(session.complete_initialize());
        // A second initialize must not move Ready backwards.
        assert!(!session.begin_initialize());
        assert_eq!(session.state(), SessionState::Ready);
        assert!(!session.begin_initialize());
        assert!(session.is_ready());
    }

    #[test]
    fn pre_initialize_session_also_defers_until_ready() {
        let session = McpSession::new();
        // Change observed while fully uninitialized is deferred, not emitted.
        assert!(session.defer_list_changed_if_not_ready());
        assert!(session.defer_list_changed_if_not_ready());
        // Not ready yet: nothing to take.
        assert!(!session.take_deferred_list_changed());

        assert!(session.begin_initialize());
        // Still awaiting notifications/initialized.
        assert!(!session.take_deferred_list_changed());
        assert!(session.complete_initialize());

        // Coalesced to exactly one emission.
        assert!(session.take_deferred_list_changed());
        assert!(!session.take_deferred_list_changed());
    }

    #[test]
    fn ready_session_never_defers() {
        let session = McpSession::new();
        assert!(session.begin_initialize());
        assert!(session.complete_initialize());
        assert!(!session.defer_list_changed_if_not_ready());
        assert!(!session.take_deferred_list_changed());
    }
}
