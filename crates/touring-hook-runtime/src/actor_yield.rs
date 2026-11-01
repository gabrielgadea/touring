//! Cooperative yield point for the serialized project actor.
//!
//! The daemon runs one actor thread per project. It owns the `HookRuntime` and
//! executes commands in series, so a heavy hook held the thread for its whole
//! run: during a 15-minute `index rebuild` of the analise project, `memory store`
//! from another session waited in the queue until its 15 s budget expired
//! (14/09/2026). While a heavy hook runs, the actor installs a yield function;
//! a heavy hook that calls [`yield_now`] where it holds no open transaction lets
//! the function serve the light commands already queued.
//!
//! Installing is not yielding. Today only `index rebuild` has yield points
//! (between walked files, at the post-walk phase boundaries, and before each
//! file the sweep or the phantom-module purge retires). The other heavy hooks
//! (`ast blast`, `tantivy reindex`, `mutation-test`, …) get the function and
//! never call it, so a light command queued behind them waits for them to end.
//! A heavy hook that gains a yield point proves it with a test that counts the
//! yields, as `rebuild_yield_tests` does (cross-audit 14/09/2026, A5).
//!
//! One thread, one runtime: the function runs on the actor thread against the
//! same `&mut HookRuntime` the heavy hook passes in, so no second runtime and no
//! locking are needed.

use std::cell::RefCell;

use crate::HookRuntime;

/// What the actor installs: serve queued light commands against the runtime.
pub type YieldFn = Box<dyn FnMut(&mut HookRuntime)>;

thread_local! {
    /// The installed function. It is taken out while it runs, so a nested yield
    /// finds the slot empty instead of re-entering the queue it is being served
    /// from — the slot is the re-entrancy guard.
    static YIELD_FN: RefCell<Option<YieldFn>> = const { RefCell::new(None) };
}

/// Uninstalls the yield function when dropped, including while unwinding.
#[must_use = "the yield function is uninstalled when the guard drops"]
pub struct YieldGuard {
    // The function lives in a thread-local: the guard must drop on its thread.
    _not_send: std::marker::PhantomData<*const ()>,
}

impl Drop for YieldGuard {
    fn drop(&mut self) {
        YIELD_FN.with(|slot| slot.borrow_mut().take());
    }
}

/// Install `f` as this thread's yield function until the guard drops. A second
/// install replaces the first.
pub fn install(f: YieldFn) -> YieldGuard {
    YIELD_FN.with(|slot| *slot.borrow_mut() = Some(f));
    YieldGuard {
        _not_send: std::marker::PhantomData,
    }
}

/// Whether a yield function is installed and available on this thread: `false`
/// while the function itself runs, which is when a nested yield is refused.
pub fn is_installed() -> bool {
    YIELD_FN.with(|slot| slot.borrow().is_some())
}

/// Give queued light commands a turn; returns whether a yield function ran.
///
/// A no-op when nothing is installed, and when called from inside a yield: a
/// handler served during the yield never yields again. The caller must hold no
/// open transaction or lock a light handler could need — the handlers run here,
/// on this thread, against `rt`.
pub fn yield_now(rt: &mut HookRuntime) -> bool {
    let Some(f) = YIELD_FN.with(|slot| slot.borrow_mut().take()) else {
        return false;
    };
    let mut running = Running { f: Some(f) };
    if let Some(f) = running.f.as_mut() {
        f(rt);
    }
    true
}

/// Puts the function back when the yield ends, also while unwinding, so a panic
/// inside it cannot leave the thread unable to yield.
struct Running {
    f: Option<YieldFn>,
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(f) = self.f.take() {
            YIELD_FN.with(|slot| {
                let mut slot = slot.borrow_mut();
                if slot.is_none() {
                    *slot = Some(f);
                }
            });
        }
    }
}

#[cfg(test)]
#[path = "actor_yield_tests.rs"]
mod tests;
