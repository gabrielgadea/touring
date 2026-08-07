//! Shared helpers for the `cli-*` handlers (Master Plan A.W2.P5 extraction).
//!
//! Mechanical extraction from `cli_handlers.rs` of the cross-module helper
//! functions: memory-recall SQL/FTS5 path, semantic-embedding bridge,
//! federated DB discovery, the AhoCorasick skill-matcher, and the
//! decompose-table bootstrap. Every symbol keeps its original visibility;
//! `cli_handlers.rs` re-exports them so existing `crate::cli_handlers::<helper>`
//! call sites across the 37 grouped modules resolve unchanged.

use crate::runtime::HookRuntime;
use rusqlite::params;

/// Reward for a retrieval that returned `found` of an asked-for `requested`.
///
/// # Why this exists
///
/// Five CLI sites used to inject a literal `1.0` on any non-error result
/// (`cli-tantivy-search`, `-fuzzy`, `-suggest`, `cli-ast-semantic`,
/// `cli-ast-quality`). A constant reward has **zero variance**: the LinUCB
/// regression then fits `x·θ = 1` for every context and can discriminate
/// nothing, and once the EMA converges onto that constant the
/// `min_reward_delta` filter drops the updates entirely. A signal that is
/// always the same teaches nothing (04/08/2026).
///
/// Coverage is the honest scalar available at those call sites: a query that
/// found nothing did *not* succeed just because it did not error.
///
/// Returns 0.0 for an empty result and saturates at 1.0 once the caller got
/// everything it asked for. A `requested` of 0 is meaningless as a denominator,
/// so it degrades to "found anything at all".
pub(crate) fn retrieval_coverage_reward(found: usize, requested: usize) -> f64 {
    if requested == 0 {
        return if found > 0 { 1.0 } else { 0.0 };
    }
    (found as f64 / requested as f64).clamp(0.0, 1.0)
}

/// Synthetic RL warm-up: seed the online bandit with a small set of
/// canonical tool rewards so the first real edits aren't cold-started.
pub(crate) fn inject_synthetic_tool_rewards(rt: &mut HookRuntime) {
    let Some(ref mut engine) = rt.learning.online_rl else {
        return;
    };
    let synthetic_rewards = [
        ("Read", 0.8, 0),
        ("Edit", 0.75, 1),
        ("Write", 0.7, 1),
        ("Bash", 0.6, 3),
    ];
    for (tool_name, quality, file_type) in synthetic_rewards {
        let reward = touring_intelligence::rl::ImmediateReward {
            tool_name: tool_name.to_string(),
            accepted: quality > 0.0,
            latency_ms: 0,
            error_count: 0,
            cila_level: 2,
            file_type,
            quality_score: Some(quality),
        };
        if rt.learning.qtable_cache.is_none() {
            rt.learning.qtable_cache = Some(touring_intelligence::rl::QTable::new());
        }
        if let Some(mut qtable) = rt.learning.qtable_cache.take() {
            if rt.learning.linucb.is_none() {
                rt.learning.linucb = Some(touring_intelligence::rl::LinUCBBandit::new());
            }
            if let Some(ref mut linucb) = rt.learning.linucb {
                engine.process_reward(&reward, &mut qtable, linucb);
            }
            rt.learning.qtable_cache = Some(qtable);
        }
        tracing::debug!(
            tool = tool_name,
            quality = quality,
            "S-9: synthetic reward injected"
        );
    }
}

/// Tokenizes a free-text recall query into bare, lower-cased terms.
///
/// Splits on every non-alphanumeric char, so a key fragment like
/// `transcript-ab12` yields `transcript` + `ab12`. Empty tokens are dropped
/// and at most 12 are kept — a pathological query cannot build an unbounded
/// MATCH expression.
pub(crate) fn memory_recall_terms(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .take(12)
        .map(str::to_lowercase)
        .collect()
}

/// Builds a safe FTS5 `MATCH` expression from a free-text query.
///
/// Each term is emitted as a double-quoted phrase literal. This is essential:
/// a raw `MATCH 'outcome:bash:transcript-ab12:failure'` fails with
/// `no such column: outcome` because FTS5 reads `:` as the column-filter
/// operator (bare `AND`/`OR`/`NOT`/`NEAR` are operators too). Quoting
/// neutralises all of it. Space-separated phrases are implicitly AND-ed, so
/// `outcome transcript failure` matches an entry keyed
/// `outcome:bash:transcript-ab12:failure`. Returns an empty string when the
/// query has no usable term — the caller then skips the FTS path.
pub(crate) fn memory_recall_fts5_expr(query: &str) -> String {
    memory_recall_terms(query)
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(" ")
}

/// SQL expression yielding `outcome_reward`, degrading to `NULL` when the
/// column is absent.
///
/// Recall is **federated**: it reaches other projects' `memory.db` files, which
/// may predate the outcome columns. Probing costs one PRAGMA and is far better
/// than letting a `no such column` error turn that project's recall into an
/// empty result — which is precisely how `memory_recall_fts5` fails, silently.
pub(crate) fn outcome_reward_select(conn: &rusqlite::Connection, alias: &str) -> String {
    let present = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('memory_entries') WHERE name = 'outcome_reward'",
            [],
            |r| r.get::<_, i32>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);
    if present {
        format!("{alias}outcome_reward")
    } else {
        "NULL".to_string()
    }
}

/// Maps a recall result row (`key, value, tier, entry_type, outcome_reward`) to JSON.
///
/// `outcome_reward` is the `r` of a case `(s, a, r)` (Memento Eq. 12) and is
/// emitted ONLY when non-NULL: an unobserved case must stay distinguishable
/// from one measured at zero, or a value-ranked recall would bury every curated
/// lesson for the crime of never having been scored.
pub(crate) fn memory_recall_row_to_json(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<serde_json::Value> {
    let mut out = serde_json::json!({
        "key": row.get::<_, String>(0)?,
        "value": row.get::<_, String>(1)?,
        "tier": row.get::<_, String>(2)?,
        "type": row.get::<_, String>(3)?,
    });
    let reward = row.get::<_, Option<f64>>(4).ok().flatten();
    if let (Some(r), Some(obj)) = (reward, out.as_object_mut()) {
        obj.insert("outcome_reward".into(), serde_json::json!(r));
    }
    Ok(out)
}

/// FTS5 primary recall path — tokenized `MATCH` over `memories_fts`,
/// bm25-ranked, joined back to `memory_entries` for the `tier` column.
///
/// Returns an empty vec when `memories_fts` is absent (older DB schema) or the
/// query matched nothing; the caller then tries [`memory_recall_like`].
pub(crate) fn memory_recall_fts5(
    conn: &rusqlite::Connection,
    fts_expr: &str,
) -> Vec<serde_json::Value> {
    let reward_col = outcome_reward_select(conn, "e.");
    let sql = format!(
        "SELECT e.key, e.value, e.tier, e.entry_type, {reward_col} \
         FROM memories_fts \
         JOIN memory_entries e ON e.rowid = memories_fts.rowid \
         WHERE memories_fts MATCH ?1 \
         ORDER BY bm25(memories_fts) LIMIT 20"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return vec![];
    };
    stmt.query_map(params![fts_expr], memory_recall_row_to_json)
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

/// Fallback recall path — per-term `LIKE` across `key` + `value`, AND-joined,
/// used when FTS5 is unavailable or returned nothing (e.g. a mid-token
/// fragment FTS5 cannot tokenize to). A query with no alphanumeric term
/// degrades to a single raw-substring `LIKE`. Empty vec on prepare failure.
pub(crate) fn memory_recall_like(
    conn: &rusqlite::Connection,
    query: &str,
) -> Vec<serde_json::Value> {
    let terms = memory_recall_terms(query);
    let (where_clause, binds): (String, Vec<String>) = if terms.is_empty() {
        (
            "key LIKE ?1 OR value LIKE ?1".to_string(),
            vec![format!("%{query}%")],
        )
    } else {
        let clause = (1..=terms.len())
            .map(|n| format!("(key LIKE ?{n} OR value LIKE ?{n})"))
            .collect::<Vec<_>>()
            .join(" AND ");
        (clause, terms.iter().map(|t| format!("%{t}%")).collect())
    };
    let reward_col = outcome_reward_select(conn, "");
    let sql = format!(
        "SELECT key, value, tier, entry_type, {reward_col} FROM memory_entries \
         WHERE {where_clause} LIMIT 20"
    );
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("memory recall prepare failed: {}", e);
            return vec![];
        }
    };
    let bind_refs: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    stmt.query_map(bind_refs.as_slice(), memory_recall_row_to_json)
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

/// Searches `memory.db` for entries matching `query`; up to 20 rows.
///
/// **Primary path — FTS5** ([`memory_recall_fts5`]). `memories_fts` is a
/// trigger-maintained full-text index over `key` + `value` + `entry_type`. The
/// query is tokenized (see [`memory_recall_fts5_expr`]) so a multi-word recall
/// such as `outcome transcript failure` matches an entry whose key is
/// `outcome:bash:transcript-ab12:failure`, ranked by bm25. The previous
/// implementation queried `LIKE '%<whole query>%'`, which matched the query
/// only as ONE contiguous substring — any multi-word recall silently returned
/// nothing even though the index already held the answer.
///
/// **Fallback path** — per-term `LIKE` ([`memory_recall_like`]) when
/// `memories_fts` is absent or the FTS query found nothing. An empty vec is
/// returned on connection failure (DEBUG-logged) — recall is best-effort.
pub(crate) fn memory_recall_sql(
    memory_db_path: &std::path::Path,
    query: &str,
) -> Vec<serde_json::Value> {
    let conn = match rusqlite::Connection::open(memory_db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("memory recall connection failed: {}", e);
            return vec![];
        }
    };
    let fts_expr = memory_recall_fts5_expr(query);
    if !fts_expr.is_empty() {
        let hits = memory_recall_fts5(&conn, &fts_expr);
        if !hits.is_empty() {
            return hits;
        }
    }
    memory_recall_like(&conn, query)
}

// Carve R (2026-06-10): the semantic-embedder chain (singleton +
// semantic_text_embedding + semantic_or_hash_embedding) moved to
// `touring_hook_runtime::embeddings` — re-imported here so every
// `cli::shared::semantic_*` consumer path is unchanged.
pub(crate) use touring_hook_runtime::embeddings::{
    semantic_or_hash_embedding, semantic_text_embedding,
};

/// arctic-embed-m query instruction prefix (Snowflake/snowflake-arctic-embed-m
/// model card). Applied to the QUERY only — documents are embedded verbatim by
/// [`semantic_or_hash_embedding`] — to preserve the query↔document asymmetry the
/// model was trained on. Without it, query and corpus vectors drift into the
/// same region and cosine scores compress, degrading ranking.
pub(crate) const ARCTIC_QUERY_PREFIX: &str =
    "Represent this sentence for searching relevant passages: ";

/// The `~/.claude` directory (HOME-based), falling back to a relative
/// `.claude` when `HOME` is unset.
///
/// `pub` (ES2 P2): the single source of truth for the constitution root, shared
/// with the `touring attest-contract` CLI in touring-server (`HarnessContract`).
pub fn touring_claude_dir() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join(".claude"))
        .unwrap_or_else(|| std::path::PathBuf::from(".claude"))
}

/// Short, HOME-relative label for a `memory.db` path, used to tag a federated
/// recall row with the project it came from (e.g. `.claude/rust/.claude/...`).
fn memory_db_label(db: &std::path::Path) -> String {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .and_then(|home| {
            db.strip_prefix(&home)
                .ok()
                .map(|rel| rel.display().to_string())
        })
        .unwrap_or_else(|| db.display().to_string())
}

/// Discovers every project DB named `db_filename` for federated retrieval.
///
/// `primary` (the current project's DB) is always element 0 so its rows win
/// on key-dedup downstream. Then probes the canonical
/// `<root>/.claude/touring/<db_filename>` layout for `claude_dir` itself, its
/// parent (the global `~/.claude/touring/<db_filename>`), and the child and
/// grandchild directories of `claude_dir`. The scan is depth-bounded (it never
/// recurses past grandchildren) and fail-open: an unreadable directory is
/// skipped. Paths are canonicalized and de-duplicated; only existing files
/// are kept. Used for `memory.db` (federated recall) and `knowledge.db`
/// (federated PreToolUse lesson retrieval).
pub(crate) fn discover_canonical_dbs(
    primary: &std::path::Path,
    claude_dir: &std::path::Path,
    db_filename: &str,
) -> Vec<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = vec![primary.to_path_buf()];
    let mut roots: Vec<std::path::PathBuf> = vec![claude_dir.to_path_buf()];
    if let Some(parent) = claude_dir.parent() {
        roots.push(parent.to_path_buf());
    }
    if let Ok(rd) = std::fs::read_dir(claude_dir) {
        for child in rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
            if let Ok(rd2) = std::fs::read_dir(&child) {
                roots.extend(rd2.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
            }
            roots.push(child);
        }
    }
    let rel = format!(".claude/touring/{db_filename}");
    candidates.extend(roots.into_iter().map(|r| r.join(&rel)));
    let mut seen: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    candidates
        .into_iter()
        .map(|p| p.canonicalize().unwrap_or(p))
        .filter(|p| p.is_file() && seen.insert(p.clone()))
        .collect()
}

/// Federated recall — runs [`memory_recall_sql`] across every `memory.db` in
/// `dbs` and merges the rows, so a lesson stored under one project is found
/// from any other. `dbs[0]` (the current project's DB) is queried first, so
/// its rows win when the same `key` exists in several DBs. Each row is tagged
/// with `source_db` (a HOME-relative label) so the caller can show where a
/// lesson came from. Capped at 20 merged rows.
pub(crate) fn memory_recall_sql_federated(
    dbs: &[std::path::PathBuf],
    query: &str,
) -> Vec<serde_json::Value> {
    let mut merged: Vec<serde_json::Value> = vec![];
    let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    for db in dbs {
        let label = memory_db_label(db);
        for mut row in memory_recall_sql(db, query) {
            let key = row
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if key.is_empty() || !seen_keys.insert(key) {
                continue;
            }
            if let Some(obj) = row.as_object_mut() {
                obj.insert("source_db".to_string(), serde_json::json!(label));
            }
            merged.push(row);
            if merged.len() >= 20 {
                return merged;
            }
        }
    }
    merged
}

/// AhoCorasick automaton for keyword-to-skill-group routing.
///
/// Pattern groups (indexed by `skill_group(pattern_idx)`):
///   Group 0 → "touring index find"      (patterns 0-2)
///   Group 1 → "touring ast blast"       (patterns 3-5)
///   Group 2 → "touring memory recall"   (patterns 6-9)
///   Group 3 → "touring wiring orphans"  (patterns 10-12)
///   Group 4 → "touring gotcha match"    (patterns 13-15)
///   Group 5 → "touring evolution insights" (patterns 16-18)
///
/// Single-pass over `query_lower` replaces 18 sequential `.contains()` calls.
#[allow(clippy::incompatible_msrv)]
static SKILL_PATTERNS: std::sync::LazyLock<aho_corasick::AhoCorasick> =
    std::sync::LazyLock::new(|| {
        aho_corasick::AhoCorasick::new([
            "symbol",
            "find",
            "definition",
            "blast",
            "impact",
            "radius",
            "memory",
            "recall",
            "lesson",
            "pattern",
            "wiring",
            "orphan",
            "integration",
            "gotcha",
            "pitfall",
            "anti-pattern",
            "evolution",
            "insight",
            "drift",
        ])
        .expect("SKILL_PATTERNS: all literals are valid — infallible")
    });

/// Map an AhoCorasick pattern index to a skill group (0-5).
#[inline]
fn skill_group(pattern_idx: usize) -> usize {
    match pattern_idx {
        0..=2 => 0,
        3..=5 => 1,
        6..=9 => 2,
        10..=12 => 3,
        13..=15 => 4,
        16..=18 => 5,
        _ => 6,
    }
}

/// Keyword-based skill matching as fallback when no RL bandit is available.
///
/// Uses a [`SKILL_PATTERNS`] AhoCorasick automaton for a single-pass scan
/// over the lowercased query instead of 18 sequential `.contains()` calls.
/// Complexity: O(n + m) where n = query length, m = total pattern length.
pub(crate) fn keyword_skill_match(query: &str) -> Vec<serde_json::Value> {
    let query_lower = query.to_lowercase();
    let mut scored: Vec<(i32, &str, &str, &str)> = vec![
        (
            0,
            "touring index find",
            "high",
            "Find symbol definitions in the indexed codebase",
        ),
        (
            0,
            "touring ast blast",
            "medium",
            "Analyze blast radius for a file",
        ),
        (
            0,
            "touring memory recall",
            "medium",
            "Recall past patterns and lessons",
        ),
        (
            0,
            "touring wiring orphans",
            "low",
            "Find orphan pub symbols needing consumers",
        ),
        (
            0,
            "touring gotcha match",
            "low",
            "Check known pitfalls for a file",
        ),
        (
            0,
            "touring evolution insights",
            "low",
            "Review learned patterns and tool effectiveness",
        ),
        (
            0,
            "touring session start",
            "low",
            "Start a new touring session",
        ),
    ];
    const BOOST_TARGETS: [&str; 6] = [
        "touring index find",
        "touring ast blast",
        "touring memory recall",
        "touring wiring orphans",
        "touring gotcha match",
        "touring evolution insights",
    ];
    let mut boosted: [bool; 6] = [false; 6];
    for mat in SKILL_PATTERNS.find_iter(&query_lower) {
        let g = skill_group(mat.pattern().as_usize());
        if g < 6 {
            boosted[g] = true;
        }
    }
    for (group, target) in BOOST_TARGETS.iter().enumerate() {
        if boosted[group] {
            for entry in scored.iter_mut() {
                if entry.1 == *target {
                    entry.0 += 3;
                }
            }
        }
    }
    scored.sort_by_key(|b| std::cmp::Reverse(b.0));
    scored
        .into_iter()
        .take(3)
        .map(|(_, skill, relevance, description)| {
            serde_json::json!(
                { "skill" : skill, "relevance" : relevance, "description" : description,
                "source" : "keyword_fallback" }
            )
        })
        .collect()
}

/// Ensure decompose tables exist (idempotent). Called by create handler and
/// by the extracted `cli/workflow.rs` handlers (A-W2.P3).
pub(crate) fn ensure_decompose_tables(db: &crate::knowledge::FileKnowledgeDB) {
    let _ = db.conn_ref().execute_batch(
        "CREATE TABLE IF NOT EXISTS task_decompositions (
            task_id TEXT PRIMARY KEY,
            task_type TEXT NOT NULL,
            description TEXT NOT NULL,
            cila_level INTEGER NOT NULL DEFAULT 3,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            archived_at TEXT,
            status TEXT NOT NULL DEFAULT 'active',
            metrics TEXT
        );
        CREATE TABLE IF NOT EXISTS decomposition_subtasks (
            subtask_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            description TEXT NOT NULL,
            depends_on TEXT NOT NULL DEFAULT '[]',
            priority INTEGER NOT NULL DEFAULT 255,
            status TEXT NOT NULL,
            deadline TEXT,
            deadline_behavior TEXT DEFAULT 'Fail',
            parallel_group TEXT,
            review_required INTEGER NOT NULL DEFAULT 0,
            complexity_hint TEXT,
            retry_policy TEXT,
            attempts INTEGER NOT NULL DEFAULT 0,
            quality_score REAL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (task_id) REFERENCES task_decompositions(task_id)
        );
        CREATE INDEX IF NOT EXISTS idx_task_status ON task_decompositions(status);
        CREATE INDEX IF NOT EXISTS idx_subtasks_task ON decomposition_subtasks(task_id);",
    );
    let _ = db.conn_ref().execute(
        "ALTER TABLE task_decompositions ADD COLUMN origin TEXT NOT NULL DEFAULT 'claude-code'",
        [],
    );
    let _ = db.conn_ref().execute(
        "ALTER TABLE task_decompositions ADD COLUMN mirrored_to_cc INTEGER NOT NULL DEFAULT 1",
        [],
    );
    let _ = db
        .conn_ref()
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_task_origin_mirror ON task_decompositions(origin, mirrored_to_cc)",
            [],
        );
    let _ = db
        .conn_ref()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS cc_action_suggestions (
            suggestion_id TEXT PRIMARY KEY,
            action_type TEXT NOT NULL,
            target_task_id TEXT NOT NULL,
            target_subtask_id TEXT,
            reason TEXT NOT NULL,
            evidence_json TEXT NOT NULL DEFAULT '{}',
            suggested_at TEXT NOT NULL,
            consumed INTEGER NOT NULL DEFAULT 0,
            consumed_at TEXT,
            consumed_action TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_action_suggestions_pending ON cc_action_suggestions(action_type, consumed);
        CREATE INDEX IF NOT EXISTS idx_action_suggestions_target ON cc_action_suggestions(target_task_id);",
        );
    let _ = db.conn_ref().execute(
        "ALTER TABLE cc_action_suggestions ADD COLUMN surface_count INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = db.conn_ref().execute_batch(
        "CREATE TABLE IF NOT EXISTS action_type_deactivation (
            action_type TEXT PRIMARY KEY,
            consecutive_ignores INTEGER NOT NULL DEFAULT 0,
            deactivated_until TEXT
        );",
    );
    let _ = db
        .conn_ref()
        .execute(
            "ALTER TABLE action_type_deactivation ADD COLUMN acceptance_count INTEGER NOT NULL DEFAULT 0",
            [],
        );
    let _ = db.conn_ref().execute(
        "ALTER TABLE action_type_deactivation ADD COLUMN total_samples INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = db.conn_ref().execute_batch(
        "CREATE TABLE IF NOT EXISTS subtask_results (
            id TEXT PRIMARY KEY,
            subtask_id TEXT NOT NULL,
            started_at TEXT NOT NULL,
            completed_at TEXT,
            duration_ms INTEGER,
            cache_hit INTEGER NOT NULL DEFAULT 0,
            output_json TEXT,
            error TEXT,
            FOREIGN KEY (subtask_id) REFERENCES decomposition_subtasks(subtask_id)
        );
        CREATE INDEX IF NOT EXISTS idx_results_subtask ON subtask_results(subtask_id);
        CREATE INDEX IF NOT EXISTS idx_results_started ON subtask_results(started_at);",
    );
}

/// Extract the required `file_path` string from a CLI payload.
///
/// Returns `Ok(&str)` when the key is present and non-empty.  Returns
/// `Err(String)` with a canonical `{"error":"file_path required"}` JSON
/// response when the key is absent or empty.
///
/// Centralises the repetitive 6-line guard that every `cli_ast_*` /
/// `cli_wiring_*` handler previously inlined.  Each call site collapses to
/// three lines:
///
/// ```rust,ignore
/// let file_path = match crate::cli::shared::require_file_path(payload) {
///     Ok(fp) => fp,
///     Err(e) => return e,
/// };
/// ```
pub(crate) fn require_file_path(payload: &serde_json::Value) -> Result<&str, String> {
    let fp = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if fp.is_empty() {
        Err(serde_json::json!({"error": "file_path required"}).to_string())
    } else {
        Ok(fp)
    }
}

#[cfg(test)]
mod retrieval_reward_tests {
    use super::retrieval_coverage_reward;

    /// The whole point: the signal must MOVE with the outcome.
    ///
    /// The five call sites this replaced all emitted a literal `1.0`, so a
    /// query that found nothing and one that found everything were
    /// indistinguishable to the learner.
    #[test]
    fn coverage_discriminates_empty_from_full_results() {
        assert_eq!(retrieval_coverage_reward(0, 10), 0.0, "nothing found");
        assert_eq!(retrieval_coverage_reward(10, 10), 1.0, "fully satisfied");
        assert!(
            retrieval_coverage_reward(0, 10) < retrieval_coverage_reward(10, 10),
            "an empty result must not be rewarded like a full one"
        );
    }

    #[test]
    fn coverage_is_proportional_between_the_extremes() {
        let half = retrieval_coverage_reward(5, 10);
        assert!((half - 0.5).abs() < f64::EPSILON, "5 of 10 is 0.5, got {half}");
    }

    #[test]
    fn overshooting_the_request_saturates_at_one() {
        assert_eq!(
            retrieval_coverage_reward(50, 10),
            1.0,
            "reward must stay within the [-1, 1] band the engine clamps to"
        );
    }

    #[test]
    fn a_zero_request_degrades_to_found_anything_at_all() {
        assert_eq!(retrieval_coverage_reward(0, 0), 0.0);
        assert_eq!(
            retrieval_coverage_reward(3, 0),
            1.0,
            "0 is meaningless as a denominator — never divide by it"
        );
    }
}
