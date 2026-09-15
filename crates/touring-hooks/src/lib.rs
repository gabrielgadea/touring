//! touring-hooks — compatibility façade over the split Neural Hooks stack
//! (daemon-lib-rearch Phases C+D, 2026-06-10).
//!
//! The monolith was carved into:
//! - **touring-hooks-core** — data/intelligence engines (knowledge, tantivy,
//!   health-delta, bridges) with zero HookRuntime/cli coupling;
//! - **touring-dispatch** — the daemon's nervous system (HookRuntime,
//!   hook_registry, daemon actor, every pre/post lifecycle hook, the cli/
//!   handlers) — it re-exports the core at the historical module paths;
//! - **touring-hooks** (this crate) — a thin façade that re-exports the
//!   dispatch root verbatim plus the `touring-hook` / `touring-daemon`
//!   binaries, so every `touring_hooks::X` path used by touring-server,
//!   touring-cortex, the 57 integration suites and the 6 benches keeps
//!   resolving with zero churn.

// D.W2.P3.T6 (2026-06-11): facade measured at 0 missing docs - lock it in.
#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free façade — lock against
// future bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

// Everything public from the dispatch layer (which itself re-exports the
// core engines + leaf crates) surfaces here at the historical paths.
pub use touring_dispatch::*;

// `#[macro_export]` macros live at their defining crate's root and are not
// reliably carried by glob re-exports — re-export explicitly.
pub use touring_dispatch::with_validation;

pub mod token_meter;

// The `hooks-active` / `hooks-noop` features only forward to `touring-dispatch`,
// where the handlers are compiled in or stubbed out. This façade held a
// `HOOKS_MODE` flag that "each hook function checks at runtime": no hook ever
// read it (the hooks live in the dispatch crate, which cannot depend on this
// façade), and no benchmark or CI job did either. It was removed with the dead
// public items of the cross-audit R2 census (15/09/2026).
