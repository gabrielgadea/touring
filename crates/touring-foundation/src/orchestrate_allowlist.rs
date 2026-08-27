//! The read-only hook allowlist that `touring run --orchestrate` speaks — and
//! the single source from which BOTH the SDK text and the daemon's enforcement
//! are derived.
//!
//! # Why this lives here
//!
//! Until 2026-08-27 the allowlist existed only inside the injected Python
//! client, as `self.READONLY_HOOKS`. That taught the contract but did not
//! impose it, and the difference was demonstrable in one program:
//!
//! ```python
//! touring.READONLY_HOOKS = touring.READONLY_HOOKS + ("cli-memory-store",)
//! touring.query("cli-memory-store", {"key": "x", "value": "…"})   # gravou
//! ```
//!
//! A tuple on a client object is a suggestion. The daemon accepted the call
//! because nothing on its side had ever been told what a sandboxed sub-call is
//! allowed to ask for. This module is that telling: the daemon now refuses any
//! non-allowlisted hook whose request carries a sandbox `origin`, and the SDK
//! renders its guard from the same array — so the text the model reads and the
//! rule the daemon applies cannot drift apart.
//!
//! It is the same lesson the code-mode presentation already paid for
//! ([`crate::code_mode`]): one definition, every executor.

/// Marker embedded in the `origin` of every `--orchestrate` sub-call
/// (`<run_id>:code:<n>`), minted by `touring run` and stamped by the SDK.
///
/// Its presence is what tells the daemon "this request came from inside a
/// sandbox", which is the only condition under which the allowlist applies:
/// an ordinary CLI or hook call is not a sandboxed sub-call and keeps the full
/// dispatch surface.
pub const SANDBOX_ORIGIN_MARKER: &str = ":code:";

/// Hooks a sandboxed `--orchestrate` program may call.
///
/// Curated by PURPOSE, never by name pattern: a filter over mutating verbs
/// still let `cli-gotcha-add`, `cli-jobs-spawn` and `cli-saga-begin` through,
/// because absence of a dangerous word is not evidence of safety. Deliberately
/// excluded: `cli-ast-grep` (has a `--rewrite` mode) and `cli-wiring-suggest`.
pub const READONLY_HOOKS: &[&str] = &[
    "cli-ast-blast", "cli-ast-blast-cross-feature", "cli-ast-blast-enriched",
    "cli-ast-callgraph", "cli-ast-calls", "cli-ast-detail", "cli-ast-features",
    "cli-ast-find", "cli-ast-heat", "cli-ast-imports", "cli-ast-meta",
    "cli-ast-modules", "cli-ast-overview", "cli-ast-quality", "cli-ast-rationale",
    "cli-ast-scan", "cli-ast-scope", "cli-ast-semantic", "cli-ast-skeleton",
    "cli-ast-tdg", "cli-ast-todos", "cli-cognitive-metrics",
    "cli-decompose-frontier", "cli-decompose-get", "cli-decompose-ready",
    "cli-decompose-status", "cli-doctor", "cli-evolution-drift",
    "cli-evolution-insights", "cli-file-knowledge-audit",
    "cli-file-knowledge-extended", "cli-file-knowledge-stats",
    "cli-find-references", "cli-gate-metrics", "cli-gotcha-list",
    "cli-gotcha-match", "cli-gotcha-stats", "cli-granularity-status",
    "cli-health-delta-history", "cli-health-delta-status", "cli-incremental-status",
    "cli-index-files", "cli-index-find", "cli-index-search", "cli-index-status",
    "cli-kpi", "cli-learning-status", "cli-memory-communities", "cli-memory-list",
    "cli-memory-moc", "cli-memory-query", "cli-memory-recall", "cli-memory-stats",
    "cli-memory-tags", "cli-repo-health", "cli-repo-score", "cli-resolve-def",
    "cli-search-docs", "cli-search-symbols", "cli-status", "cli-tantivy-fuzzy",
    "cli-tantivy-search", "cli-tantivy-stats", "cli-wiring-chains",
    "cli-wiring-community", "cli-wiring-cycles", "cli-wiring-impact",
    "cli-wiring-modules", "cli-wiring-orphans", "cli-wiring-purpose",
    "cli-wiring-status",
];

/// Whether `origin` identifies a sub-call made from inside a run sandbox.
///
/// Empty or absent origin ⇒ not a sandbox call. The marker is the whole test;
/// a caller that forges it only narrows its OWN surface, never widens it.
#[must_use]
pub fn is_sandbox_origin(origin: Option<&str>) -> bool {
    origin.is_some_and(|o| o.contains(SANDBOX_ORIGIN_MARKER))
}

/// Whether a sandboxed sub-call to `hook` is permitted.
#[must_use]
pub fn sandbox_may_call(hook: &str) -> bool {
    READONLY_HOOKS.contains(&hook)
}

/// The refusal a sandboxed sub-call gets for a non-allowlisted hook.
///
/// Names the hook and points at the escape hatch, because a refusal that does
/// not say what to do instead is just an obstacle: a program that genuinely
/// needs to mutate runs OUTSIDE the sandbox, where the caller is accountable.
#[must_use]
pub fn refusal(hook: &str) -> String {
    format!(
        "hook '{hook}' is not in the orchestrate read-only allowlist ({} hooks). \
         A sandboxed program reads; it does not mutate daemon state. Run the \
         mutation outside the sandbox (plain `touring <cmd>`), where the caller \
         is accountable for it.",
        READONLY_HOOKS.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allowlist_is_sorted_and_unique() {
        let mut ordenada = READONLY_HOOKS.to_vec();
        ordenada.sort_unstable();
        assert_eq!(READONLY_HOOKS.to_vec(), ordenada, "deve estar ordenada");
        let n = ordenada.len();
        ordenada.dedup();
        assert_eq!(n, ordenada.len(), "sem duplicatas");
    }

    /// A defesa em profundidade sobre a curadoria: nenhum verbo de MUTAÇÃO na
    /// lista. Não é o critério de seleção — a curadoria é por propósito —, é a
    /// rede embaixo dela.
    #[test]
    fn no_mutating_verb_is_allowlisted() {
        const MUTANTES: &[&str] = &[
            "-add", "-create", "-update", "-delete", "-reset", "-store", "-write",
            "-rebuild", "-ingest", "-spawn", "-drop", "-begin", "-abort", "-commit",
            "-apply", "-purge", "-flush", "-install", "-init", "-sync", "-import",
            "-reindex", "-populate", "-repair", "-rename", "-run", "-claim",
            "-release", "-finalize", "-reload", "-unregister", "-gc", "-warmstart",
            "-drain", "-consumed", "-checkpoint", "-start", "-edit", "-backfill",
        ];
        for h in READONLY_HOOKS {
            for m in MUTANTES {
                assert!(!h.ends_with(m), "`{h}` termina em `{m}` — não é leitura");
            }
        }
    }

    /// O hook exato que o probe de 27/08 usou para provar o bypass.
    #[test]
    fn the_hook_that_proved_the_bypass_is_refused() {
        assert!(!sandbox_may_call("cli-memory-store"));
        assert!(sandbox_may_call("cli-memory-recall"), "ler segue permitido");
        assert!(refusal("cli-memory-store").contains("cli-memory-store"));
    }

    #[test]
    fn only_a_stamped_origin_is_a_sandbox_call() {
        assert!(is_sandbox_origin(Some("run-123-456:code:7")));
        assert!(!is_sandbox_origin(None));
        assert!(!is_sandbox_origin(Some("")));
        assert!(!is_sandbox_origin(Some("cli")), "chamada normal não é sandbox");
    }
}
