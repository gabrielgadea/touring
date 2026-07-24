//! Curated, intent-searchable catalog of the highest-value Touring tools and
//! commands (C3 — coupling backlog). Backs `touring search-tools <intent>` (CLI)
//! and the `touring_search` MCP tool: **progressive disclosure** so the LLM can
//! discover the right command from a natural-language intent instead of loading
//! every tool schema upfront (Anthropic "Tool Search" — ~−85% schema tokens).
//!
//! Design (see `docs/2026-06-26-coupling-backlog.md` C3): a small *static*
//! array — the stable, high-value command surface — ranked by a compact BM25
//! with a name/keyword field boost. Pure and unit-testable; no daemon, index, or
//! filesystem dependency, so it cannot go stale against a live index and runs in
//! microseconds. The `when_to_use` field carries the *intent* phrasing that the
//! generated `#[tool]` descriptions lack — which is what makes intent ranking
//! land on the right command.

/// One searchable tool/command entry.
#[derive(Debug, Clone, Copy)]
pub struct ToolEntry {
    /// Canonical invocation form, e.g. `"touring wiring impact <symbol> --depth 2"`.
    pub name: &'static str,
    /// Surface the entry lives on: `"cli"` or `"mcp"`.
    pub kind: &'static str,
    /// One-line description of what it does.
    pub summary: &'static str,
    /// Intent phrasing — when to reach for it (the signal `#[tool]` descs lack).
    pub when_to_use: &'static str,
    /// Extra ranking terms (synonyms the summary/when_to_use may omit).
    pub keywords: &'static [&'static str],
}

/// The curated catalog. Returns a `'static` slice so callers never allocate.
pub fn catalog() -> &'static [ToolEntry] {
    CATALOG
}

static CATALOG: &[ToolEntry] = &[
    // ── Symbol lookup & references ──────────────────────────────────────────
    ToolEntry {
        name: "touring index find <symbol> -j",
        kind: "cli",
        summary: "Exact <10ms symbol lookup with definition locations (VGP).",
        when_to_use: "find where a symbol is defined, or check whether a symbol exists before using or creating it",
        keywords: &[
            "symbol",
            "definition",
            "exists",
            "lookup",
            "where",
            "defined",
            "vgp",
        ],
    },
    ToolEntry {
        name: "touring wiring impact <symbol> --depth 2",
        kind: "cli",
        summary: "Transitive consumers of a symbol via BFS over the wiring graph.",
        when_to_use: "find who calls, uses or depends on a symbol; the blast radius of changing a function",
        keywords: &[
            "consumers",
            "callers",
            "who",
            "calls",
            "depends",
            "references",
            "blast",
            "impact",
            "uses",
        ],
    },
    ToolEntry {
        name: "touring ast find <symbol> -j",
        kind: "cli",
        summary: "A symbol's signature, kind, module path and line.",
        when_to_use: "see a symbol's signature, type or module path",
        keywords: &["signature", "type", "module", "path", "declaration"],
    },
    ToolEntry {
        name: "touring find-references <file:line:col>",
        kind: "cli",
        summary: "All references to the symbol at a cursor position (semantic).",
        when_to_use: "find every usage site of the symbol under the cursor",
        keywords: &["references", "usages", "callsites", "cursor", "occurrences"],
    },
    // ── Free-text & fuzzy search ────────────────────────────────────────────
    ToolEntry {
        name: "touring tantivy search \"<query>\"",
        kind: "cli",
        summary: "BM25-ranked full-text search over the symbol index, with snippets.",
        when_to_use: "search the codebase by free text or a phrase when you do not know the exact symbol name",
        keywords: &[
            "search", "text", "bm25", "find", "grep", "ripgrep", "query", "phrase",
        ],
    },
    ToolEntry {
        name: "touring tantivy fuzzy \"<query>\" 2",
        kind: "cli",
        summary: "Edit-distance fuzzy search — tolerant of typos.",
        when_to_use: "search when the term might be misspelled or you only remember it approximately",
        keywords: &[
            "fuzzy",
            "typo",
            "approximate",
            "misspelled",
            "edit-distance",
        ],
    },
    ToolEntry {
        name: "touring index files \"<pattern>\" --limit 200",
        kind: "cli",
        summary: "Symbol-aware file enumeration with metadata.",
        when_to_use: "list or enumerate files matching a glob instead of find/ls",
        keywords: &["files", "enumerate", "list", "glob", "find", "ls"],
    },
    // ── Pre-edit triage (file metadata first) ───────────────────────────────
    ToolEntry {
        name: "touring ast meta <file> --depth summary -j",
        kind: "cli",
        summary: "File metadata first: blast_radius, quality, cognitive, fan_in/out.",
        when_to_use: "before editing any file — gauge risk, quality and blast radius",
        keywords: &[
            "metadata",
            "blast",
            "quality",
            "cognitive",
            "before",
            "edit",
            "risk",
            "fan",
        ],
    },
    ToolEntry {
        name: "touring ast blast <file>",
        kind: "cli",
        summary: "Full dependency tree (blast radius) of a file.",
        when_to_use: "see everything a file change could ripple into before a refactor",
        keywords: &[
            "blast",
            "dependency",
            "tree",
            "ripple",
            "refactor",
            "impact",
        ],
    },
    ToolEntry {
        name: "touring ast tdg <file>",
        kind: "cli",
        summary: "Technical-debt grade (A+..F) for a file.",
        when_to_use: "check the technical-debt grade of a file; stop if D/F before editing",
        keywords: &["tdg", "grade", "debt", "quality", "score"],
    },
    ToolEntry {
        name: "touring pre-edit",
        kind: "cli",
        summary: "Composite pre-edit safety score (0-1) with CILA budget.",
        when_to_use: "gate an edit — require score >= 0.8 before changing code",
        keywords: &["pre-edit", "gate", "score", "safety", "cila", "budget"],
    },
    // ── Comprehension ───────────────────────────────────────────────────────
    ToolEntry {
        name: "touring ast overview <file> -j",
        kind: "cli",
        summary: "Structure: symbols, imports and shape of a file.",
        when_to_use: "understand a file's structure and public surface without reading every line",
        keywords: &[
            "overview",
            "structure",
            "symbols",
            "imports",
            "understand",
            "shape",
        ],
    },
    ToolEntry {
        name: "touring ast rust-semantic <file.rs>",
        kind: "cli",
        summary: "Deep Rust semantics (syn): generics, trait bounds, lifetimes, unsafe, async.",
        when_to_use: "match surrounding Rust conventions (generics, traits, lifetimes) before editing",
        keywords: &[
            "rust",
            "semantic",
            "generics",
            "traits",
            "lifetimes",
            "unsafe",
            "async",
            "syn",
        ],
    },
    ToolEntry {
        name: "touring file-knowledge extended <file>",
        kind: "cli",
        summary: "23 enriched metadata fields (community, modularity, cognitive, …).",
        when_to_use: "deep per-file analysis: community, modularity and cognitive metrics",
        keywords: &[
            "file-knowledge",
            "metadata",
            "community",
            "modularity",
            "cognitive",
            "enriched",
        ],
    },
    ToolEntry {
        name: "touring ast workspace-info",
        kind: "cli",
        summary: "cargo_metadata intel: packages, features, cross-crate dependents.",
        when_to_use: "pick a target crate or reason about cross-crate features and dependents",
        keywords: &[
            "workspace",
            "cargo",
            "packages",
            "features",
            "crates",
            "dependents",
            "metadata",
        ],
    },
    // ── Wiring & dead code (REGRA #0) ───────────────────────────────────────
    ToolEntry {
        name: "touring wiring orphans -j",
        kind: "cli",
        summary: "Public symbols with no consumers (potentialization, REGRA #0).",
        when_to_use: "find orphan/dead public symbols that should be wired or removed",
        keywords: &["orphans", "dead", "unused", "pub", "potentialize", "wiring"],
    },
    ToolEntry {
        name: "touring wiring audit -j",
        kind: "cli",
        summary: "Full wiring audit: orphans plus modules scoring below 1.0.",
        when_to_use: "audit the whole workspace's wiring health",
        keywords: &["audit", "wiring", "health", "modules", "integration"],
    },
    // ── Master workflow tools (orchestrate multiple engines in one call) ─────
    ToolEntry {
        name: "touring_audit (MCP) — path, layers",
        kind: "mcp",
        summary: "Master audit: offensive CWE/OWASP scan (10 detectors) + 6 P0 BLOCK quality dims in one call → ranked verdict.",
        when_to_use: "audit a file for security vulnerabilities, failures, gaps or problems before editing or merging",
        keywords: &[
            "audit",
            "vulnerability",
            "security",
            "cwe",
            "owasp",
            "sqli",
            "xss",
            "injection",
            "gap",
            "problem",
            "failure",
            "scan",
        ],
    },
    ToolEntry {
        name: "touring wiring cycles --min-depth 2",
        kind: "cli",
        summary: "Dependency cycle detection (Tarjan SCC).",
        when_to_use: "detect dependency cycles between modules or crates",
        keywords: &["cycles", "cycle", "circular", "tarjan", "scc", "dependency"],
    },
    ToolEntry {
        name: "touring wiring chains",
        kind: "cli",
        summary: "Functional source→sink module relationships.",
        when_to_use: "understand the call/flow chain from an entry point to a sink",
        keywords: &["chains", "flow", "source", "sink", "call", "graph", "path"],
    },
    ToolEntry {
        name: "touring wiring suggest --top 20 -j",
        kind: "cli",
        summary: "Auto-wire suggestions for orphan symbols.",
        when_to_use: "get suggestions for wiring a new or orphan symbol to consumers",
        keywords: &["suggest", "auto-wire", "wire", "orphan", "connect"],
    },
    // ── System health & diagnostics ─────────────────────────────────────────
    ToolEntry {
        name: "touring doctor -j",
        kind: "cli",
        summary: "Daemon + index health gate (5 components).",
        when_to_use: "check system or daemon health before a critical action",
        keywords: &[
            "doctor",
            "health",
            "daemon",
            "diagnostic",
            "status",
            "check",
        ],
    },
    ToolEntry {
        name: "touring status -j",
        kind: "cli",
        summary: "Unified dashboard: index, orphans, RL, composite_health_score.",
        when_to_use: "get a quick dashboard of index, wiring and RL state",
        keywords: &[
            "status",
            "dashboard",
            "health",
            "overview",
            "metrics",
            "composite",
        ],
    },
    ToolEntry {
        name: "touring e2e -j",
        kind: "cli",
        summary: "Composite end-to-end system score (0-1).",
        when_to_use: "validate overall system health before a risky change",
        keywords: &["e2e", "composite", "score", "health", "system", "validate"],
    },
    ToolEntry {
        name: "touring gate-metrics -j",
        kind: "cli",
        summary: "Live gate/CEG counters (captured, sandboxed, blocked, …).",
        when_to_use: "inspect live hook, CEG and gate counters",
        keywords: &[
            "gate-metrics",
            "counters",
            "ceg",
            "metrics",
            "telemetry",
            "live",
        ],
    },
    // ── Memory & pitfalls (history without git) ─────────────────────────────
    ToolEntry {
        name: "touring memory recall \"<query>\"",
        kind: "cli",
        summary: "Recall past lessons, decisions and outcomes (substitutes git log).",
        when_to_use: "recall how something was solved before, or past decisions and history",
        keywords: &[
            "memory", "recall", "history", "past", "lesson", "decision", "log", "remember",
        ],
    },
    ToolEntry {
        name: "touring memory store --tier semantic <key> \"<state>\"",
        kind: "cli",
        summary: "Persist a lesson/decision/state to memory.",
        when_to_use: "persist a lesson learned or snapshot state before a risky refactor",
        keywords: &[
            "memory",
            "store",
            "persist",
            "lesson",
            "snapshot",
            "checkpoint",
            "save",
        ],
    },
    ToolEntry {
        name: "touring gotcha match <file>",
        kind: "cli",
        summary: "Known pitfalls (gotcha DB) for a file or pattern.",
        when_to_use: "check for known pitfalls before editing or when an error repeats",
        keywords: &["gotcha", "pitfall", "known", "error", "trap", "warning"],
    },
    // ── Decomposition & planning ────────────────────────────────────────────
    ToolEntry {
        name: "touring decompose create <type> \"<desc>\"",
        kind: "cli",
        summary: "Create a validated task DAG (native, not a todo list).",
        when_to_use: "break a multi-step task into a dependency DAG",
        keywords: &[
            "decompose",
            "dag",
            "task",
            "plan",
            "breakdown",
            "subtasks",
            "steps",
        ],
    },
    ToolEntry {
        name: "touring decompose add <task_id> <subtask_id> \"<desc>\" --depends-on=<ids>",
        kind: "cli",
        summary: "Add a subtask node with dependencies to a DAG.",
        when_to_use: "add a node and its dependencies to an existing task DAG",
        keywords: &["decompose", "add", "node", "subtask", "depends", "dag"],
    },
    ToolEntry {
        name: "touring decompose create plan \"<intent>\"",
        kind: "cli",
        summary: "Create a task DAG for a non-trivial implementation (use /plan or the taco-planning skill for the prose plan).",
        when_to_use: "plan a non-trivial implementation (3+ steps or an architectural decision)",
        keywords: &[
            "plan",
            "planning",
            "roadmap",
            "implementation",
            "design",
            "quality",
        ],
    },
    // ── Code creation & editing (blast + pre-edit gate) ─────────────────────
    ToolEntry {
        name: "touring generate verify --symbol <name>",
        kind: "cli",
        summary: "VGP-verify a symbol before creating it; then create the file with the Write tool.",
        when_to_use: "create a new code file (.rs/.py/.ts) — verify collisions first",
        keywords: &["create", "new", "file", "generate", "scaffold", "module"],
    },
    ToolEntry {
        name: "touring pre-edit + touring ast blast <file>",
        kind: "cli",
        summary: "Gate an edit (blast radius + pre-edit score >= 0.8) before applying it with the Edit tool.",
        when_to_use: "edit existing code (rename, rewrite, refactor) with a safety gate",
        keywords: &["edit", "modify", "rename", "rewrite", "refactor", "change"],
    },
    // ── Compute-in-code (Code Mode) ─────────────────────────────────────────
    ToolEntry {
        name: "touring_ctx_execute language=python code='<code>'",
        kind: "mcp",
        summary: "Run sandboxed code once — the 30-200× compression alternative to N tool calls.",
        when_to_use: "count, filter or aggregate across many files in one pass instead of repeated grep/Read",
        keywords: &[
            "ctx_execute",
            "code",
            "sandbox",
            "compute",
            "aggregate",
            "count",
            "filter",
            "loop",
            "batch",
            "script",
        ],
    },
    ToolEntry {
        name: "touring inferlets run <name>",
        kind: "cli",
        summary: "Run a saved WASM inferlet for a recurring aggregation (200× compression).",
        when_to_use: "run a recurring count/aggregation as a saved WASM inferlet",
        keywords: &["inferlet", "wasm", "aggregate", "recurring", "compute"],
    },
    // ── RL feedback ─────────────────────────────────────────────────────────
    ToolEntry {
        name: "touring learning reward <tool> <value> \"<context>\"",
        kind: "cli",
        summary: "Inject an RL reward after a measurable outcome.",
        when_to_use: "close the RL loop after a wave succeeds or fails",
        keywords: &["learning", "reward", "rl", "feedback", "outcome"],
    },
    ToolEntry {
        name: "touring learning status",
        kind: "cli",
        summary: "RL status: LinUCB arms, EMA reward, convergence.",
        when_to_use: "inspect RL convergence and bandit state",
        keywords: &[
            "learning",
            "status",
            "rl",
            "bandit",
            "linucb",
            "convergence",
            "ema",
        ],
    },
    // ── Discovery (this tool) ───────────────────────────────────────────────
    ToolEntry {
        name: "touring search-tools \"<intent>\"",
        kind: "cli",
        summary: "Discover the right Touring tool/command from a natural-language intent.",
        when_to_use: "you do not know which touring command fits — describe the intent and get ranked tools",
        keywords: &[
            "search-tools",
            "discover",
            "which",
            "tool",
            "command",
            "intent",
            "help",
        ],
    },
];

// ── Intent ranking (compact BM25 + name/keyword field boost) ─────────────────

/// A catalog hit with its BM25 relevance score.
#[derive(Debug, Clone, Copy)]
pub struct ScoredEntry {
    /// The matched catalog entry.
    pub entry: &'static ToolEntry,
    /// BM25 score (with name/keyword boost); higher is more relevant.
    pub score: f64,
}

/// BM25 term-frequency saturation parameter.
const BM25_K1: f64 = 1.2;
/// BM25 length-normalization parameter.
const BM25_B: f64 = 0.75;
/// Multiplier when a query term matches the entry's name or keyword list — a
/// name/keyword hit is a stronger tool-selection signal than a prose hit.
const FIELD_BOOST: f64 = 2.0;

/// Tokenize free text into lowercase alphanumeric terms of length ≥ 2, dropping
/// function-word stopwords that carry no tool-selection signal.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| !is_stopword(t))
        .collect()
}

/// Function-word stopwords with no tool-selection signal. Intentionally narrow —
/// verbs like `find`/`search`/`list` stay, since they map to real commands.
fn is_stopword(t: &str) -> bool {
    matches!(
        t,
        "the"
            | "a"
            | "an"
            | "of"
            | "to"
            | "in"
            | "on"
            | "for"
            | "and"
            | "or"
            | "is"
            | "are"
            | "with"
            | "how"
            | "do"
            | "does"
            | "did"
            | "me"
            | "my"
            | "want"
            | "need"
            | "all"
            | "that"
            | "this"
            | "it"
            | "be"
            | "can"
    )
}

/// The searchable document for an entry: name + summary + when_to_use + keywords.
fn entry_doc_tokens(e: &ToolEntry) -> Vec<String> {
    let mut s = String::with_capacity(128);
    s.push_str(e.name);
    s.push(' ');
    s.push_str(e.summary);
    s.push(' ');
    s.push_str(e.when_to_use);
    for kw in e.keywords {
        s.push(' ');
        s.push_str(kw);
    }
    tokenize(&s)
}

/// True iff `term` matches the entry's name or one of its keywords (field boost).
fn term_in_name_or_keywords(e: &ToolEntry, term: &str) -> bool {
    e.name.to_ascii_lowercase().contains(term)
        || e.keywords.iter().any(|kw| kw.eq_ignore_ascii_case(term))
}

/// BM25 score of one document against the query `terms`, with the name/keyword
/// field boost. Split out of [`search_catalog`] so the hot loop stays within the
/// complexity gate and the scoring is independently testable. `df[t]` is the
/// document frequency of `terms[t]`, `n` the corpus size, `avgdl` the mean doc
/// length.
fn bm25_score(
    entry: &ToolEntry,
    doc: &[String],
    terms: &[String],
    df: &[f64],
    n: f64,
    avgdl: f64,
) -> f64 {
    let dl = doc.len() as f64;
    let mut score = 0.0_f64;
    for (t, term) in terms.iter().enumerate() {
        let tf = doc.iter().filter(|w| *w == term).count() as f64;
        if df[t] == 0.0 || tf == 0.0 {
            continue;
        }
        let idf = (1.0 + (n - df[t] + 0.5) / (df[t] + 0.5)).ln();
        let norm = tf * (BM25_K1 + 1.0) / (tf + BM25_K1 * (1.0 - BM25_B + BM25_B * dl / avgdl));
        let mut term_score = idf * norm;
        if term_in_name_or_keywords(entry, term) {
            term_score *= FIELD_BOOST;
        }
        score += term_score;
    }
    score
}

/// Rank the curated catalog against a natural-language `intent`, returning up to
/// `top_k` entries with score > 0, highest score first. Pure: compact BM25 over
/// each entry's (name + summary + when_to_use + keywords) with a name/keyword
/// field boost. Ties keep catalog order (stable sort).
pub fn search_catalog(intent: &str, top_k: usize) -> Vec<ScoredEntry> {
    let terms = tokenize(intent);
    if terms.is_empty() || top_k == 0 {
        return Vec::new();
    }
    let docs: Vec<Vec<String>> = CATALOG.iter().map(entry_doc_tokens).collect();
    let n = docs.len() as f64;
    let total_len: usize = docs.iter().map(Vec::len).sum();
    let avgdl = if total_len == 0 {
        1.0
    } else {
        total_len as f64 / n
    };

    // Document frequency per query term over the catalog.
    let df: Vec<f64> = terms
        .iter()
        .map(|term| docs.iter().filter(|d| d.iter().any(|w| w == term)).count() as f64)
        .collect();

    let mut scored: Vec<ScoredEntry> = CATALOG
        .iter()
        .enumerate()
        .map(|(i, entry)| ScoredEntry {
            entry,
            score: bm25_score(entry, &docs[i], &terms, &df, n, avgdl),
        })
        .filter(|s| s.score > 0.0)
        .collect();

    // Sort by score desc; stable so equal scores keep catalog order.
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(top_k);
    scored
}

/// Run [`search_catalog`] and render the hits as a JSON value
/// `{ "intent", "count", "results":[{name,kind,summary,when_to_use,score}] }`.
/// Shared by the CLI (`search-tools -j`) and the `touring_search` MCP tool so
/// both surfaces emit an identical shape.
pub fn search_as_json(intent: &str, top_k: usize) -> serde_json::Value {
    let hits = search_catalog(intent, top_k);
    serde_json::json!({
        "intent": intent,
        "count": hits.len(),
        "results": hits.iter().map(|h| serde_json::json!({
            "name": h.entry.name,
            "kind": h.entry.kind,
            "summary": h.entry.summary,
            "when_to_use": h.entry.when_to_use,
            // Round to 3 dp — the absolute scale is meaningless; the order is what matters.
            "score": (h.score * 1000.0).round() / 1000.0,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_non_empty_and_well_formed() {
        let c = catalog();
        assert!(c.len() >= 30, "catalog should cover the high-value surface");
        for e in c {
            assert!(
                e.name.starts_with("touring"),
                "unexpected tool name: {}",
                e.name
            );
            assert!(e.kind == "cli" || e.kind == "mcp", "kind: {}", e.kind);
            assert!(!e.summary.is_empty() && !e.when_to_use.is_empty());
        }
    }

    #[test]
    fn tokenize_drops_stopwords_and_short_tokens() {
        let t = tokenize("How do I find the consumers of a symbol?");
        assert!(t.contains(&"find".to_string()));
        assert!(t.contains(&"consumers".to_string()));
        assert!(t.contains(&"symbol".to_string()));
        // Stopwords + 1-char tokens are dropped.
        assert!(!t.contains(&"how".to_string()));
        assert!(!t.contains(&"the".to_string()));
        assert!(!t.contains(&"a".to_string()));
        assert!(!t.contains(&"i".to_string()));
    }

    #[test]
    fn empty_or_stopword_only_intent_yields_nothing() {
        assert!(search_catalog("", 5).is_empty());
        assert!(search_catalog("the a of to", 5).is_empty());
        assert!(search_catalog("find consumers", 0).is_empty());
    }

    #[test]
    fn ranks_consumers_intent_to_wiring_impact() {
        let hits = search_catalog("find the consumers of a symbol", 5);
        assert!(!hits.is_empty());
        assert!(
            hits[0].entry.name.contains("wiring impact"),
            "top hit was {}",
            hits[0].entry.name
        );
    }

    #[test]
    fn ranks_definition_intent_to_index_find() {
        let hits = search_catalog("where is this symbol defined", 5);
        assert!(!hits.is_empty());
        assert!(
            hits[0].entry.name.contains("index find"),
            "top hit was {}",
            hits[0].entry.name
        );
    }

    #[test]
    fn ranks_aggregation_intent_to_code_mode() {
        let hits = search_catalog("count and aggregate matches across many files", 5);
        assert!(!hits.is_empty());
        assert!(
            hits.iter().any(|h| h.entry.name.contains("ctx_execute")),
            "ctx_execute should surface for aggregation intents"
        );
    }

    #[test]
    fn respects_top_k_and_descending_order() {
        let hits = search_catalog("search code health wiring symbol", 3);
        assert!(hits.len() <= 3);
        for w in hits.windows(2) {
            assert!(w[0].score >= w[1].score, "scores must be descending");
        }
    }

    #[test]
    fn search_as_json_has_expected_shape() {
        let v = search_as_json("check system health", 4);
        assert_eq!(v["intent"], "check system health");
        assert!(v["count"].as_u64().unwrap() >= 1);
        let results = v["results"].as_array().expect("results array");
        assert!(!results.is_empty());
        let top = &results[0];
        assert!(top["name"].is_string());
        assert!(top["kind"].is_string());
        assert!(top["summary"].is_string());
        assert!(top["when_to_use"].is_string());
        assert!(top["score"].is_number());
    }
}
