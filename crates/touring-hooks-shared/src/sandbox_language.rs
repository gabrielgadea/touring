//! Sandbox language vocabulary — the [`SandboxLanguage`] enum.
//!
//! Fronteira 2 follow-up (2026-06-10): extracted from
//! `touring-ceg::gateway::sandbox_executor` into the leaf crate so that
//! leaf-side consumers (the risk-pattern matcher
//! [`crate::forbidden_patterns`]) can classify per language without depending
//! on the Code Execution Gateway. `touring-ceg` re-exports it from
//! `sandbox_executor`, so every `crate::gateway::sandbox_executor::SandboxLanguage`
//! and `touring_ceg::gateway::sandbox_executor::SandboxLanguage` path — plus the
//! `touring_hooks::sandbox_executor::SandboxLanguage` parent re-export — keeps
//! resolving unchanged for all 8 workspace consumers.
//!
//! This follows the established "vocabulary in the leaf" pattern already used
//! for `isolation_mode`, `severity`, and `signal_layer`.

/// I-11 — Sandbox language enum mapped to executable runtime detection.
//
// `Serialize`/`Deserialize` (CEG P3.2): the gateway records the classified
// language of a captured call in its serde-persisted `Evidence` ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SandboxLanguage {
    /// JavaScript source executed via a Node-family runtime.
    JavaScript,
    /// TypeScript source executed via a Node/Deno-family runtime.
    TypeScript,
    /// Python source.
    Python,
    /// Ruby source.
    Ruby,
    /// Go source.
    Go,
    /// Rust source.
    Rust,
    /// PHP source.
    Php,
    /// Perl source.
    Perl,
    /// R source.
    R,
    /// Elixir source.
    Elixir,
    /// Shell script (sh/bash-family).
    Shell,
}
