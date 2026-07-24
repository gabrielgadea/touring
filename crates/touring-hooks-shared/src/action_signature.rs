//! ActionSignature — normalized `(tool_class, intent_class, context_qualifier)` key
//! for action-scoped outcome persistence.
//!
//! ## Purpose
//!
//! Enables the PreToolUse (`cli_suggester`) and PostToolUse (`post_tool_rl`) hooks to
//! share a common key for reading and writing action-outcome memory. Instead of keying
//! on raw tool names ("Bash", "Edit") — which are too coarse — or on exact file paths —
//! which are too narrow — `ActionSignature` normalizes to three dimensions:
//!
//! - **tool_class**: coarse grouping (`bash`, `edit`, `write`, `read`, `search`,
//!   `task`, `web`, `mcp`, `other`).
//! - **intent_class**: what is being operated on (`cargo` for `cargo check`, `rs` for
//!   `.rs` files, `symbol` for grep of an identifier, etc.).
//! - **context_qualifier**: highest-priority risk flag (`hi-blast`, `hi-complexity`,
//!   `gotcha-active`, `no-index`, `new-symbol`, or `plain`).
//!
//! The storage key is `outcome:<tool_class>:<intent_class>:<context_qualifier>` — directly
//! usable with `touring memory recall "outcome:<sig>"`.
//!
//! ## Fail-open guarantee
//!
//! All constructors return a valid `ActionSignature` even when inputs are missing or
//! malformed — they fall back to class `other` / intent `unknown` / qualifier `plain`.
//! No constructor panics, unwraps, or propagates errors to the caller.

use serde_json::Value;

/// Priority-ordered context qualifier flags.
///
/// When multiple flags apply, the highest-priority one wins (leftmost in this enum).
///
/// Priority order (highest first):
///   `HiBlast` > `HiComplexity` > `GotchaActive` > `NoIndex` > `NewSymbol` > `Plain`
///
/// Rationale: `HiBlast` (structural coupling) outranks everything because blast-radius
/// changes have the broadest impact regardless of per-file complexity. `HiComplexity`
/// ranks second — a high cognitive-score file warrants extra care even when well-wired.
/// `GotchaActive` follows because known pitfalls are actionable but file-scoped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextQualifier {
    /// File has more than 10 dependents (callers). Highest priority.
    HiBlast,
    /// File has high cognitive complexity score (> 0.7). Second priority.
    /// Populated when `EnrichmentData::cognitive_score` is available.
    HiComplexity,
    /// File gotcha matches present — known pitfalls.
    GotchaActive,
    /// File is not in the Touring index.
    NoIndex,
    /// Symbol is new (not yet in the index).
    NewSymbol,
    /// No notable risk flags.
    Plain,
}

impl ContextQualifier {
    /// Returns the canonical string representation used in storage keys.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HiBlast => "hi-blast",
            Self::HiComplexity => "hi-complexity",
            Self::GotchaActive => "gotcha-active",
            Self::NoIndex => "no-index",
            Self::NewSymbol => "new-symbol",
            Self::Plain => "plain",
        }
    }

    /// Compute qualifier from enrichment signals.
    ///
    /// Priority: hi-blast > hi-complexity > gotcha-active > no-index > new-symbol > plain.
    ///
    /// - `dependent_count`: number of inverse dependents for the file.
    /// - `cognitive_score`: optional f32 in [0.0, 1.0]; threshold > 0.7 → `HiComplexity`.
    /// - `gotcha_count`: number of gotcha matches for the file.
    /// - `file_is_indexed`: `Some(false)` → `NoIndex`.
    /// - `symbol_in_index`: `Some(false)` → `NewSymbol`.
    ///
    /// All parameters are optional / zero-defaulted — never panics.
    pub fn from_enrichment(
        dependent_count: Option<u32>,
        gotcha_count: usize,
        file_is_indexed: Option<bool>,
        symbol_in_index: Option<bool>,
    ) -> Self {
        Self::from_enrichment_with_cognitive(
            dependent_count,
            None,
            gotcha_count,
            file_is_indexed,
            symbol_in_index,
        )
    }

    /// Extended constructor that also accepts an optional `cognitive_score`.
    ///
    /// Prefer this over `from_enrichment` when `EnrichmentData::cognitive_score`
    /// is available (Slice 3 and later).
    pub fn from_enrichment_with_cognitive(
        dependent_count: Option<u32>,
        cognitive_score: Option<f32>,
        gotcha_count: usize,
        file_is_indexed: Option<bool>,
        symbol_in_index: Option<bool>,
    ) -> Self {
        if dependent_count.unwrap_or(0) > 10 {
            return Self::HiBlast;
        }
        if cognitive_score.unwrap_or(0.0) > 0.7 {
            return Self::HiComplexity;
        }
        if gotcha_count > 0 {
            return Self::GotchaActive;
        }
        if file_is_indexed == Some(false) {
            return Self::NoIndex;
        }
        if symbol_in_index == Some(false) {
            return Self::NewSymbol;
        }
        Self::Plain
    }
}

/// Normalized action signature: `(tool_class, intent_class, context_qualifier)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSignature {
    /// Coarse tool grouping.
    pub tool_class: String,
    /// Intent derived from the tool input.
    pub intent_class: String,
    /// Highest-priority risk qualifier.
    pub context_qualifier: ContextQualifier,
}

impl ActionSignature {
    /// Returns the storage key: `outcome:<tool_class>:<intent_class>:<context_qualifier>`.
    pub fn to_key(&self) -> String {
        format!(
            "outcome:{}:{}:{}",
            self.tool_class,
            self.intent_class,
            self.context_qualifier.as_str()
        )
    }

    /// Construct from the PreToolUse side (tool name + raw input JSON).
    ///
    /// Enrichment signals come from `EnrichmentData` (already computed by `enrich()`).
    /// Pass `None` / `0.0` / `0` / `None` / `None` when enrichment is not available.
    ///
    /// `cognitive_score` is `Some(f32)` when `EnrichmentData::cognitive_score` is
    /// populated (Slice 3+). When `None`, the `HiComplexity` qualifier is never set.
    ///
    /// Never panics. Always returns a valid signature.
    pub fn from_pre_tool(
        tool_name: &str,
        tool_input: &Value,
        dependent_count: Option<u32>,
        gotcha_count: usize,
        file_is_indexed: Option<bool>,
        symbol_in_index: Option<bool>,
    ) -> Self {
        Self::from_pre_tool_with_cognitive(
            tool_name,
            tool_input,
            dependent_count,
            None,
            gotcha_count,
            file_is_indexed,
            symbol_in_index,
        )
    }

    /// Extended PreToolUse constructor that also accepts `cognitive_score`.
    ///
    /// Use this when `EnrichmentData::cognitive_score` is available (Slice 3+).
    /// Backwards-compatible: `from_pre_tool` delegates here with `cognitive_score = None`.
    ///
    /// Never panics. Always returns a valid signature.
    pub fn from_pre_tool_with_cognitive(
        tool_name: &str,
        tool_input: &Value,
        dependent_count: Option<u32>,
        cognitive_score: Option<f32>,
        gotcha_count: usize,
        file_is_indexed: Option<bool>,
        symbol_in_index: Option<bool>,
    ) -> Self {
        let tool_class = classify_tool_class(tool_name);
        let intent_class = classify_intent_class(&tool_class, tool_name, tool_input);
        let context_qualifier = ContextQualifier::from_enrichment_with_cognitive(
            dependent_count,
            cognitive_score,
            gotcha_count,
            file_is_indexed,
            symbol_in_index,
        );
        Self {
            tool_class,
            intent_class,
            context_qualifier,
        }
    }

    /// Construct from the PostToolUse side (tool name, optional file_path, has_error flag).
    ///
    /// Enrichment is not available post-side, so qualifier defaults based on
    /// whether the file_path is present. This produces a key compatible with the
    /// pre-side key so outcomes can be retrieved by action class.
    ///
    /// Never panics. Always returns a valid signature.
    pub fn from_post_tool(tool_name: &str, file_path: &str, has_error: bool) -> Self {
        let tool_class = classify_tool_class(tool_name);
        let fake_input = if !file_path.is_empty() {
            serde_json::json!({ "file_path": file_path })
        } else {
            serde_json::json!({})
        };
        let intent_class = classify_intent_class(&tool_class, tool_name, &fake_input);
        // Post-side: no enrichment available. Use `no-index` as a proxy when a file
        // is involved but errored, otherwise `plain`.
        let context_qualifier = if has_error && !file_path.is_empty() {
            ContextQualifier::GotchaActive
        } else {
            ContextQualifier::Plain
        };
        Self {
            tool_class,
            intent_class,
            context_qualifier,
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Map tool name → coarse tool_class string.
///
/// Public: the transcript miner (`touring-server::ingest::transcript_miner`)
/// reuses this so mined-lesson memory keys share the exact same `tool_class`
/// segment that `cli_suggester` queries — the writer↔reader contract.
pub fn classify_tool_class(tool_name: &str) -> String {
    match tool_name {
        "Bash" => "bash",
        "Edit" | "NotebookEdit" => "edit",
        "Write" => "write",
        "Read" => "read",
        "Grep" | "Glob" => "search",
        "Task" | "TodoWrite" => "task",
        "WebFetch" | "WebSearch" => "web",
        other if other.starts_with("mcp__") => "mcp",
        _ => "other",
    }
    .to_string()
}

/// Extract the intent_class from the tool_class + raw input.
///
/// Rules:
/// - `bash`: first command token (e.g. `cargo check` → `cargo`).
/// - `edit` / `write` / `read`: file extension (e.g. `.rs` → `rs`; no ext → `unknown`).
/// - `search`: `symbol` when pattern is a clean identifier, else `free-text`.
/// - `task` / `web` / `mcp`: tool name as-is (already descriptive).
/// - `other`: `unknown`.
fn classify_intent_class(tool_class: &str, tool_name: &str, input: &Value) -> String {
    match tool_class {
        "bash" => {
            let cmd = input
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            first_command_token(cmd).unwrap_or_else(|| "unknown".to_string())
        }
        "edit" | "write" | "read" => {
            let file = input
                .get("file_path")
                .or_else(|| input.get("notebook_path"))
                .or_else(|| input.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            file_extension(file).unwrap_or_else(|| "unknown".to_string())
        }
        "search" => {
            let pattern = input
                .get("pattern")
                .or_else(|| input.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if is_clean_identifier(pattern) {
                "symbol".to_string()
            } else {
                "free-text".to_string()
            }
        }
        "task" | "web" | "mcp" => {
            // Use tool name as intent (already meaningful: e.g. `WebFetch`, `mcp__touring__memory_store`)
            sanitize_intent(tool_name)
        }
        _ => "unknown".to_string(),
    }
}

/// Extract the first whitespace-delimited token from a shell command.
fn first_command_token(cmd: &str) -> Option<String> {
    let tok = cmd.split_whitespace().next()?;
    // Strip leading path components (e.g. `/usr/bin/cargo` → `cargo`).
    let base = tok.split('/').next_back().unwrap_or(tok);
    if base.is_empty() {
        None
    } else {
        Some(base.to_string())
    }
}

/// Extract the file extension (without the dot), lowercased.
fn file_extension(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
}

/// True iff `s` is a clean identifier (no whitespace or regex meta-chars, length 3-80,
/// alphanumeric + underscores only).
fn is_clean_identifier(s: &str) -> bool {
    if s.len() < 3 || s.len() > 80 {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true)
}

/// Produce a safe intent string from a tool name: lowercase, replace non-alphanumeric
/// characters (including underscores) with `-`, collapse consecutive dashes, trim
/// leading/trailing dashes, truncate to 40 chars.
fn sanitize_intent(name: &str) -> String {
    // Map every non-alphanumeric char (including `_`) to `-`, then collapse runs.
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else {
            // Replace any separator (_, -, __, space, etc.) with a single `-`.
            if !prev_dash && !out.is_empty() {
                out.push('-');
            }
            prev_dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-');
    trimmed.chars().take(40).collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── tool_class mapping ─────────────────────────────────────────────────────

    #[test]
    fn tool_class_bash() {
        assert_eq!(classify_tool_class("Bash"), "bash");
    }

    #[test]
    fn tool_class_edit() {
        assert_eq!(classify_tool_class("Edit"), "edit");
        assert_eq!(classify_tool_class("NotebookEdit"), "edit");
    }

    #[test]
    fn tool_class_write() {
        assert_eq!(classify_tool_class("Write"), "write");
    }

    #[test]
    fn tool_class_read() {
        assert_eq!(classify_tool_class("Read"), "read");
    }

    #[test]
    fn tool_class_search() {
        assert_eq!(classify_tool_class("Grep"), "search");
        assert_eq!(classify_tool_class("Glob"), "search");
    }

    #[test]
    fn tool_class_task() {
        assert_eq!(classify_tool_class("Task"), "task");
        assert_eq!(classify_tool_class("TodoWrite"), "task");
    }

    #[test]
    fn tool_class_web() {
        assert_eq!(classify_tool_class("WebFetch"), "web");
        assert_eq!(classify_tool_class("WebSearch"), "web");
    }

    #[test]
    fn tool_class_mcp() {
        assert_eq!(classify_tool_class("mcp__touring__memory_store"), "mcp");
        assert_eq!(classify_tool_class("mcp__context7__get_docs"), "mcp");
    }

    #[test]
    fn tool_class_other() {
        assert_eq!(classify_tool_class("UnknownTool"), "other");
        assert_eq!(classify_tool_class(""), "other");
    }

    // ── intent_class mapping ───────────────────────────────────────────────────

    #[test]
    fn intent_bash_cargo_check() {
        // Strategy Part 2 example: `cargo check` → `(bash, cargo, plain)`
        let input = json!({"command": "cargo check -p touring-hooks"});
        let intent = classify_intent_class("bash", "Bash", &input);
        assert_eq!(intent, "cargo");
    }

    #[test]
    fn intent_bash_first_token_absolute_path() {
        let input = json!({"command": "/usr/bin/cargo build"});
        let intent = classify_intent_class("bash", "Bash", &input);
        assert_eq!(intent, "cargo");
    }

    #[test]
    fn intent_bash_empty_command() {
        let input = json!({"command": ""});
        let intent = classify_intent_class("bash", "Bash", &input);
        assert_eq!(intent, "unknown");
    }

    #[test]
    fn intent_edit_rust_file() {
        // Strategy Part 2 example: `Edit big_file.rs` → intent `rs`
        let input = json!({"file_path": "crates/touring-hooks/src/big_file.rs"});
        let intent = classify_intent_class("edit", "Edit", &input);
        assert_eq!(intent, "rs");
    }

    #[test]
    fn intent_read_plan_md() {
        // Strategy Part 2 example: `Read docs/plan.md` → `(read, md, plain)`
        let input = json!({"file_path": "docs/plan.md"});
        let intent = classify_intent_class("read", "Read", &input);
        assert_eq!(intent, "md");
    }

    #[test]
    fn intent_write_tsx_file() {
        // Strategy Part 2 example: `Write Button.tsx` not indexed → `(write, tsx, no-index)`
        let input = json!({"file_path": "src/Button.tsx"});
        let intent = classify_intent_class("write", "Write", &input);
        assert_eq!(intent, "tsx");
    }

    #[test]
    fn intent_search_symbol() {
        // Strategy Part 2 example: `Grep "HookRuntime"` → `(search, symbol, plain)`
        let input = json!({"pattern": "HookRuntime"});
        let intent = classify_intent_class("search", "Grep", &input);
        assert_eq!(intent, "symbol");
    }

    #[test]
    fn intent_search_free_text() {
        let input = json!({"pattern": "TODO fix the broken handler"});
        let intent = classify_intent_class("search", "Grep", &input);
        assert_eq!(intent, "free-text");
    }

    #[test]
    fn intent_mcp_tool_name() {
        // sanitize_intent collapses `__` runs to a single `-`
        let intent = classify_intent_class("mcp", "mcp__touring__memory_store", &json!({}));
        assert_eq!(intent, "mcp-touring-memory-store");
    }

    #[test]
    fn intent_web_fetch() {
        let intent = classify_intent_class("web", "WebFetch", &json!({}));
        assert_eq!(intent, "webfetch");
    }

    // ── context_qualifier ─────────────────────────────────────────────────────

    #[test]
    fn qualifier_hi_blast_wins_over_gotcha() {
        let q = ContextQualifier::from_enrichment(Some(11), 3, None, None);
        assert_eq!(q, ContextQualifier::HiBlast);
    }

    #[test]
    fn qualifier_gotcha_active() {
        let q = ContextQualifier::from_enrichment(Some(5), 2, None, None);
        assert_eq!(q, ContextQualifier::GotchaActive);
    }

    #[test]
    fn qualifier_no_index() {
        let q = ContextQualifier::from_enrichment(None, 0, Some(false), None);
        assert_eq!(q, ContextQualifier::NoIndex);
    }

    #[test]
    fn qualifier_new_symbol() {
        let q = ContextQualifier::from_enrichment(None, 0, None, Some(false));
        assert_eq!(q, ContextQualifier::NewSymbol);
    }

    #[test]
    fn qualifier_plain_when_all_ok() {
        let q = ContextQualifier::from_enrichment(Some(3), 0, Some(true), Some(true));
        assert_eq!(q, ContextQualifier::Plain);
    }

    #[test]
    fn qualifier_plain_all_none() {
        let q = ContextQualifier::from_enrichment(None, 0, None, None);
        assert_eq!(q, ContextQualifier::Plain);
    }

    // ── to_key() ──────────────────────────────────────────────────────────────

    #[test]
    fn to_key_cargo_check_plain() {
        // Strategy Part 2: `cargo check` → `outcome:bash:cargo:plain`
        let sig = ActionSignature::from_pre_tool(
            "Bash",
            &json!({"command": "cargo check --workspace"}),
            None,
            0,
            None,
            None,
        );
        assert_eq!(sig.to_key(), "outcome:bash:cargo:plain");
    }

    #[test]
    fn to_key_edit_rs_hi_blast_gotcha() {
        // Strategy Part 2: `Edit big_file.rs` (17 deps, 2 gotchas) → hi-blast wins
        let sig = ActionSignature::from_pre_tool(
            "Edit",
            &json!({"file_path": "crates/foo/src/big_file.rs"}),
            Some(17), // dependent_count > 10 → hi-blast
            2,        // gotchas (but hi-blast wins)
            Some(true),
            None,
        );
        assert_eq!(sig.to_key(), "outcome:edit:rs:hi-blast");
    }

    #[test]
    fn to_key_grep_hookruntime_plain() {
        // Strategy Part 2: `Grep "HookRuntime"` → `outcome:search:symbol:plain`
        let sig = ActionSignature::from_pre_tool(
            "Grep",
            &json!({"pattern": "HookRuntime"}),
            None,
            0,
            None,
            Some(true),
        );
        assert_eq!(sig.to_key(), "outcome:search:symbol:plain");
    }

    #[test]
    fn to_key_read_plan_md_plain() {
        // Strategy Part 2: `Read docs/plan.md` → `outcome:read:md:plain`
        let sig = ActionSignature::from_pre_tool(
            "Read",
            &json!({"file_path": "docs/plan.md"}),
            None,
            0,
            None,
            None,
        );
        assert_eq!(sig.to_key(), "outcome:read:md:plain");
    }

    #[test]
    fn to_key_write_tsx_no_index() {
        // Strategy Part 2: `Write Button.tsx` not indexed → `outcome:write:tsx:no-index`
        let sig = ActionSignature::from_pre_tool(
            "Write",
            &json!({"file_path": "src/Button.tsx"}),
            None,
            0,
            Some(false), // not in index → no-index
            None,
        );
        assert_eq!(sig.to_key(), "outcome:write:tsx:no-index");
    }

    // ── from_post_tool ────────────────────────────────────────────────────────

    #[test]
    fn from_post_tool_success_plain() {
        let sig = ActionSignature::from_post_tool("Edit", "src/foo.rs", false);
        assert_eq!(sig.tool_class, "edit");
        assert_eq!(sig.intent_class, "rs");
        assert_eq!(sig.context_qualifier, ContextQualifier::Plain);
    }

    #[test]
    fn from_post_tool_error_sets_gotcha_active() {
        let sig = ActionSignature::from_post_tool("Bash", "src/main.rs", true);
        assert_eq!(sig.tool_class, "bash");
        // Bash intent comes from file_path-based fake input — extension `rs`
        // but tool_class bash → reads command from "command" key, not file_path
        // so intent will be "unknown" (no command in the fake input)
        assert_eq!(sig.context_qualifier, ContextQualifier::GotchaActive);
    }

    #[test]
    fn from_post_tool_no_file_plain() {
        let sig = ActionSignature::from_post_tool("Task", "", false);
        assert_eq!(sig.tool_class, "task");
        assert_eq!(sig.context_qualifier, ContextQualifier::Plain);
    }

    // ── key stability ─────────────────────────────────────────────────────────

    #[test]
    fn key_prefix_is_outcome() {
        let sig = ActionSignature::from_pre_tool(
            "Read",
            &json!({"file_path": "a.rs"}),
            None,
            0,
            None,
            None,
        );
        assert!(sig.to_key().starts_with("outcome:"));
    }

    #[test]
    fn sanitize_intent_mcp_name() {
        let s = sanitize_intent("mcp__touring__memory_store");
        // double underscores collapse to a single `-`
        assert_eq!(s, "mcp-touring-memory-store");
        assert!(s.len() <= 40);
    }

    // ── HiComplexity (Slice 3 Part 2) ────────────────────────────────────────

    #[test]
    fn qualifier_hi_complexity_above_threshold() {
        // cognitive_score = 0.71 (strictly > 0.7) → HiComplexity
        let q = ContextQualifier::from_enrichment_with_cognitive(
            None,       // no dependents — HiBlast does NOT fire
            Some(0.71), // cognitive_score
            0,          // gotcha_count
            None,       // file_is_indexed
            None,       // symbol_in_index
        );
        assert_eq!(q, ContextQualifier::HiComplexity);
    }

    #[test]
    fn qualifier_hi_complexity_at_exact_threshold() {
        // cognitive_score == 0.7 is NOT strictly > 0.7 → Plain
        let q = ContextQualifier::from_enrichment_with_cognitive(None, Some(0.7), 0, None, None);
        assert_eq!(q, ContextQualifier::Plain);
    }

    #[test]
    fn qualifier_hi_blast_outranks_hi_complexity() {
        // dependent_count = 11 (> 10) AND cognitive_score = 0.9 → HiBlast wins
        let q = ContextQualifier::from_enrichment_with_cognitive(
            Some(11),  // blast — should win
            Some(0.9), // also hi-complexity, but lower priority
            0,
            None,
            None,
        );
        assert_eq!(q, ContextQualifier::HiBlast);
    }

    #[test]
    fn qualifier_hi_complexity_outranks_gotcha() {
        // cognitive_score > 0.7 AND gotcha_count > 0 → HiComplexity outranks GotchaActive
        let q = ContextQualifier::from_enrichment_with_cognitive(
            None,
            Some(0.8),
            3, // gotcha hits — outranked by HiComplexity
            None,
            None,
        );
        assert_eq!(q, ContextQualifier::HiComplexity);
    }

    #[test]
    fn to_key_hi_complexity() {
        // End-to-end: key must embed "hi-complexity"
        let sig = ActionSignature::from_pre_tool_with_cognitive(
            "Edit",
            &json!({"file_path": "src/complex.rs"}),
            None,       // no blast
            Some(0.9),  // hi-complexity fires
            0,          // no gotchas
            Some(true), // indexed
            None,
        );
        let key = sig.to_key();
        assert!(
            key.contains("hi-complexity"),
            "expected 'hi-complexity' in key, got: {key}"
        );
        assert_eq!(sig.context_qualifier, ContextQualifier::HiComplexity);
    }

    #[test]
    fn from_pre_tool_with_cognitive_fires_hi_complexity() {
        // Verify from_pre_tool_with_cognitive constructor end-to-end
        let sig = ActionSignature::from_pre_tool_with_cognitive(
            "Bash",
            &json!({"command": "cargo check"}),
            None,
            Some(0.75),
            0,
            None,
            None,
        );
        assert_eq!(sig.context_qualifier, ContextQualifier::HiComplexity);
        assert!(sig.to_key().starts_with("outcome:bash:cargo:hi-complexity"));
    }

    #[test]
    fn hi_complexity_as_str() {
        assert_eq!(ContextQualifier::HiComplexity.as_str(), "hi-complexity");
    }
}
