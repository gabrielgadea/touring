//! Wave C2 (2026-04-20) — Auto-cascade subtask planning from API diffs.
//!
//! Bridges three existing building blocks:
//!
//! 1. `diff_api_surfaces` — detects
//!    which public items were added / removed between two snapshots.
//! 2. `CallGraph` — maps callees back to their
//!    call sites (who invokes whom, at what line).
//! 3. An eventual `TaskDecomposer` consumer that turns proposals into
//!    concrete subtasks in the daemon's DAG.
//!
//! The function [`plan_api_cascade`] is the pure data transform in the middle:
//! given a list of `ApiChange`s and a `CallGraph`, it emits a [`CascadePlan`]
//! describing, for each breaking change, which call sites need follow-up work.
//!
//! No I/O, no daemon hops, no cross-crate wiring. Callers (hooks, decomposer)
//! consume the plan at their own layer.

use serde::{Deserialize, Serialize};

use crate::ast::call_graph::{CallGraph, CallSite};
use crate::ast::rust_semantic::{ApiChange, ApiChangeKind};

/// Subtask proposal emitted for a single API change.
///
/// Describes the symbol that changed, the surfaced callers, and a textual
/// rationale the decomposer can paste directly into a subtask description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtaskProposal {
    /// The stringified API item the change refers to (copied from
    /// [`ApiChange::item`]).
    pub api_item: String,
    /// The extracted symbol name (best-effort; see
    /// [`extract_symbol_name`]). Empty when extraction failed.
    pub symbol: String,
    /// Kind of change that triggered the proposal.
    pub kind: ApiChangeKind,
    /// Callers identified via the call graph. Empty when the change is
    /// non-breaking (e.g. `Added` with no prior consumers) or when the
    /// symbol couldn't be extracted.
    pub callers: Vec<CallerRef>,
    /// Human-readable reason, suitable for embedding in a subtask
    /// description.
    pub reason: String,
    /// Severity hint: `"high"` for `Removed` with ≥1 caller (breakage),
    /// `"medium"` for `Added` with existing overloads, `"low"` otherwise.
    pub severity: Severity,
}

/// Compact reference to a call site, cheap to serialize across the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallerRef {
    /// Enclosing function name (from [`CallSite::caller`]).
    pub caller: String,
    /// Line number of the call (1-indexed).
    pub line: usize,
}

impl From<&CallSite> for CallerRef {
    fn from(site: &CallSite) -> Self {
        Self {
            caller: site.caller.clone(),
            line: site.line,
        }
    }
}

/// Severity of a proposed subtask, used by decomposers to prioritize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Definite breakage — e.g. a removed public item with live callers.
    High,
    /// Suggests attention — new surface that may want propagation.
    Medium,
    /// Informational — added surface with no existing callers.
    Low,
}

/// Complete cascade plan — the root structure a decomposer consumes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CascadePlan {
    /// One proposal per breaking change found.
    pub proposals: Vec<SubtaskProposal>,
}

impl CascadePlan {
    /// Number of proposals in the plan.
    #[must_use]
    pub fn len(&self) -> usize {
        self.proposals.len()
    }

    /// `true` when the plan has no proposals (no action required).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.proposals.is_empty()
    }

    /// Count proposals by severity.
    #[must_use]
    pub fn count_by_severity(&self, severity: Severity) -> usize {
        self.proposals
            .iter()
            .filter(|p| p.severity == severity)
            .count()
    }

    /// Return only proposals whose severity is [`Severity::High`].
    #[must_use]
    pub fn high_severity(&self) -> Vec<&SubtaskProposal> {
        self.proposals
            .iter()
            .filter(|p| p.severity == Severity::High)
            .collect()
    }
}

/// Best-effort extraction of the identifier from a stringified Rust API item.
///
/// Recognizes the common shapes produced by
/// `RustSemanticReport::public_api_surface`:
///
/// | Example input                                  | Returned symbol |
/// |-----------------------------------------------|-----------------|
/// | `"pub fn greet(name: &str) -> String"`         | `greet`         |
/// | `"pub const MAX: u32 = 99"`                    | `MAX`           |
/// | `"pub static VERSION: &str = \"1.0\""`         | `VERSION`       |
/// | `"pub struct User { … }"`                      | `User`          |
/// | `"pub enum Level { Low, High }"`               | `Level`         |
/// | `"pub trait Display"`                          | `Display`       |
/// | `"pub type Id = u64"`                          | `Id`            |
/// | `"pub mod utils"`                              | `utils`         |
/// | `"pub unsafe fn drop(ptr: *mut u8)"`           | `drop`          |
/// | anything else                                  | `""` (empty)    |
///
/// This is intentionally simple — it does not parse generics, lifetimes, or
/// attributes. Callers that need full-fidelity extraction should feed the
/// item through `syn::parse_str` instead.
#[must_use]
pub fn extract_symbol_name(item: &str) -> String {
    // Strip leading "pub " (with optional qualifier like "pub(crate) ").
    let after_vis = strip_visibility(item.trim());

    // `fn` always wins when present — handles `pub const fn`, `pub async fn`,
    // `pub unsafe fn`, and plain `pub fn` uniformly.
    let tokens: Vec<&str> = after_vis.split_whitespace().collect();
    if let Some(fn_idx) = tokens.iter().position(|t| *t == "fn") {
        return tokens
            .get(fn_idx + 1)
            .map(|t| trim_identifier(t))
            .unwrap_or_default();
    }

    // No `fn` — pick the first other kind keyword. Covers `const`, `static`,
    // `struct`, `enum`, `trait`, `mod`, `type`, `union`.
    for (i, tok) in tokens.iter().enumerate() {
        if is_non_fn_kind(tok) {
            return tokens
                .get(i + 1)
                .map(|t| trim_identifier(t))
                .unwrap_or_default();
        }
    }
    String::new()
}

/// `true` when `tok` is a Rust item kind other than `fn` (which has
/// dedicated handling upstream to win over modifier positions).
fn is_non_fn_kind(tok: &str) -> bool {
    matches!(
        tok,
        "struct" | "enum" | "trait" | "mod" | "type" | "union" | "static" | "const"
    )
}

/// Build a cascade plan from a set of API changes and a pre-computed
/// [`CallGraph`].
///
/// For each change:
///
/// - **`Removed`** — Every call site in `graph.callers_of(symbol)` is listed
///   as an affected caller. Severity is `High` when at least one caller
///   exists, else `Low` (the removed symbol was dead code).
/// - **`Added`** — Severity is `Medium` when `graph.callers_of(symbol)`
///   already shows call sites (unusual — typically implies a previously-
///   unresolved reference just got satisfied), `Low` otherwise.
///
/// When [`extract_symbol_name`] returns empty, the proposal is still emitted
/// but `callers` is empty and severity degrades to `Low`, so no false-
/// positive cascades fire on unparseable items.
///
/// The plan preserves the input ordering of `changes`.
#[must_use]
pub fn plan_api_cascade(changes: &[ApiChange], graph: &CallGraph) -> CascadePlan {
    let proposals = changes
        .iter()
        .map(|change| build_proposal(change, graph))
        .collect();
    CascadePlan { proposals }
}

/// Build one proposal for a single change.
fn build_proposal(change: &ApiChange, graph: &CallGraph) -> SubtaskProposal {
    let symbol = extract_symbol_name(&change.item);
    let callers: Vec<CallerRef> = if symbol.is_empty() {
        Vec::new()
    } else {
        graph
            .callers_of(&symbol)
            .iter()
            .map(|site| CallerRef::from(*site))
            .collect()
    };

    let severity = classify_severity(&change.kind, &callers);
    let reason = describe_change(&change.kind, &symbol, callers.len());

    SubtaskProposal {
        api_item: change.item.clone(),
        symbol,
        kind: change.kind.clone(),
        callers,
        reason,
        severity,
    }
}

/// Map (kind, caller-count) → severity.
fn classify_severity(kind: &ApiChangeKind, callers: &[CallerRef]) -> Severity {
    match (kind, callers.is_empty()) {
        (ApiChangeKind::Removed, false) => Severity::High,
        (ApiChangeKind::Removed, true) => Severity::Low,
        (ApiChangeKind::Added, false) => Severity::Medium,
        (ApiChangeKind::Added, true) => Severity::Low,
    }
}

/// Compose a human-readable reason string.
fn describe_change(kind: &ApiChangeKind, symbol: &str, caller_count: usize) -> String {
    let symbol_desc = if symbol.is_empty() {
        "<unparseable item>".to_string()
    } else {
        format!("`{symbol}`")
    };
    match kind {
        ApiChangeKind::Removed if caller_count > 0 => format!(
            "Removed {symbol_desc}: update {caller_count} call site(s) that still invoke it."
        ),
        ApiChangeKind::Removed => format!(
            "Removed {symbol_desc}: no live call sites found; verify no dynamic references."
        ),
        ApiChangeKind::Added if caller_count > 0 => format!(
            "Added {symbol_desc}: {caller_count} existing call site(s) may now resolve — review."
        ),
        ApiChangeKind::Added => {
            format!("Added {symbol_desc}: new public surface; consider propagating usage.")
        }
    }
}

/// Strip the leading visibility marker (`pub`, `pub(crate)`, `pub(super)`, …)
/// from an API item string. Whitespace is collapsed.
fn strip_visibility(item: &str) -> &str {
    let trimmed = item.trim_start();
    if let Some(rest) = trimmed.strip_prefix("pub") {
        // `pub` may be followed by `(…) ` qualifier or whitespace.
        let after = rest.trim_start_matches(|c: char| c == '(' || c.is_alphanumeric() || c == ')');
        return after.trim_start();
    }
    trimmed
}

/// Keep only the identifier portion of a token: drop `<…>`, `(`, `:`, `=`, `;`.
fn trim_identifier(token: &str) -> String {
    let end = token
        .find(['<', '(', ':', '=', ';', '{'])
        .unwrap_or(token.len());
    token.get(..end).unwrap_or("").to_string()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::call_graph::CallSite;

    fn mk_site(caller: &str, callee: &str, line: usize) -> CallSite {
        CallSite {
            caller: caller.to_string(),
            callee: callee.to_string(),
            line,
            args_count: 0,
        }
    }

    fn graph_with(sites: Vec<CallSite>) -> CallGraph {
        CallGraph { sites }
    }

    // ── symbol extraction ───

    #[test]
    fn extract_symbol_from_plain_fn() {
        assert_eq!(
            extract_symbol_name("pub fn greet(name: &str) -> String"),
            "greet"
        );
    }

    #[test]
    fn extract_symbol_from_qualified_vis() {
        assert_eq!(
            extract_symbol_name("pub(crate) fn internal() -> u32"),
            "internal"
        );
    }

    #[test]
    fn extract_symbol_from_struct() {
        assert_eq!(extract_symbol_name("pub struct User { id: u64 }"), "User");
    }

    #[test]
    fn extract_symbol_from_enum() {
        assert_eq!(extract_symbol_name("pub enum Level { Low, High }"), "Level");
    }

    #[test]
    fn extract_symbol_from_trait() {
        assert_eq!(extract_symbol_name("pub trait Display"), "Display");
    }

    #[test]
    fn extract_symbol_from_const() {
        assert_eq!(extract_symbol_name("pub const MAX: u32 = 99"), "MAX");
    }

    #[test]
    fn extract_symbol_from_static() {
        assert_eq!(
            extract_symbol_name("pub static VERSION: &str = \"1.0\""),
            "VERSION"
        );
    }

    #[test]
    fn extract_symbol_from_type_alias() {
        assert_eq!(extract_symbol_name("pub type Id = u64"), "Id");
    }

    #[test]
    fn extract_symbol_from_mod() {
        assert_eq!(extract_symbol_name("pub mod utils"), "utils");
    }

    #[test]
    fn extract_symbol_strips_generics() {
        assert_eq!(
            extract_symbol_name("pub fn sort<T: Ord>(items: &mut [T])"),
            "sort"
        );
        assert_eq!(
            extract_symbol_name("pub struct Wrapper<T> { inner: T }"),
            "Wrapper"
        );
    }

    #[test]
    fn extract_symbol_const_fn_prefers_fn_over_const() {
        // `const` modifier on a fn item must not be mistaken for a const kind.
        assert_eq!(
            extract_symbol_name("pub const fn compute() -> u32"),
            "compute"
        );
    }

    #[test]
    fn extract_symbol_handles_unsafe_and_async() {
        assert_eq!(
            extract_symbol_name("pub unsafe fn drop(ptr: *mut u8)"),
            "drop"
        );
        assert_eq!(
            extract_symbol_name("pub async fn fetch(url: &str) -> String"),
            "fetch"
        );
    }

    #[test]
    fn extract_symbol_returns_empty_for_unparseable() {
        assert_eq!(extract_symbol_name(""), "");
        assert_eq!(extract_symbol_name("gibberish without pub"), "");
        assert_eq!(extract_symbol_name("pub fn"), "");
    }

    // ── plan construction ───

    #[test]
    fn cascade_plan_is_empty_when_no_changes() {
        let plan = plan_api_cascade(&[], &CallGraph::default());
        assert!(plan.is_empty());
        assert_eq!(plan.len(), 0);
    }

    #[test]
    fn removed_with_callers_produces_high_severity() {
        let change = ApiChange {
            kind: ApiChangeKind::Removed,
            item: "pub fn greet(name: &str) -> String".to_string(),
        };
        let graph = graph_with(vec![
            mk_site("caller_a", "greet", 10),
            mk_site("caller_b", "greet", 42),
        ]);
        let plan = plan_api_cascade(&[change], &graph);
        assert_eq!(plan.len(), 1);
        let proposal = plan.proposals.first().expect("one proposal");
        assert_eq!(proposal.symbol, "greet");
        assert_eq!(proposal.severity, Severity::High);
        assert_eq!(proposal.callers.len(), 2);
        assert!(proposal.reason.contains("2 call site"));
    }

    #[test]
    fn removed_without_callers_produces_low_severity() {
        let change = ApiChange {
            kind: ApiChangeKind::Removed,
            item: "pub fn unused() -> ()".to_string(),
        };
        let plan = plan_api_cascade(&[change], &CallGraph::default());
        let proposal = plan.proposals.first().expect("one proposal");
        assert_eq!(proposal.severity, Severity::Low);
        assert!(proposal.callers.is_empty());
    }

    #[test]
    fn added_without_callers_produces_low_severity() {
        let change = ApiChange {
            kind: ApiChangeKind::Added,
            item: "pub fn brand_new() -> bool".to_string(),
        };
        let plan = plan_api_cascade(&[change], &CallGraph::default());
        let proposal = plan.proposals.first().expect("one proposal");
        assert_eq!(proposal.severity, Severity::Low);
    }

    #[test]
    fn added_with_prior_callers_produces_medium_severity() {
        // Unusual but real: a dynamic reference already exists when the
        // symbol is newly introduced.
        let change = ApiChange {
            kind: ApiChangeKind::Added,
            item: "pub fn forward_decl() -> ()".to_string(),
        };
        let graph = graph_with(vec![mk_site("consumer", "forward_decl", 7)]);
        let plan = plan_api_cascade(&[change], &graph);
        let proposal = plan.proposals.first().expect("one proposal");
        assert_eq!(proposal.severity, Severity::Medium);
    }

    #[test]
    fn unparseable_item_produces_no_callers_and_low_severity() {
        let change = ApiChange {
            kind: ApiChangeKind::Removed,
            item: "this is not a valid rust item".to_string(),
        };
        let graph = graph_with(vec![mk_site("caller", "greet", 10)]);
        let plan = plan_api_cascade(&[change], &graph);
        let proposal = plan.proposals.first().expect("one proposal");
        assert_eq!(proposal.symbol, "");
        assert!(proposal.callers.is_empty());
        assert_eq!(proposal.severity, Severity::Low);
        assert!(proposal.reason.contains("<unparseable item>"));
    }

    #[test]
    fn plan_preserves_change_ordering() {
        let changes = vec![
            ApiChange {
                kind: ApiChangeKind::Removed,
                item: "pub fn alpha()".to_string(),
            },
            ApiChange {
                kind: ApiChangeKind::Added,
                item: "pub fn bravo()".to_string(),
            },
            ApiChange {
                kind: ApiChangeKind::Removed,
                item: "pub fn charlie()".to_string(),
            },
        ];
        let plan = plan_api_cascade(&changes, &CallGraph::default());
        let names: Vec<String> = plan.proposals.iter().map(|p| p.symbol.clone()).collect();
        assert_eq!(names, vec!["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn count_by_severity_groups_correctly() {
        let graph = graph_with(vec![mk_site("c", "alpha", 1)]);
        let changes = vec![
            ApiChange {
                kind: ApiChangeKind::Removed,
                item: "pub fn alpha()".to_string(),
            }, // High (has caller)
            ApiChange {
                kind: ApiChangeKind::Removed,
                item: "pub fn bravo()".to_string(),
            }, // Low (no caller)
            ApiChange {
                kind: ApiChangeKind::Added,
                item: "pub fn charlie()".to_string(),
            }, // Low (no caller)
        ];
        let plan = plan_api_cascade(&changes, &graph);
        assert_eq!(plan.count_by_severity(Severity::High), 1);
        assert_eq!(plan.count_by_severity(Severity::Medium), 0);
        assert_eq!(plan.count_by_severity(Severity::Low), 2);
        assert_eq!(plan.high_severity().len(), 1);
    }

    #[test]
    fn end_to_end_real_source_pipeline() {
        use crate::ast::rust_semantic::{RustSemanticReport, diff_api_surfaces};

        let before = "\
            pub fn greet(name: &str) -> String { String::new() }\n\
            pub fn farewell(name: &str) -> String { String::new() }\n\
        ";
        let after = "\
            pub fn greet(name: &str) -> String { String::new() }\n\
            // farewell removed\n\
        ";
        let old_api = RustSemanticReport::public_api_surface(before).expect("parse before");
        let new_api = RustSemanticReport::public_api_surface(after).expect("parse after");
        let changes = diff_api_surfaces(&old_api, &new_api);

        // Simulate a codebase where `farewell` still has a caller.
        let graph = graph_with(vec![mk_site("main", "farewell", 12)]);
        let plan = plan_api_cascade(&changes, &graph);

        // At least one Removed proposal must land on `farewell` with High severity.
        let farewell = plan
            .proposals
            .iter()
            .find(|p| p.symbol == "farewell")
            .expect("farewell removal must appear");
        assert_eq!(farewell.kind, ApiChangeKind::Removed);
        assert_eq!(farewell.severity, Severity::High);
        assert_eq!(farewell.callers.len(), 1);
        assert_eq!(farewell.callers.first().map(|c| c.line), Some(12));
    }

    #[test]
    fn cascade_plan_serde_roundtrip() {
        let change = ApiChange {
            kind: ApiChangeKind::Removed,
            item: "pub fn greet()".to_string(),
        };
        let graph = graph_with(vec![mk_site("main", "greet", 5)]);
        let plan = plan_api_cascade(&[change], &graph);
        let json = serde_json::to_string(&plan).expect("serialize");
        let restored: CascadePlan = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(plan, restored);
    }
}
