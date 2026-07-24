"""glossary — render_glossary_md.

Extracted from renderers.py lines 898-1044. Each module owns one logical
rendering concern (utility, index/wave/cross-audit, one of the 9 cross-cutting
docs). All public functions are re-exported by ``renderers/__init__.py``.
"""
from __future__ import annotations

from .utilities import yaml_frontmatter, md_table, write_atomic, sha256_hex

def render_glossary_md() -> str:
    """Render 04-GLOSSARY.md — terminology canonical."""
    meta = {"plan": _PLAN_NAME, "version": _VERSION, "type": "glossary", "created": _TODAY}
    fm = yaml_frontmatter(meta)
    body = textwrap.dedent(f"""\
        # 04-GLOSSARY — Touring Premium Terminology

        > Canonical definitions for terms used across the {_PLAN_NAME} plan.
        > When ambiguity arises, this glossary is the source of truth.

        ## A

        - **ACO**: Ant Colony Optimization. Used in touring-intelligence for adaptive threshold tuning and bandit policy adjustments via pheromone trails.
        - **ADR**: Architecture Decision Record. Markdown doc capturing a single decision (context, decision, consequences). This plan has 3: ADR-001/002/003.
        - **AST**: Abstract Syntax Tree. Output of code parsers (tree-sitter, syn, ast-grep). Lives in touring-code.
        - **ast-grep**: Polyglot structural search + rewrite engine (Rust, TypeScript, Python, Go, etc.). Wraps tree-sitter parsing with pattern matching.

        ## B

        - **Bandit**: Reinforcement-learning policy (LinUCB, ε-greedy) that selects actions to maximize cumulative reward. Used for tool routing in touring-intelligence.
        - **Blast Radius**: Number of files/symbols transitively affected by editing a given file. Computed by `touring ast blast <file>`.
        - **BM25**: Best Match 25 — ranking function for TF-IDF text retrieval. Used in Tantivy FTS and reranker pipelines.

        ## C

        - **Cargo features**: Mechanism for conditional compilation. Maps to tier-* in touring-server (free/standard/premium/enterprise).
        - **Claude Code (CC)**: Anthropic's official CLI for Claude. Touring integrates via 24 lifecycle hooks (pre-edit, post-edit, session_start, etc.).
        - **Composite Health Score**: Workspace-wide quality metric (0.0-1.0) combining wiring, complexity, modularity, redundancy.
        - **Cortex**: Internal sub-system of touring-intelligence that orchestrates handler dispatch, signal fusion, scoring, and cross-audit.
        - **CRC**: Refinement Engine cycle — Execute → Observe → Diagnose → Decide → Act → Validate.
        - **Cycle (dependency)**: Path in the dependency graph that returns to its origin. Detected by `touring wiring cycles` (Tarjan SCC). Goal: zero cycles workspace-wide.

        ## D

        - **DAG**: Directed Acyclic Graph. Used to represent task decomposition (touring decompose) and wave dependencies.
        - **Daemon**: Background process holding the symbol index, knowledge DB, and RPC interface. Spawned via `touring-hook --start-daemon`.
        - **Decompose**: Task breakdown into a DAG of subtasks with dependencies. Lives in touring-orchestration.
        - **DISCOVER**: 5-step protocol before code generation (tantivy + wiring + ast + memory + index).

        ## E

        - **Engineer-day**: Unit of effort estimation. 1 engineer-day = 8 working hours of focused engineering.
        - **EntityId**: Deterministic identifier in RFC-004 entity registry. Derived from canonical name + admission criteria.
        - **External Client**: User outside Gabriel's workstation who installs Touring via `curl install.touring.dev | sh`.

        ## F

        - **Façade**: Single public crate that re-exports symbols from multiple internal sub-crates. Used in touring-server, touring-hooks, touring-bindings.
        - **Feature flag (Cargo)**: Compile-time toggle for optional functionality. E.g., `lang-rust`, `intel-mcts`, `tier-premium`.

        ## G

        - **Gate (quality)**: Non-negotiable threshold that must be met before a wave is declared complete. E.g., test ratio ≥ 20%, cycle count = 0.
        - **GoT**: Graph of Thoughts. Reasoning structure in touring-intelligence/reasoning/ for multi-step inference.

        ## H

        - **Halstead**: Software complexity metric (n1, n2, N1, N2, V, D, E, B, T). Computed by touring-analysis/quality.
        - **Hook**: Function invoked at a specific point in Claude Code lifecycle (pre-edit, post-edit, session_start, etc.). 24 events covered.

        ## I

        - **Intelligence**: Layer 4 of the topology — touring-intelligence crate combining reasoning + RL + pipeline + ANN.

        ## J

        - **JWT**: JSON Web Token. Used for license keys (ed25519-signed) in tier-premium and tier-enterprise.

        ## K

        - **Kernel**: Layer 2 of the topology — primitives without policy. Includes touring-simd, touring-rkyv, touring-identity.
        - **Kill rate (mutation)**: Fraction of mutations introduced by cargo-mutants that are detected by tests. Target ≥ 80%.

        ## L

        - **LinUCB**: Linear Upper Confidence Bound — contextual bandit algorithm. Used for tool selection.
        - **LOC**: Lines of Code (non-blank, non-comment by convention).

        ## M

        - **Macrociclo**: A cycle in the dependency graph spanning many crates. The {_PLAN_NAME} plan targets elimination of the depth-618 macrocycle present at audit time.
        - **MCTS**: Monte Carlo Tree Search. Used in touring-intelligence/reasoning/ for plan exploration and decompose.
        - **MSRV**: Minimum Supported Rust Version. Target for v1.0: 1.83 LTS.
        - **Mutation testing**: Inject syntactic mutations and verify tests catch them. Tool: cargo-mutants.

        ## N

        - **N1 Generator**: Code-first generator that produces other code (vs N0 hand-written scripts). `generate_plan.py` is N1.

        ## O

        - **OKR**: Objectives + Key Results. Used in Y1 planning.
        - **Orphan (symbol)**: Public symbol without any consumer. Detected by `touring wiring orphans`. Goal (REGRA #0): zero new orphans per wave.

        ## P

        - **Pensieve**: Memory subsystem in touring-intelligence/reasoning/ for persistent lessons across sessions.
        - **PLG**: Product-Led Growth — sales motion where the free product drives conversion (no SDR involvement).
        - **PSI**: Pressure Stall Information (Linux kernel). Used by touring-foundation/sentinel/ for memory pressure detection.
        - **Pub (Rust)**: Public visibility modifier. Pub items are part of the API surface; pub(crate) is internal-only.

        ## R

        - **RFC**: Request for Comments — formal spec doc. The plan references RFC-001..005 (Constitution v8.0).
        - **REGRA**: Portuguese for "rule" — used in CLAUDE.md to label non-negotiable rules (REGRA #0 potencializar, REGRA #11 git proibido, etc.).
        - **rkyv**: Zero-copy serialization library. Lives in touring-rkyv (kernel layer).
        - **RL**: Reinforcement Learning. Touring-intelligence/rl/ handles bandit, online_rl, clustering.

        ## S

        - **SBOM**: Software Bill of Materials. CycloneDX format published per release.
        - **Semver**: Semantic Versioning (major.minor.patch). Enforced via cargo-semver-checks.
        - **Sigstore**: Open-source artifact signing. Used for release tarball verification.
        - **SLG**: Sales-Led Growth — sales motion with active engagement (inside sales or full enterprise).
        - **SSO**: Single Sign-On (Okta/Google/GitHub). Enterprise feature.
        - **syn**: Rust parsing library. Used for deep Rust semantic analysis in touring-code/parsers/syn/.

        ## T

        - **TACO**: Touring Agentic Code Orchestration. Identity of this orchestrator.
        - **Tantivy**: Rust full-text search library (BM25). Used in touring-storage/fts/.
        - **TDG**: Technical Debt Grade (A+..F). 6-dimensional grade computed by touring-analysis/quality/tdg.
        - **Tier**: Commercial level (Free/Standard/Premium/Enterprise). Maps to Cargo features.
        - **Typestate**: Encoding state transitions in type system. Used in touring-generator pipeline (Draft → Verified → Rendered → Speculated → Committed).
        - **tree-sitter**: Multi-language parsing library. Used in touring-code/parsers/tree_sitter/.

        ## V

        - **VGP**: Verified Generation Protocol. Mandatory before generating code referencing symbols.
        - **VP-Scout**: 7-chain verification protocol for scouting agents (feature trace, dependency cycle, already implemented, homonimia, compilation evidence, test file content, wiring cache staleness).

        ## W

        - **Wave**: A unit of refactor work. The plan has 15 waves (W0..W14).
        - **Wiring**: Dependency relationship between symbols/modules/crates. Audited by `touring wiring audit`.
        - **Workspace (Cargo)**: A set of related crates managed by a single root Cargo.toml.

        ## References

        - Touring CLI ranks: `~/.claude/rules/touring-cli-index.md`
        - VP-Scout: `~/.claude/rules/VP-Scout.md`
        - Touring Decision Matrix: `~/.claude/rules/touring-decision-matrix.md`
        - Constitution v8.0: `~/.claude/rust/docs/CONSTITUTION-v8.md`
        """)
    return fm + body


