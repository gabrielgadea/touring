use std::cell::Cell;
use std::rc::Rc;

use super::{install, is_installed, yield_now};
use crate::HookRuntime;

fn runtime() -> (tempfile::TempDir, HookRuntime) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let rt = HookRuntime::new(tmp.path()).expect("runtime");
    (tmp, rt)
}

#[test]
fn without_an_installed_function_yield_is_a_no_op() {
    let (_tmp, mut rt) = runtime();
    assert!(!is_installed());
    assert!(!yield_now(&mut rt));
}

#[test]
fn an_installed_function_runs_on_every_yield_until_the_guard_drops() {
    let (_tmp, mut rt) = runtime();
    let calls = Rc::new(Cell::new(0u32));
    let seen = Rc::clone(&calls);
    let guard = install(Box::new(move |_rt| seen.set(seen.get() + 1)));
    assert!(is_installed());
    assert!(yield_now(&mut rt));
    assert!(yield_now(&mut rt));
    assert_eq!(calls.get(), 2);
    drop(guard);
    assert!(!is_installed());
    assert!(!yield_now(&mut rt));
    assert_eq!(calls.get(), 2, "a dropped guard leaves nothing to call");
}

#[test]
fn a_handler_served_during_a_yield_never_yields_again() {
    let (_tmp, mut rt) = runtime();
    let inner = Rc::new(Cell::new(None));
    let record = Rc::clone(&inner);
    let _guard = install(Box::new(move |rt| {
        assert!(!is_installed(), "unavailable while it runs");
        record.set(Some(yield_now(rt)));
    }));
    assert!(yield_now(&mut rt));
    assert_eq!(
        inner.get(),
        Some(false),
        "the nested yield must not re-enter"
    );
    assert!(
        is_installed(),
        "the function is back in place after the yield"
    );
    assert!(yield_now(&mut rt), "and serves the next yield");
}

#[test]
fn a_panicking_heavy_hook_still_uninstalls_the_function() {
    let (_tmp, mut rt) = runtime();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = install(Box::new(|_rt| {}));
        yield_now(&mut rt);
        panic!("heavy hook failed");
    }));
    assert!(result.is_err());
    assert!(!is_installed());
}

/// Cross-audit 14/09/2026 (A9): the old version reinstalled a new function
/// before asserting, so it passed even if the unwind lost the installed one. The
/// SAME function must be back in the slot, with no reinstall.
#[test]
fn a_panic_inside_the_yield_function_puts_the_same_function_back() {
    let (_tmp, mut rt) = runtime();
    let calls = Rc::new(Cell::new(0u32));
    let seen = Rc::clone(&calls);
    let _guard = install(Box::new(move |_rt| {
        seen.set(seen.get() + 1);
        if seen.get() == 1 {
            panic!("served handler escaped");
        }
    }));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| yield_now(&mut rt)));
    assert!(result.is_err());
    assert!(
        is_installed(),
        "the unwind put the function back in its slot"
    );
    assert!(yield_now(&mut rt), "and it serves the next yield");
    assert_eq!(calls.get(), 2, "the same function ran both times");
}
