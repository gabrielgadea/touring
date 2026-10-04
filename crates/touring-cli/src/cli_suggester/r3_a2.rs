//! r3 shadow, A2 — the lifecycle of a route offer, OBSERVED (S-8, plan
//! `2026-10-02-jev-harness-r3`).
//!
//! Production consumes a [`super::RouteOffer`] only when `classify_route_outcome`
//! returns `Some` — a `None` leaves the offer in place — so "three Bash calls went by
//! with no verdict" and "a new offer replaced it" are invisible to the live path. This
//! module keeps its OWN pending record per actor (it never touches `route_offers()`),
//! accumulates up to [`K_WINDOW`] command heads, and writes ONE `a2_offer_closed` line
//! when the offer closes: `consumed` (production returned a verdict), `window` (K Bash
//! calls without one) or `replaced` (a new offer in the same actor).
//!
//! Observe-only by construction: the public callers compute production's decision
//! FIRST and hand it over by value; nothing here can alter it. With the flag OFF
//! (`r3_shadow_enabled`, default) both entry points return before allocating or
//! touching a file; an emission failure is swallowed and counted by the emitter.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;
use touring_hook_runtime::r3_shadow::{
    A2Closed, ClosedBy, K_WINDOW, a2_offer_closed_event, emit_r3_event, r3_shadow_enabled,
    truncate_text,
};

/// Open offers kept at once. Past this the OLDEST is dropped without a line — an offer
/// that never closes (session ended) has no `closed_by` to carry.
const MAX_PENDING: usize = 1024;

/// An offer waiting for its verdict.
struct Pending {
    /// Raw session id (hashed at emission, never stored in an event).
    session: String,
    mode: String,
    offered_secs: u64,
    /// The first Bash commands that followed, each already cut to the text cap.
    cmd_heads: Vec<String>,
}

impl Pending {
    fn closed_event(&self, closed_by: ClosedBy, baseline_value: Option<f64>) -> Value {
        a2_offer_closed_event(&A2Closed {
            session: &self.session,
            mode: &self.mode,
            offered_secs: self.offered_secs,
            closed_by,
            cmd_heads: &self.cmd_heads,
            baseline_value,
        })
    }
}

/// Pending offers by actor key. A struct (not a bare static) so a test can own one.
pub(super) struct Registry {
    map: Mutex<HashMap<String, Pending>>,
}

impl Registry {
    pub(super) fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }

    /// A new offer for `key`. A still-pending one closes as `replaced` — exactly one
    /// line for it — and the new one starts accumulating.
    pub(super) fn offer(
        &self,
        key: &str,
        session: &str,
        mode: &str,
        offered_secs: u64,
        sink: &mut dyn FnMut(&Value),
    ) {
        let (displaced, replaced) = {
            let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
            // At the cap, the OLDEST offer of another actor is displaced by this newer one.
            let displaced = if map.len() >= MAX_PENDING && !map.contains_key(key) {
                let oldest = map
                    .iter()
                    .min_by_key(|(_, p)| p.offered_secs)
                    .map(|(k, _)| k.clone());
                oldest.and_then(|k| map.remove(&k))
            } else {
                None
            };
            let replaced = map.insert(
                key.to_string(),
                Pending {
                    session: session.to_string(),
                    mode: mode.to_string(),
                    offered_secs,
                    cmd_heads: Vec::new(),
                },
            );
            (displaced, replaced)
        };
        // A displaced offer still leaves exactly ONE line (cross-audit 02/10/2026, finding 7): S3 counts lines.
        // It closes as `replaced` — a newer offer took its place in the registry — which keeps the closed schema.
        for prev in displaced.into_iter().chain(replaced) {
            sink(&prev.closed_event(ClosedBy::Replaced, None));
        }
    }

    /// A Bash call finished and production answered `verdict` (`None` = no verdict).
    ///
    /// Not a Bash call (empty `cmd`) ⇒ nothing to count. Otherwise the head joins the
    /// pending offer; a verdict closes it `consumed`, the K-th verdict-less call closes
    /// it `window`. The lock is released before the sink runs (it does file I/O).
    pub(super) fn outcome(
        &self,
        key: &str,
        cmd: &str,
        verdict: Option<f64>,
        sink: &mut dyn FnMut(&Value),
    ) {
        if cmd.trim().is_empty() {
            return;
        }
        let closed = {
            let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
            let Some(pending) = map.get_mut(key) else {
                return;
            };
            // The cap (S6) is held three times on purpose: cut here on arrival, cut again by
            // `a2_offer_closed_event`, and refused by `emit_r3_event_at` if a line still breaks it.
            // For a head cut here the later two are no-ops, so they do not mean the input can be longer.
            pending.cmd_heads.push(truncate_text(cmd));
            let closed_by = if verdict.is_some() {
                Some(ClosedBy::Consumed)
            } else if pending.cmd_heads.len() >= K_WINDOW {
                Some(ClosedBy::Window)
            } else {
                None
            };
            closed_by.and_then(|by| map.remove(key).map(|p| (p, by)))
        };
        if let Some((pending, by)) = closed {
            sink(&pending.closed_event(by, verdict));
        }
    }

    /// Is an offer pending for `key`? (tests; also the OFF-flag "nothing recorded" proof)
    #[cfg(test)]
    pub(super) fn is_pending(&self, key: &str) -> bool {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(key)
    }

    /// Offers held right now (tests: the cap).
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

/// Is an offer pending for this actor in the PROCESS registry? (tests)
#[cfg(test)]
pub(super) fn pending_for(project_root: &Path, actor: &str) -> bool {
    registry().is_pending(&actor_key(project_root, actor))
}

fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(Registry::new)
}

/// The same actor key production uses for its own offer cache (`project\u{2}actor`).
pub(super) fn actor_key(project_root: &Path, actor: &str) -> String {
    format!("{}\u{2}{actor}", project_root.display())
}

/// The session id alone: an actor key may carry `\u{2}agent:<id>` after it, and the
/// event names the SESSION (a subagent shares its parent's).
fn session_of(actor: &str) -> &str {
    actor.split('\u{2}').next().unwrap_or(actor)
}

fn emit(event: &Value) {
    #[cfg(test)]
    if test_sink::offer(event) {
        return;
    }
    // Fail-open: the emitter swallows and counts; its `bool` has no consumer here.
    let _ = emit_r3_event(event);
}

/// Hook for `record_route_offer`: a route was just offered to `actor`.
pub(super) fn on_offer(project_root: &Path, actor: &str, mode: &str, offered_secs: u64) {
    if !r3_shadow_enabled(project_root) {
        return;
    }
    let key = actor_key(project_root, actor);
    registry().offer(&key, session_of(actor), mode, offered_secs, &mut |e| {
        emit(e)
    });
}

/// Hook for `claim_route_reward`: a PostToolUse finished with production's `verdict`.
pub(super) fn on_outcome(project_root: &Path, payload: &Value, cmd: &str, verdict: Option<f64>) {
    if !r3_shadow_enabled(project_root) {
        return;
    }
    let actor = super::session_key(payload);
    let key = actor_key(project_root, &actor);
    registry().outcome(&key, cmd, verdict, &mut |e| emit(e));
}

/// A per-thread stand-in for the events file, so a test never writes the real journal
/// and never races a parallel test over a global (each test runs on its own thread, and
/// the code under test is synchronous on it).
#[cfg(test)]
pub(super) mod test_sink {
    use serde_json::Value;
    use std::cell::RefCell;

    type Sink = Box<dyn FnMut(&Value) -> bool>;

    thread_local! {
        static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
    }

    /// Route every event of THIS thread to `f` (its `bool` is the emitter's "written").
    pub(in crate::cli_suggester) fn install(f: impl FnMut(&Value) -> bool + 'static) {
        SINK.with(|s| *s.borrow_mut() = Some(Box::new(f)));
    }

    pub(super) fn offer(event: &Value) -> bool {
        SINK.with(|s| match s.borrow_mut().as_mut() {
            Some(f) => {
                let _ = f(event);
                true
            }
            None => false,
        })
    }
}
