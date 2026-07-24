//! Workflow Templates — canonical reusable `touring decompose` blueprints.
//!
//! Codifies the 10 operational workflows (W1–W10) defined in Rn2 §6
//! ("Os 10 workflows de operação") as static, `const`-friendly data
//! structures that can be used to pre-populate a `touring decompose` DAG.
//!
//! # Usage
//!
//! ```ignore
//! use touring_hooks::workflow_templates::{template, all_templates};
//!
//! let wf = template("W3").expect("W3 must exist");
//! assert_eq!(wf.id, "W3");
//! assert!(!wf.steps.is_empty());
//! ```
//!
//! # Invariants
//!
//! 1. `ALL_TEMPLATES` contains exactly 10 entries (W1–W10).
//! 2. Every template has at least one step and at least one pattern tag.
//! 3. No heap allocation at lookup time — all data is `&'static`.
//! 4. `template(id)` is O(10) linear scan; templates are looked up by id string.
//! 5. This module never panics — all public functions return `Option` or a slice.

/// A single step within a workflow template.
///
/// Each step maps to one concrete action (tool invocation or Touring CLI command)
/// that the agent should execute at that point in the workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct WorkflowStep {
    /// 1-based execution order within the workflow.
    pub order: u8,
    /// Short imperative description of what to do.
    pub action: &'static str,
    /// The concrete tool or CLI command to invoke (e.g. `"touring index find <symbol>"`).
    pub tool_or_cmd: &'static str,
    /// Optional guidance note, pitfall warning, or rationale.
    pub note: &'static str,
}

/// A reusable workflow template that encodes a canonical sequence of steps
/// for a common agent operation pattern (W1–W10 from Rn2 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct WorkflowTemplate {
    /// Stable identifier, e.g. `"W3"`.
    pub id: &'static str,
    /// Human-readable name, e.g. `"Bugfix pontual (1 arquivo)"`.
    pub name: &'static str,
    /// One-line description of the workflow's purpose.
    pub description: &'static str,
    /// Pattern tags from Rn2 §5 that this workflow applies (e.g. `&["P2", "P4", "P7", "P9"]`).
    pub patterns: &'static [&'static str],
    /// Ordered steps to execute.  At least one step is guaranteed.
    pub steps: &'static [WorkflowStep],
    /// The main pitfall / armadilha to avoid when running this workflow.
    pub pitfall: &'static str,
}

// ---------------------------------------------------------------------------
// W1 — Localizar "onde está X?"
// ---------------------------------------------------------------------------
static W1_STEPS: &[WorkflowStep] = &[
    WorkflowStep {
        order: 1,
        action: "Broad probe via Touring index",
        tool_or_cmd: "touring index find <XSymbol>",
        note: "Primary VGP lookup — returns file:line in <10 ms.",
    },
    WorkflowStep {
        order: 2,
        action: "Fuzzy fallback if name is uncertain",
        tool_or_cmd: "touring tantivy search \"<XSymbol>\"",
        note: "BM25 full-text search handles misspellings and partial names.",
    },
    WorkflowStep {
        order: 3,
        action: "Confirm with narrow Grep",
        tool_or_cmd: "Grep(pattern=\"XSymbol\", output_mode=\"content\", -n, -C 2)",
        note: "Only after index narrows the file; never start here.",
    },
];

// ---------------------------------------------------------------------------
// W2 — Compreender uma feature / fluxo
// ---------------------------------------------------------------------------
static W2_STEPS: &[WorkflowStep] = &[
    WorkflowStep {
        order: 1,
        action: "Get file metadata at entrypoint",
        tool_or_cmd: "touring ast meta <entrypoint_file> --depth summary -j",
        note: "blast_radius + quality_score + fan_in/fan_out before reading raw bytes.",
    },
    WorkflowStep {
        order: 2,
        action: "Trace transitive call chain",
        tool_or_cmd: "touring wiring impact <symbol> --depth 4",
        note: "BFS from entry symbol; depth 4 covers most real chains.",
    },
    WorkflowStep {
        order: 3,
        action: "Map source-to-sink module graph",
        tool_or_cmd: "touring wiring chains",
        note: "Reveals functional chain types: Sequential / Complementary / Hierarchical.",
    },
    WorkflowStep {
        order: 4,
        action: "Delegate broad file reads to a subagent",
        tool_or_cmd: "Task(tools=[\"Read\",\"Grep\",\"Glob\"])",
        note: "Subagent returns only the flow map — keeps orchestrator context clean (P8).",
    },
];

// ---------------------------------------------------------------------------
// W3 — Bugfix pontual (1 arquivo)
// ---------------------------------------------------------------------------
static W3_STEPS: &[WorkflowStep] = &[
    WorkflowStep {
        order: 1,
        action: "Check file metadata + blast radius",
        tool_or_cmd: "touring ast meta <file> --depth summary -j",
        note: "Reveals blast_radius and quality_score before touching anything.",
    },
    WorkflowStep {
        order: 2,
        action: "Pre-edit gate",
        tool_or_cmd: "touring pre-edit",
        note: "Score must be >= 0.8; CILA budget enforced.",
    },
    WorkflowStep {
        order: 3,
        action: "Locate the exact line",
        tool_or_cmd: "Grep(symbol, -n)",
        note: "Get exact file:line before Read.",
    },
    WorkflowStep {
        order: 4,
        action: "Read the precise window",
        tool_or_cmd: "Read(file, offset=N, limit=30)",
        note: "Always read the exact window before Edit to avoid old_string mismatch.",
    },
    WorkflowStep {
        order: 5,
        action: "Apply fix via canonical edit workflow",
        tool_or_cmd: "Edit tool --path <file> --intent \"<fix description>\"",
        note: "Atomic snapshot + shadow validate + cargo check inside Edit tool.",
    },
    WorkflowStep {
        order: 6,
        action: "Validate compilation",
        tool_or_cmd: "cargo check -p <crate>",
        note: "Always compile after every fix — P9 invariant.",
    },
];

// ---------------------------------------------------------------------------
// W4 — Refatoração cross-file (renomear / extrair / mover)
// ---------------------------------------------------------------------------
static W4_STEPS: &[WorkflowStep] = &[
    WorkflowStep {
        order: 1,
        action: "Full blast radius before any change",
        tool_or_cmd: "touring ast blast <file>",
        note: "Know every dependent before mutating the first.",
    },
    WorkflowStep {
        order: 2,
        action: "Map all callsites of the symbol",
        tool_or_cmd: "touring wiring impact <symbol> --depth 2",
        note: "Transitive consumer list — P6: map ALL before mutating any.",
    },
    WorkflowStep {
        order: 3,
        action: "Apply structural rewrite across all callsites",
        tool_or_cmd: "touring ast grep <file> <pattern> --rewrite <replacement>",
        note: "One ast-grep operation beats N×Edit — avoids missing a callsite.",
    },
    WorkflowStep {
        order: 4,
        action: "Wiring audit post-refactor",
        tool_or_cmd: "touring wiring audit -j",
        note: "Zero new orphans required — REGRA #0.",
    },
    WorkflowStep {
        order: 5,
        action: "Compile the workspace",
        tool_or_cmd: "cargo check --workspace",
        note: "Cross-file refactors can break consumers in other crates.",
    },
];

// ---------------------------------------------------------------------------
// W5 — Criar feature / módulo novo
// ---------------------------------------------------------------------------
static W5_STEPS: &[WorkflowStep] = &[
    WorkflowStep {
        order: 1,
        action: "VGP: verify new symbol does not collide",
        tool_or_cmd: "touring index find <NewSymbol>",
        note: "Must return count:0 before creating.",
    },
    WorkflowStep {
        order: 2,
        action: "Discover similar files for convention reference",
        tool_or_cmd: "Glob(pattern=\"<similar_pattern>\") then Read(example_file)",
        note: "Imitate existing conventions — P4 map-first.",
    },
    WorkflowStep {
        order: 3,
        action: "Create via canonical workflow (edição-com-gate)",
        tool_or_cmd: "Write tool --path <file> --intent \"<intent>\" --kind RustModule",
        note: "Never use Write tool for new .rs files — hook BLOCKS it.",
    },
    WorkflowStep {
        order: 4,
        action: "Check for orphan pub symbols post-creation",
        tool_or_cmd: "touring wiring orphans -j",
        note: "REGRA #0: new pub symbols must be wired to consumers.",
    },
    WorkflowStep {
        order: 5,
        action: "Register in parent lib.rs",
        tool_or_cmd: "Edit(lib.rs, add \"pub mod <new_module>;\") — or Edit tool",
        note: "Module must be declared before cargo can see it.",
    },
];

// ---------------------------------------------------------------------------
// W6 — Depurar teste falhando / erro
// ---------------------------------------------------------------------------
static W6_STEPS: &[WorkflowStep] = &[
    WorkflowStep {
        order: 1,
        action: "System health gate (FASE 0)",
        tool_or_cmd: "touring doctor -j",
        note: "Daemon or index issues can produce phantom test failures.",
    },
    WorkflowStep {
        order: 2,
        action: "Recall similar past bugs",
        tool_or_cmd: "touring memory recall \"<symptom>\"",
        note: "Pattern match against past lessons before fresh diagnosis.",
    },
    WorkflowStep {
        order: 3,
        action: "Check known pitfalls for the file",
        tool_or_cmd: "touring gotcha match <file>",
        note: "Pitfall DB catches recurring mistakes before the edit.",
    },
    WorkflowStep {
        order: 4,
        action: "Run the failing test and capture stderr",
        tool_or_cmd: "Bash(\"cargo test -p <crate> -- <test_name> 2>&1\")",
        note: "Capture actual error output — never hypothesize without running.",
    },
    WorkflowStep {
        order: 5,
        action: "Locate symbol from stack trace",
        tool_or_cmd: "Grep(<symbol_from_trace>, -n)",
        note: "Narrow to exact file:line before reading.",
    },
    WorkflowStep {
        order: 6,
        action: "Cross-caller compare if 2+ similar callers",
        tool_or_cmd: "touring wiring impact <suspect_fn> --depth 2",
        note: "C08 mandatory — asymmetry between callers is a common bug root.",
    },
    WorkflowStep {
        order: 7,
        action: "Apply fix and rerun test",
        tool_or_cmd: "Edit tool → cargo test -p <crate>",
        note: "Circuit breaker: if same error appears 2×, re-diagnose the CAUSE.",
    },
];

// ---------------------------------------------------------------------------
// W7 — Code review
// ---------------------------------------------------------------------------
static W7_STEPS: &[WorkflowStep] = &[
    WorkflowStep {
        order: 1,
        action: "TDG grade: structural quality",
        tool_or_cmd: "touring ast tdg <file>",
        note: "Grade D/F → flag for refactor before approving.",
    },
    WorkflowStep {
        order: 2,
        action: "Wiring audit: orphan and integration health",
        tool_or_cmd: "touring wiring audit -j",
        note: "integration_score < 1.0 on any modified module = review issue.",
    },
    WorkflowStep {
        order: 3,
        action: "Rust semantic analysis: unsafe, complexity",
        tool_or_cmd: "touring ast rust-semantic <file>",
        note: "unsafe_count, async_count, semantic_complexity — flag if high.",
    },
    WorkflowStep {
        order: 4,
        action: "Fan-out parallel read-only review",
        tool_or_cmd: "Task(tools=[\"Read\",\"Grep\",\"Glob\"]) — read-only allowlist",
        note: "P8: subagent for broad reads; restrict to read-only tools (no Write/Edit).",
    },
];

// ---------------------------------------------------------------------------
// W8 — Validar (build / test / lint)
// ---------------------------------------------------------------------------
static W8_STEPS: &[WorkflowStep] = &[
    WorkflowStep {
        order: 1,
        action: "E2E composite health score",
        tool_or_cmd: "touring e2e -j",
        note: "Score 0-1 across the full system — quick pre-validation gate.",
    },
    WorkflowStep {
        order: 2,
        action: "Compile check",
        tool_or_cmd: "cargo check --workspace",
        note: "FASE 0 gate — exit code != 0 blocks all subsequent phases.",
    },
    WorkflowStep {
        order: 3,
        action: "Run test suite in background for long suites",
        tool_or_cmd: "Bash(\"cargo test --workspace\", run_in_background=True)",
        note: "Avoid foreground timeout on long suites — poll result.",
    },
    WorkflowStep {
        order: 4,
        action: "Clippy lint gate",
        tool_or_cmd: "cargo clippy --workspace -- -D warnings",
        note: "Zero warnings target — warnings in PR = tech debt.",
    },
];

// ---------------------------------------------------------------------------
// W9 — Explorar codebase desconhecido
// ---------------------------------------------------------------------------
static W9_STEPS: &[WorkflowStep] = &[
    WorkflowStep {
        order: 1,
        action: "Check index status — rebuild if project is new",
        tool_or_cmd: "touring index status  (then: touring index rebuild $PWD if stale)",
        note: "Never explore without an up-to-date index.",
    },
    WorkflowStep {
        order: 2,
        action: "Workspace-level package and feature map",
        tool_or_cmd: "touring ast workspace-info",
        note: "cargo_metadata: packages, features, dependents — P4 Map-First.",
    },
    WorkflowStep {
        order: 3,
        action: "Find entrypoints",
        tool_or_cmd: "Grep(pattern=\"fn main|fn handler|#\\[tokio::main\", output_mode=\"files_with_matches\")",
        note: "Locate the seams before reading any file body.",
    },
    WorkflowStep {
        order: 4,
        action: "Delegate broad exploration to a subagent",
        tool_or_cmd: "Task(tools=[\"Read\",\"Grep\",\"Glob\"]) — returns flow map only",
        note: "P8: subagent returns a compact map; orchestrator context stays clean.",
    },
];

// ---------------------------------------------------------------------------
// W10 — Versionar / commit
// ---------------------------------------------------------------------------
static W10_STEPS: &[WorkflowStep] = &[
    WorkflowStep {
        order: 1,
        action: "Persist implementation state as Touring snapshot",
        tool_or_cmd: "touring memory store \"snapshot:<task>:<ts>\" \"<state>\" --tier semantic",
        note: "REGRA #11: git is prohibited — Touring memory is the audit trail.",
    },
    WorkflowStep {
        order: 2,
        action: "Inject RL reward to close the learning loop",
        tool_or_cmd: "touring learning reward edit 1.0 \"<feature completed>\"",
        note: "Always close the RL loop before declaring a task done.",
    },
    WorkflowStep {
        order: 3,
        action: "Write diary entry",
        tool_or_cmd: "touring diary write touring-engineer \"<summary>\" --aaak --topic implement",
        note: "AAAK format: what was done, why, lessons, next steps.",
    },
    WorkflowStep {
        order: 4,
        action: "Commit via heredoc (git managed by Gabriel only)",
        tool_or_cmd: "Bash(\"git commit -m \\\"$(cat <<'EOF'\\n...\\nEOF\\n)\\\"\")",
        note: "P7+P10: multi-line commit message via heredoc — NEVER interactive git.",
    },
];

// ---------------------------------------------------------------------------
// Static catalog
// ---------------------------------------------------------------------------

static W1: WorkflowTemplate = WorkflowTemplate {
    id: "W1",
    name: "Localizar \"onde está X?\"",
    description: "Locate a symbol, file, or concept in the codebase using index-first strategy.",
    patterns: &["P1", "P5"],
    steps: W1_STEPS,
    pitfall: "Generic term (data, Manager) produces thousands of hits. After 2 iterations without narrowing, change the search term — do not keep reading.",
};

static W2: WorkflowTemplate = WorkflowTemplate {
    id: "W2",
    name: "Compreender uma feature / fluxo",
    description: "Understand how a feature or code path works end-to-end without saturating context.",
    patterns: &["P4", "P8", "P3"],
    steps: W2_STEPS,
    pitfall: "Reading file-by-file in the main context. Use P8 — broad exploration is the canonical subagent use case.",
};

static W3: WorkflowTemplate = WorkflowTemplate {
    id: "W3",
    name: "Bugfix pontual (1 arquivo)",
    description: "Apply a targeted fix to a single file with full pre/post validation.",
    patterns: &["P2", "P4", "P7", "P9"],
    steps: W3_STEPS,
    pitfall: "Editing without Read of the exact window first — Edit fails due to old_string mismatch.",
};

static W4: WorkflowTemplate = WorkflowTemplate {
    id: "W4",
    name: "Refatoração cross-file (renomear / extrair / mover)",
    description: "Rename or move a symbol across multiple files without missing any callsite.",
    patterns: &["P4", "P6", "P7", "P9", "P10"],
    steps: W4_STEPS,
    pitfall: "Editing callsites one-by-one misses one. Map ALL with P6 before mutating any.",
};

static W5: WorkflowTemplate = WorkflowTemplate {
    id: "W5",
    name: "Criar feature / módulo novo",
    description: "Create a new pub symbol or module and wire it to consumers (REGRA #0).",
    patterns: &["P4", "P7", "P9", "P10"],
    steps: W5_STEPS,
    pitfall: "Creating an orphan pub symbol. Always check wiring orphans post-creation.",
};

static W6: WorkflowTemplate = WorkflowTemplate {
    id: "W6",
    name: "Depurar teste falhando / erro",
    description: "Diagnose and fix a failing test or runtime error, starting from system health.",
    patterns: &["P5", "P6", "P1", "P10"],
    steps: W6_STEPS,
    pitfall: "Fixing the symptom. If the same error appears twice, re-diagnose the root cause (circuit breaker).",
};

static W7: WorkflowTemplate = WorkflowTemplate {
    id: "W7",
    name: "Code review",
    description: "Structural and semantic review of a code change using TDG grade, wiring, and semantics.",
    patterns: &["P8", "P3", "P4"],
    steps: W7_STEPS,
    pitfall: "Running review with write-enabled tools. Restrict the subagent to read-only allowlist.",
};

static W8: WorkflowTemplate = WorkflowTemplate {
    id: "W8",
    name: "Validar (build / test / lint)",
    description: "Run the full build, test, and lint validation pipeline.",
    patterns: &["P9", "P3"],
    steps: W8_STEPS,
    pitfall: "Running a long test suite in foreground — hits timeout. Use run_in_background and poll.",
};

static W9: WorkflowTemplate = WorkflowTemplate {
    id: "W9",
    name: "Explorar codebase desconhecido",
    description: "Orient in an unfamiliar codebase using index-first and subagent exploration.",
    patterns: &["P4", "P8", "P1"],
    steps: W9_STEPS,
    pitfall: "Reading file-by-file without a map. Always build the map first (P4), then dive in.",
};

static W10: WorkflowTemplate = WorkflowTemplate {
    id: "W10",
    name: "Versionar / commit",
    description: "Snapshot state, close RL loop, write diary, and commit with proper heredoc message.",
    patterns: &["P7", "P10"],
    steps: W10_STEPS,
    pitfall: "Multi-line commit message without heredoc breaks shell quoting.",
};

/// All 10 canonical workflow templates in W1–W10 order.
pub static ALL_TEMPLATES: &[WorkflowTemplate] = &[W1, W2, W3, W4, W5, W6, W7, W8, W9, W10];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Return the [`WorkflowTemplate`] with the given `id` (e.g. `"W3"`), or
/// `None` if no template with that id exists.
///
/// Lookup is case-sensitive and O(n) over the 10-element catalog.
pub fn template(id: &str) -> Option<&'static WorkflowTemplate> {
    ALL_TEMPLATES.iter().find(|t| t.id == id)
}

/// Return a reference to the full catalog slice (`&[WorkflowTemplate; 10]`).
pub fn all_templates() -> &'static [WorkflowTemplate] {
    ALL_TEMPLATES
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_exactly_ten_templates() {
        assert_eq!(
            all_templates().len(),
            10,
            "catalog must contain exactly W1-W10"
        );
    }

    #[test]
    fn all_templates_have_non_empty_steps() {
        for wf in all_templates() {
            assert!(!wf.steps.is_empty(), "template {} has no steps", wf.id);
        }
    }

    #[test]
    fn all_templates_have_non_empty_patterns() {
        for wf in all_templates() {
            assert!(
                !wf.patterns.is_empty(),
                "template {} has no pattern tags",
                wf.id
            );
        }
    }

    #[test]
    fn template_w3_is_found() {
        let wf = template("W3").expect("W3 must be present in catalog");
        assert_eq!(wf.id, "W3");
        assert!(!wf.steps.is_empty());
    }

    #[test]
    fn template_lookup_returns_none_for_unknown_id() {
        assert!(template("W99").is_none());
        assert!(template("").is_none());
        assert!(template("w1").is_none(), "lookup is case-sensitive");
    }

    #[test]
    fn step_orders_are_contiguous_and_start_at_one() {
        for wf in all_templates() {
            for (i, step) in wf.steps.iter().enumerate() {
                assert_eq!(
                    step.order as usize,
                    i + 1,
                    "template {} step {} has non-contiguous order {}",
                    wf.id,
                    i,
                    step.order
                );
            }
        }
    }

    #[test]
    fn all_template_ids_are_w1_through_w10() {
        let expected = ["W1", "W2", "W3", "W4", "W5", "W6", "W7", "W8", "W9", "W10"];
        for (wf, exp_id) in all_templates().iter().zip(expected.iter()) {
            assert_eq!(wf.id, *exp_id);
        }
    }

    #[test]
    fn all_steps_have_non_empty_fields() {
        for wf in all_templates() {
            for step in wf.steps {
                assert!(
                    !step.action.is_empty(),
                    "template {} step {} action empty",
                    wf.id,
                    step.order
                );
                assert!(
                    !step.tool_or_cmd.is_empty(),
                    "template {} step {} tool_or_cmd empty",
                    wf.id,
                    step.order
                );
            }
        }
    }

    #[test]
    fn w10_references_touring_memory_store() {
        let w10 = template("W10").expect("W10 must exist");
        let has_memory = w10
            .steps
            .iter()
            .any(|s| s.tool_or_cmd.contains("touring memory store"));
        assert!(
            has_memory,
            "W10 must reference touring memory store (REGRA #11)"
        );
    }

    #[test]
    fn w5_references_taco_forge_create() {
        let w5 = template("W5").expect("W5 must exist");
        let has_create = w5
            .steps
            .iter()
            .any(|s| s.tool_or_cmd.contains("Write tool"));
        assert!(has_create, "W5 must reference Write tool (edição-com-gate)");
    }
}
