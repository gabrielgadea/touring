//! # touring-ceg — the Code Execution Gateway
//!
//! The **X0..X9 execution-gating pipeline** ([`gateway`]) plus the Deno-style
//! deny-by-default **capability model** ([`capability`]), extracted from
//! `touring-hooks` (Fronteira 1 of the monolith decomposition, 2026-06-10 —
//! plan `recursive-cuddling-blossom`, Sessions A+B).
//!
//! ## Architecture
//!
//! - [`gateway`] — [`gateway::pre_exec::run_gateway`] drives X0 CAPTURE → X7
//!   DECISION through the `Execution<S>` typestate (X3 VGP and X5 SANDBOX are
//!   structurally unskippable); X8 supervised execution
//!   ([`gateway::supervised`]) and X9 LEARN ([`gateway::learn`]) close the
//!   loop. The X9 LEARN stage is generic over the `CegRuntime` /
//!   `LearnRuntime` IoC traits (`touring-contracts` + [`gateway::deps`]), so
//!   this crate never names a host runtime type.
//! - [`capability`] — `Capability` / `CapabilityProfile` + the four built-in
//!   profiles (ReadOnly / StagedWrite / Trusted / Sandboxed), with landlock +
//!   rlimit kernel enforcement on Linux.
//!
//! ## Who drives it
//!
//! The Claude Code **hook driver** stays in the parent
//! (`touring_hooks::ceg_adapter`): it maps `GatewayOutcome` verdicts to the
//! hook protocol (`HookResponse`) and feeds X9 LEARN through the parent's
//! `HookRuntime` (which implements the IoC traits). `touring-hooks`
//! re-exports [`gateway`] and [`capability`] at its root, so every historical
//! `touring_hooks::gateway::*` / `crate::gateway::*` path keeps resolving
//! unchanged for the 3 touring-server consumers and the parent's own modules.
//!
//! ## Feature flags
//!
//! - `txn_lock_enforcement` (off by default here; **default-on** via the
//!   parent's forward) — gates the `ExecPool` txn-permit defense-in-depth
//!   path (28 `cfg` sites across `gateway/`).

#![cfg_attr(test, allow(clippy::all))]
#![cfg_attr(not(test), deny(missing_docs))]
#![warn(rustdoc::broken_intra_doc_links)]
// RBP-01 elite-lint ratchet (2026-06-16): security-critical CEG pipeline is
// prod-unwrap-free — lock against future bare unwrap in non-test code
// (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod capability;
pub mod gateway;

// Crate-root re-exports of the capability scope vocabulary — the ergonomic
// entry points for external profile construction (mirrors the historical
// `touring_hooks` surface).
pub use capability::{CmdScope, HostScope, KeyScope, PathScope};
