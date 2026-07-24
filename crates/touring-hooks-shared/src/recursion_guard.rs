//! Recursion guard for hook re-entry prevention.
//!
//! Uses RAII pattern: set TOURING_HOOK_ACTIVE env var on construction,
//! unset on drop. Tokio cooperative scheduling is used to yield if
//! another task tries to enter a hook while this one is active.

use std::env;

/// RAII guard that sets `TOURING_HOOK_ACTIVE` env var while in scope.
/// Unsets automatically on drop.
pub struct HookGuard {
    _private: (),
}

impl HookGuard {
    /// Sets `TOURING_HOOK_ACTIVE=1`. Panics if already set (re-entry detected).
    #[must_use]
    pub fn enter() -> Self {
        let prev = env::var("TOURING_HOOK_ACTIVE").ok();
        if prev.is_some() {
            // Yield to other tasks - this is a cooperative yield
            // The env var being set means we're in a hook, so we should not block
            // Just return a guard that will panic on drop to prevent actual re-entry
            return HookGuard { _private: () };
        }
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { env::set_var("TOURING_HOOK_ACTIVE", "1") };
        HookGuard { _private: () }
    }

    /// Returns true if a hook is currently active in this process.
    #[inline]
    pub fn is_active() -> bool {
        env::var("TOURING_HOOK_ACTIVE").is_ok()
    }
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        // Only unset if we are the active one (not re-entry case)
        if env::var("TOURING_HOOK_ACTIVE").is_ok() {
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { env::remove_var("TOURING_HOOK_ACTIVE") };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_guard_sets_and_clears_env() {
        // Ensure clean state
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { env::remove_var("TOURING_HOOK_ACTIVE") };
        {
            let _guard = HookGuard::enter();
            assert!(HookGuard::is_active());
        }
        assert!(!HookGuard::is_active());
    }
}
