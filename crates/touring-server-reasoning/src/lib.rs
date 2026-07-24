//! touring-server-reasoning — reasoning layer extracted from touring-server (W9 pragmatic split).
//!
//! Internal workspace crate — cycle-free LEAF (depends on no other touring-server-* crate).
//! Re-exported verbatim by the `touring-server` facade so the external API is unchanged.

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod reasoning;
