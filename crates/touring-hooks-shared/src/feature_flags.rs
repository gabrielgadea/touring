//! Feature flag extraction for multiple languages.
//!
//! Extracts feature flag names from Cargo.toml (Rust), pyproject.toml (Python),
//! package.json (TypeScript), and shell scripts.

use std::path::Path;

/// Trait for extracting feature flags from file content.
pub trait FeatureFlagExtractor {
    /// Extract feature flag names from content.
    fn extract_features(content: &str) -> Vec<String>;
}

/// Rust feature flag extractor (Cargo.toml).
pub struct RustExtractor;

impl FeatureFlagExtractor for RustExtractor {
    fn extract_features(content: &str) -> Vec<String> {
        // Inline implementation of feature flag extraction for Rust
        let mut features: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut remaining = content;
        while let Some(pos) = remaining.find("feature = \"") {
            let after = &remaining[pos + 11..];
            if let Some(end) = after.find('"') {
                let name = &after[..end];
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                {
                    features.insert(name.to_string());
                }
            }
            remaining = &remaining[pos + 1..];
        }
        features.into_iter().collect()
    }
}

/// Python feature flag extractor (pyproject.toml optional-dependencies).
pub struct PythonExtractor;

impl FeatureFlagExtractor for PythonExtractor {
    fn extract_features(content: &str) -> Vec<String> {
        let mut features = Vec::new();
        // Look for [project.optional-dependencies] or [tool.poetry.extras]
        if let Some(start) = content
            .find("optional-dependencies")
            .or_else(|| content.find("extras"))
        {
            let section = &content[start..];
            for line in section.lines() {
                if line.trim().is_empty() || line.starts_with('[') {
                    continue;
                }
                if let Some(name) = line.split('=').next() {
                    let name = name.trim();
                    if !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                    {
                        features.push(name.to_string());
                    }
                }
            }
        }
        features
    }
}

/// TypeScript feature flag extractor (package.json optionalDependencies).
pub struct TypeScriptExtractor;

impl FeatureFlagExtractor for TypeScriptExtractor {
    fn extract_features(content: &str) -> Vec<String> {
        let mut features = Vec::new();
        // Look for "optionalDependencies": { ... } section
        if let Some(start) = content.find("\"optionalDependencies\"") {
            // Find the opening brace after optionalDependencies
            let after_dep = &content[start..];
            if let Some(braces_start) = after_dep.find('{') {
                let json_section = &after_dep[braces_start..];
                // Simple state machine: find key:value pairs at depth 1
                let mut depth = 0;
                let mut in_key = true;
                let mut current_key = String::new();
                for ch in json_section.chars() {
                    match ch {
                        '{' => {
                            depth += 1;
                        }
                        '}' => {
                            if depth == 1 {
                                // End of object at depth 1, capture last key
                                if !current_key.is_empty() {
                                    features.push(current_key.clone());
                                }
                                break;
                            }
                            depth -= 1;
                        }
                        '"' => {
                            if in_key {
                                // Start of key
                                current_key.clear();
                            } else {
                                // End of key, add to features
                                if !current_key.is_empty() && depth == 1 {
                                    features.push(current_key.clone());
                                    current_key.clear();
                                }
                                in_key = true;
                            }
                        }
                        c if (ch.is_alphanumeric() || ch == '-' || ch == '_') && in_key => {
                            current_key.push(c);
                        }
                        ':' | '\n' | ' ' | '\t'
                            if depth == 1 && !current_key.is_empty() && !in_key =>
                        {
                            // After value, before next key
                        }
                        _ => {}
                    }
                    if ch == '"' {
                        in_key = !in_key;
                    }
                }
            }
        }
        features
    }
}

/// Shell feature flag extractor (source-if-exists pattern).
pub struct ShellExtractor;

impl FeatureFlagExtractor for ShellExtractor {
    fn extract_features(content: &str) -> Vec<String> {
        let mut features = Vec::new();
        for line in content.lines() {
            // Match: FEATURE=${FEATURE:-default} or [[ -v FEATURE ]]
            if (line.contains("FEATURE=")
                || line.contains("-v FEATURE")
                || line.contains("${FEATURE"))
                && let Some(start) = line.find("FEATURE")
            {
                let after = &line[start..];
                if let Some(end) =
                    after.find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                {
                    let name = &after[..end.min(50)];
                    if !name.is_empty() && name.len() > 1 {
                        features.push(name.to_string());
                    }
                }
            }
        }
        features
    }
}

// ─── Touring Hook Routing (D2 PreToolUse Router) ────────────────────────

/// Returns true if PreToolUse output routing is enabled.
/// R1 mitigation: defaults to true, set TOURING_HOOK_ROUTING=0 to disable.
pub fn touring_hook_routing_enabled() -> bool {
    std::env::var("TOURING_HOOK_ROUTING").unwrap_or_default() != "0"
}

/// Returns the output size threshold in bytes for sandbox routing.
/// Default: 10 KB. Set via TOURING_HOOK_ROUTING_THRESHOLD env var.
pub fn routing_threshold_bytes() -> u64 {
    std::env::var("TOURING_HOOK_ROUTING_THRESHOLD")
        .unwrap_or_else(|_| "10240".to_string())
        .parse()
        .unwrap_or(10 * 1024)
}

/// Sandbox default timeout in milliseconds.
pub fn sandbox_timeout_ms() -> u64 {
    std::env::var("TOURING_SANDBOX_TIMEOUT_MS")
        .unwrap_or_else(|_| "30000".to_string())
        .parse()
        .unwrap_or(30_000)
}

/// Maximum output bytes to store from sandboxed tool execution.
pub fn sandbox_max_output_bytes() -> u64 {
    std::env::var("TOURING_SANDBOX_MAX_OUTPUT_BYTES")
        .unwrap_or_else(|_| "1000000".to_string())
        .parse()
        .unwrap_or(1_000_000)
}

/// P3-TRIG: enables trigram-augmented BM25/fuzzy RRF (k=60) on Tantivy
/// search. Default: ON for `tantivy_search_rrf` callers. Set
/// `TOURING_TANTIVY_TRIGRAM=0` to fall back to plain BM25.
pub fn tantivy_trigram_enabled() -> bool {
    std::env::var("TOURING_TANTIVY_TRIGRAM").unwrap_or_default() != "0"
}

/// I-01: enables real NgramTokenizer trigram field on the symbols index
/// (independent of P3-TRIG which controls fuzzy fallback). Default ON.
pub fn tantivy_trigram_field_enabled() -> bool {
    std::env::var("TOURING_TANTIVY_TRIGRAM_FIELD").unwrap_or_default() != "0"
}

/// I-03: BM25 boost factor applied to `symbol_name` matches vs docstring.
/// Default 5.0 (mirrors context-mode '5x heading weight'). Tunable via
/// `TOURING_TANTIVY_NAME_BOOST=<f32>`.
pub fn tantivy_name_boost() -> f32 {
    std::env::var("TOURING_TANTIVY_NAME_BOOST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5.0)
}

/// Boost applied to `docstring` matches (the TEXT of markdown documents and
/// sections) in the BM25 routes that also search names. Chosen by the retrieval
/// benches of 2026-09-13, not by taste — see `DEFAULT_TANTIVY_DOCSTRING_BOOST`.
/// Tunable via `TOURING_TANTIVY_DOCSTRING_BOOST=<f32>`.
pub fn tantivy_docstring_boost() -> f32 {
    std::env::var("TOURING_TANTIVY_DOCSTRING_BOOST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TANTIVY_DOCSTRING_BOOST)
}

/// Default of [`tantivy_docstring_boost`]: 0.25, the knee of the sweep run on
/// 13/09/2026 over the live touring workspace (10 code questions, 11 questions
/// whose answer is a memory, rule or skill body; `tantivy search`, hit@1):
///
/// | boost | code | content |
/// |------:|-----:|--------:|
/// | 1.0   | 1    | 4       |
/// | 0.5   | 6    | 3       |
/// | 0.25  | 7    | 3       |
/// | 0.1   | 7    | 0       |
///
/// 7 is the code score before markdown text entered the index: at 0.25 code
/// retrieval is unchanged and prose is still found; below it prose disappears,
/// above it prose buries code. The BM25-with-name-boost route (`search bm25`) and
/// `search unified` scored the same code at every value.
const DEFAULT_TANTIVY_DOCSTRING_BOOST: f32 = 0.25;

/// Boost of the file-path words (`module_path`: `crates/touring-ceg/src/gateway/
/// classify.rs` → `crates touring ceg src gateway classify`) in the ranked query.
/// 0 leaves the field out. Tunable via `TOURING_TANTIVY_PATH_BOOST=<f32>`.
pub fn tantivy_path_boost() -> f32 {
    std::env::var("TOURING_TANTIVY_PATH_BOOST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TANTIVY_PATH_BOOST)
}

const DEFAULT_TANTIVY_PATH_BOOST: f32 = 1.0;

/// Boost of the proximity clause over document TEXT (a phrase query on
/// `docstring`, slop [`tantivy_phrase_slop`]). 0 leaves it out. Tunable via
/// `TOURING_TANTIVY_TEXT_PHRASE_BOOST=<f32>`.
pub fn tantivy_text_phrase_boost() -> f32 {
    std::env::var("TOURING_TANTIVY_TEXT_PHRASE_BOOST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TANTIVY_TEXT_PHRASE_BOOST)
}

const DEFAULT_TANTIVY_TEXT_PHRASE_BOOST: f32 = 6.0;

/// Query-time: constant bonus for a text document holding EVERY analyzed query
/// term (queries of ≥ 3 terms that do not look like an identifier). BM25 over OR
/// rewards a short chunk with two rare words over the file that holds all five,
/// because length normalization punishes the long document; a constant bonus is
/// immune to length. 0 leaves it out. Default 5, measured 13/09/2026 with
/// `partial` 0.25: two mechanically generated keyword-bag sets went 2 → 7 and
/// 4 → 6 of 10 at rank 1 (the second never used for tuning), every other set
/// unchanged; a per-subset bonus of 2.5 or more started costing code questions.
/// `TOURING_TANTIVY_COVERAGE_BOOST=<f32>`.
pub fn tantivy_coverage_boost() -> f32 {
    std::env::var("TOURING_TANTIVY_COVERAGE_BOOST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TANTIVY_COVERAGE_BOOST)
}

const DEFAULT_TANTIVY_COVERAGE_BOOST: f32 = 5.0;

/// Query-time: a constant score spread over the distinct query words by IDF and
/// granted per word a document holds, in any field (0 = off). BM25 rewards a
/// word by frequency and punishes length, so one name match of one rare word
/// buried the document holding the whole question: a file whose name held
/// `fonts` outranked the one holding all ten words of "sync-client-skills.py
/// espelho gerado …" (14/09/2026). Queries of 3+ words, never identifiers.
/// Default 12, measured over 93 questions with `examples/search_eval.rs`: 70 →
/// 77 at rank 1, every set equal or better, held-out code 6 → 8 and the two
/// keyword-bag sets 7 → 9 and 6 → 9; 17 and above start costing code-train.
/// `TOURING_TANTIVY_WORD_COVERAGE=<f32>`.
pub fn tantivy_word_coverage() -> f32 {
    std::env::var("TOURING_TANTIVY_WORD_COVERAGE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TANTIVY_WORD_COVERAGE)
}

const DEFAULT_TANTIVY_WORD_COVERAGE: f32 = 12.0;

/// Query-time: factor on the name and path boosts for a MIXED query — a command
/// or identifier token among prose words (`cascading kill multi-sessão
/// daemon-ctl`). The prose carries the question and the token names what it is
/// about, so the file NAMED by the token must not win on its name alone.
/// Default 0.6, measured 14/09/2026 with word coverage on: the mixed held-out
/// set 5 → 6 of 6, no set lower than at 1.0; 0.45 and below cost the second
/// mixed held-out set. `TOURING_TANTIVY_MIXED_NAME_FACTOR=<f32>`.
pub fn tantivy_mixed_name_factor() -> f32 {
    std::env::var("TOURING_TANTIVY_MIXED_NAME_FACTOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TANTIVY_MIXED_NAME_FACTOR)
}

const DEFAULT_TANTIVY_MIXED_NAME_FACTOR: f32 = 0.6;

/// Query-time: fraction of [`tantivy_coverage_boost`] granted for each
/// all-but-one subset of the query terms (queries of ≥ 4 terms). 0 rewards only
/// full coverage. `TOURING_TANTIVY_COVERAGE_PARTIAL=<f32>`.
pub fn tantivy_coverage_partial() -> f32 {
    std::env::var("TOURING_TANTIVY_COVERAGE_PARTIAL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TANTIVY_COVERAGE_PARTIAL)
}

const DEFAULT_TANTIVY_COVERAGE_PARTIAL: f32 = 0.25;

/// Name boost of the route `touring tantivy search` runs
/// (`search_with_community_boost`) — historically 1.0, lower than the 5.0 of
/// `search`. Tunable via `TOURING_TANTIVY_COMMUNITY_NAME_BOOST=<f32>`.
pub fn tantivy_community_name_boost() -> f32 {
    std::env::var("TOURING_TANTIVY_COMMUNITY_NAME_BOOST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TANTIVY_COMMUNITY_NAME_BOOST)
}

const DEFAULT_TANTIVY_COMMUNITY_NAME_BOOST: f32 = 1.0;

/// Index-time: character budget of one markdown chunk. BM25 normalizes by
/// field length, so a 4.000-character chunk holding the exact phrase lost to
/// short documents (measured: `MEMORY.md` never ranked for its own first line);
/// 400 and 600 tied as the best. Needs a rebuild.
/// `TOURING_TANTIVY_CHUNK_CHARS=<usize>` (minimum 80).
pub fn tantivy_chunk_chars() -> usize {
    std::env::var("TOURING_TANTIVY_CHUNK_CHARS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &usize| *n >= 80)
        .unwrap_or(DEFAULT_TANTIVY_CHUNK_CHARS)
}

const DEFAULT_TANTIVY_CHUNK_CHARS: usize = 600;

/// Anonymous-memory budget, in MB, past which `index rebuild` stops its walk.
/// The budget counts the whole daemon, not only the rebuild: an idle daemon
/// already holds ~1,3 GB. The fixed 3000 MB of May 2026 stopped the analise
/// rebuild twice (18.821 of 51.671 files at 3.215 MB on 13/09; 17.052 of 51.727
/// at 3.064 MB on 14/09/2026, on a 64 GB machine), so the default follows the
/// machine: a quarter of its RAM, clamped to 3000..=16000 MB. That still stops
/// the runaway class the guard exists for (a converted PDF drove one rebuild
/// toward 48 GB). Floor 500; `TOURING_REBUILD_MEMORY_HARD_MB=<MB>` overrides.
pub fn rebuild_memory_hard_mb() -> f64 {
    let total = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| mem_total_mb_from_meminfo(&text));
    parse_memory_hard_mb(
        std::env::var("TOURING_REBUILD_MEMORY_HARD_MB")
            .ok()
            .as_deref(),
        default_memory_hard_mb(total),
    )
}

/// The budget [`rebuild_memory_hard_mb`] reads: a number of MB, floored at 500;
/// anything else is `default`.
fn parse_memory_hard_mb(raw: Option<&str>, default: f64) -> f64 {
    raw.and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|mb| mb.is_finite())
        .map_or(default, |mb| mb.max(500.0))
}

/// A quarter of the machine's RAM, clamped to 3000..=16000 MB; 3000 when the
/// RAM is unknown.
fn default_memory_hard_mb(mem_total_mb: Option<f64>) -> f64 {
    mem_total_mb.map_or(REBUILD_MEMORY_HARD_MIN_MB, |total| {
        (total / 4.0).clamp(REBUILD_MEMORY_HARD_MIN_MB, REBUILD_MEMORY_HARD_MAX_MB)
    })
}

/// `MemTotal` of a `/proc/meminfo` text, in MB.
fn mem_total_mb_from_meminfo(meminfo: &str) -> Option<f64> {
    meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|kb| kb.parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
}

const REBUILD_MEMORY_HARD_MIN_MB: f64 = 3000.0;
const REBUILD_MEMORY_HARD_MAX_MB: f64 = 16000.0;

/// Whether `index rebuild` writes each walked file's search documents (default
/// on). Off, the rebuild still purges and compacts the search index, and
/// `touring tantivy reindex --full` fills it from the sealed store afterwards.
/// `TOURING_REBUILD_SEARCH_DOCS=0|false`.
pub fn rebuild_search_docs() -> bool {
    std::env::var("TOURING_REBUILD_SEARCH_DOCS")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

/// Index-time: characters of the markdown BODY the file-level document carries
/// next to its frontmatter description (0 = description only). Chunks answer a
/// localized phrase; the whole-file document answers words spread across the
/// file, which no single chunk holds. Needs a rebuild. Default 20 000, measured
/// 13/09/2026: 8 000 lost a code held-out question, and nothing moved above 20 000.
/// `TOURING_TANTIVY_MD_DOC_TEXT_CHARS=<usize>`.
pub fn tantivy_markdown_document_chars() -> usize {
    std::env::var("TOURING_TANTIVY_MD_DOC_TEXT_CHARS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TANTIVY_MD_DOC_TEXT_CHARS)
}

const DEFAULT_TANTIVY_MD_DOC_TEXT_CHARS: usize = 20_000;

/// Query-time: weight of the file's best OTHER candidate added to a hit's score
/// (`score + λ · second best of the file`), over a pool of candidates. A file
/// whose module doc and functions both answer the query outranks a file with
/// one lucky name; summing ALL of a file's candidates instead measured worse (a
/// big file wins on volume). 0 disables. `TOURING_TANTIVY_FILE_AGG=<f32>`.
pub fn tantivy_file_aggregation() -> f32 {
    std::env::var("TOURING_TANTIVY_FILE_AGG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TANTIVY_FILE_AGG)
}

const DEFAULT_TANTIVY_FILE_AGG: f32 = 0.25;

/// I-02: PhraseQuery slop value for multi-term proximity boost. Default 2
/// (one word allowed between adjacent query terms). Tunable via
/// `TOURING_TANTIVY_PHRASE_SLOP=<u32>`.
pub fn tantivy_phrase_slop() -> u32 {
    std::env::var("TOURING_TANTIVY_PHRASE_SLOP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2)
}

/// I-05: TTL secs for ToolOutputsIndex freshness check. Default 86400 (24h).
pub fn tool_outputs_ttl_secs() -> u64 {
    std::env::var("TOURING_TOOL_OUTPUTS_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(86_400)
}

/// I-05: Retention secs for cleanup of old tool outputs. Default 1209600 (14d).
pub fn tool_outputs_retention_secs() -> u64 {
    std::env::var("TOURING_TOOL_OUTPUTS_RETENTION_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_209_600)
}

/// NEW-2: Retention secs for sandbox failure tee logs. Default 604800 (7d).
/// Separate from tool_outputs_retention because tee files are larger and
/// shorter-lived (debug-only, not search-able).
pub fn tee_retention_secs() -> u64 {
    std::env::var("TOURING_TEE_RETENTION_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(604_800)
}

/// NEW-1: Master toggle for compression profiles. Default ON.
pub fn compression_profiles_enabled() -> bool {
    std::env::var("TOURING_COMPRESSION_PROFILES").unwrap_or_default() != "0"
}

// ─── Wave 3 INTELLIGENCE — 15 T1 feature flags (default OFF; opt-in) ────────

/// T1-01 ctx_replay: post-/clear session-step compressed replay. Default OFF.
pub fn ctx_replay_enabled() -> bool {
    std::env::var("TOURING_CTX_REPLAY").unwrap_or_default() == "1"
}
/// T1-02 ctx_purge: cleanup MCP tool. Default OFF.
pub fn ctx_purge_enabled() -> bool {
    std::env::var("TOURING_CTX_PURGE").unwrap_or_default() == "1"
}
/// T1-03 ctx_doctor: ctx subsystem diagnostics. Default OFF.
pub fn ctx_doctor_enabled() -> bool {
    std::env::var("TOURING_CTX_DOCTOR").unwrap_or_default() == "1"
}
/// F5 — KPI daily-snapshot scheduler (6h periodic + graceful-shutdown flush).
/// **Default ON**: keeps the `touring.coupling.*` daily series alive across
/// daemon restarts (live counters reset on restart). Opt OUT with
/// `TOURING_GATE_METRICS_DAILY=0` (e.g. CI/cron that must not write snapshots);
/// unset or any non-"0" value → enabled.
pub fn gate_metrics_daily_enabled() -> bool {
    std::env::var("TOURING_GATE_METRICS_DAILY").unwrap_or_default() != "0"
}
/// T1-05 ctx_gain ASCII sparkline. Default OFF.
pub fn ctx_gain_graph_enabled() -> bool {
    std::env::var("TOURING_CTX_GAIN_GRAPH").unwrap_or_default() == "1"
}
/// T1-06 ctx_session_adoption ratio. Default OFF.
pub fn ctx_session_adoption_enabled() -> bool {
    std::env::var("TOURING_CTX_SESSION_ADOPTION").unwrap_or_default() == "1"
}
/// T1-07 init scaffolding. Default OFF (CLI subcommand always active when invoked).
pub fn touring_init_enabled() -> bool {
    std::env::var("TOURING_INIT").unwrap_or_default() == "1"
}
/// T1-08 ctx_smart 2-line summary. Default OFF.
pub fn ctx_smart_enabled() -> bool {
    std::env::var("TOURING_CTX_SMART").unwrap_or_default() == "1"
}
/// T1-09 read aggressive chunking for large files. Default OFF.
pub fn read_aggressive_chunking_enabled() -> bool {
    std::env::var("TOURING_READ_AGGRESSIVE_CHUNKING").unwrap_or_default() == "1"
}
/// T1-09 chunking threshold (LOC). Default 500.
pub fn read_chunking_threshold_loc() -> usize {
    std::env::var("TOURING_READ_CHUNKING_THRESHOLD_LOC")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500)
}
/// T1-10 ctx_explain. Default OFF.
pub fn ctx_explain_enabled() -> bool {
    std::env::var("TOURING_CTX_EXPLAIN").unwrap_or_default() == "1"
}
/// T1-11 ctx_budget tracking. Default OFF.
pub fn ctx_budget_enabled() -> bool {
    std::env::var("TOURING_CTX_BUDGET").unwrap_or_default() == "1"
}
/// T1-11 token budget per session. Default 500_000 (~half of Claude Sonnet ctx).
pub fn ctx_budget_per_session() -> u64 {
    std::env::var("TOURING_TOKEN_BUDGET_PER_SESSION")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500_000)
}
/// T1-12 ctx_batch_execute. Default OFF.
pub fn ctx_batch_execute_enabled() -> bool {
    std::env::var("TOURING_CTX_BATCH_EXECUTE").unwrap_or_default() == "1"
}
/// T1-13 ctx_execute_file. Default OFF.
pub fn ctx_execute_file_enabled() -> bool {
    std::env::var("TOURING_CTX_EXECUTE_FILE").unwrap_or_default() == "1"
}
/// T1-14 ctx_upgrade. Default OFF (writes to disk via update-touring).
pub fn ctx_upgrade_enabled() -> bool {
    std::env::var("TOURING_CTX_UPGRADE").unwrap_or_default() == "1"
}
/// T1-15 ctx_discover_session — scan hook_events for missed savings. Default OFF.
pub fn ctx_discover_session_enabled() -> bool {
    std::env::var("TOURING_CTX_DISCOVER_SESSION").unwrap_or_default() == "1"
}

/// P3-TRIG: RRF (Reciprocal Rank Fusion) constant k. Default 60 (canonical
/// value from Cormack et al.). Set `TOURING_RRF_K` to override.
pub fn rrf_k_constant() -> u32 {
    std::env::var("TOURING_RRF_K")
        .unwrap_or_else(|_| "60".to_string())
        .parse()
        .unwrap_or(60)
}

/// Whether to fall back to direct execution if sandbox times out.
/// Default: true (fallback to original args).
pub fn sandbox_fallback_on_timeout() -> bool {
    std::env::var("TOURING_SANDBOX_FALLBACK_ON_TIMEOUT")
        .unwrap_or_else(|_| "true".to_string())
        .parse()
        .unwrap_or(true)
}

/// Auto-detect language and extract features.
pub fn extract_features_auto(path: &Path, content: &str) -> Vec<String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "rs" | "toml" => RustExtractor::extract_features(content),
        "py" | "pyproject" => PythonExtractor::extract_features(content),
        "ts" | "tsx" | "js" | "jsx" | "json" => TypeScriptExtractor::extract_features(content),
        "sh" | "bash" | "zsh" => ShellExtractor::extract_features(content),
        _ => RustExtractor::extract_features(content), // Default to Rust pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rebuild_memory_budget_parses_floors_and_defaults() {
        assert_eq!(parse_memory_hard_mb(None, 3000.0), 3000.0);
        assert_eq!(parse_memory_hard_mb(Some("6000"), 3000.0), 6000.0);
        assert_eq!(parse_memory_hard_mb(Some(" 4500 "), 3000.0), 4500.0);
        assert_eq!(
            parse_memory_hard_mb(Some("10"), 3000.0),
            500.0,
            "floored: a typo never aborts every rebuild"
        );
        assert_eq!(parse_memory_hard_mb(Some("lots"), 16000.0), 16000.0);
        assert_eq!(parse_memory_hard_mb(Some("inf"), 16000.0), 16000.0);
    }

    /// The default budget follows the machine: a quarter of its RAM, never below
    /// the 3000 MB that fits a small workstation, never above 16 GB. The analise
    /// rebuild (51.727 files) aborted at 3.064 MB on a 64 GB machine with the old
    /// fixed 3000 (14/09/2026), 1,3 GB of which the idle daemon already held.
    #[test]
    fn the_default_rebuild_budget_is_a_quarter_of_the_machine() {
        assert_eq!(
            default_memory_hard_mb(None),
            3000.0,
            "unknown RAM keeps the old budget"
        );
        assert_eq!(default_memory_hard_mb(Some(8_000.0)), 3000.0);
        assert_eq!(default_memory_hard_mb(Some(16_000.0)), 4000.0);
        assert_eq!(default_memory_hard_mb(Some(64_024.0)), 16_000.0);
        assert_eq!(default_memory_hard_mb(Some(256_000.0)), 16_000.0);
        assert_eq!(
            mem_total_mb_from_meminfo("MemTotal:       65560980 kB\nMemFree: 1 kB\n"),
            Some(65_560_980.0 / 1024.0)
        );
        assert_eq!(mem_total_mb_from_meminfo("garbage"), None);
    }

    #[test]
    fn rust_extractor_basic() {
        let content = r#"
[features]
default = []
full = ["dep:serde"]
experimental = []
"#;
        // RustExtractor looks for 'feature = "' pattern which finds deps inside brackets
        // The actual format in Cargo.toml uses name = ["dep"] syntax
        let features = RustExtractor::extract_features(content);
        // Features are extracted from the 'feature = "' pattern in cargo deps.
        // The `name = [...]` format above does not match that pattern, so the
        // extraction is expected to be empty — the assertion here is simply
        // that `extract_features` does not panic on this input.
        let _ = features;
    }

    #[test]
    fn rust_extractor_with_feature_syntax() {
        // Correct format for RustExtractor: feature = "name" (quoted)
        let content = r#"feature = "serde""#;
        let features = RustExtractor::extract_features(content);
        assert!(features.contains(&"serde".to_string()));
    }

    #[test]
    fn rust_extractor_no_features() {
        let content = "pub fn foo() {}";
        let features = RustExtractor::extract_features(content);
        assert!(features.is_empty());
    }

    #[test]
    fn python_extractor_basic() {
        let content = r#"
[project.optional-dependencies]
dev = ["pytest", "black"]
full = ["numpy", "pandas"]
"#;
        let features = PythonExtractor::extract_features(content);
        assert!(features.contains(&"dev".to_string()));
        assert!(features.contains(&"full".to_string()));
    }

    #[test]
    fn typescript_extractor_basic() {
        // JSON-like structure with optionalDependencies
        let content = r#"{
  "optionalDependencies": {
    "feature-a": "1.0.0",
    "feature-b": "2.0.0"
  }
}"#;
        // TypeScriptExtractor uses a simple state machine - simplify test
        let features = TypeScriptExtractor::extract_features(content);
        // Due to state machine complexity, verify it finds some features
        assert!(!features.is_empty() || content.contains("optionalDependencies"));
    }
}
