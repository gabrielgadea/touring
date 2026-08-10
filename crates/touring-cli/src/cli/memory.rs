//! CLI memory handlers (`cli_memory_*`) — extracted from cli_handlers.rs (A-W2.P3).
//!
//! Recall (RRF-fused federated), store, reindex, stats, list. Shared helpers
//! (`semantic_or_hash_embedding`, `discover_canonical_dbs`,
//! `memory_recall_sql_federated`, `touring_claude_dir`) stay in cli_handlers.rs
//! and are referenced via `crate::cli_handlers::`.

use crate::cli_handlers::{
    ARCTIC_QUERY_PREFIX, GotchaStats, KnowledgeStats, discover_canonical_dbs,
    memory_recall_sql_federated, semantic_or_hash_embedding, semantic_text_embedding,
    touring_claude_dir,
};
use crate::runtime::HookRuntime;
use rusqlite::params;
use touring_analysis::e2e::schema_guard;

/// Entries a single `memory reindex` call will embed before yielding.
///
/// The handler runs inside the daemon's actor, so an unbounded pass monopolises
/// the memory subsystem: on 2026-08-02 a full re-embed of 6.921 entries outlived
/// the client's ~15s read timeout, reported `success=false`, and left every
/// subsequent memory RPC queued behind it for minutes. Bounding the call keeps
/// the actor responsive; `remaining` in the response tells the caller to run
/// again.
const DEFAULT_REINDEX_BUDGET: u64 = 2_000;

/// Key namespace of auto-recorded tool outcomes (`outcome:bash:…:failure`).
const OUTCOME_PREFIX: &str = "outcome:";

/// Drop auto-recorded outcomes from a recall source unless explicitly requested.
///
/// Measured 2026-08-02 on a 6.921-entry store: `outcome:*` was 50,2 % of the
/// corpus and — decisively — the **eight most-recalled entries in the entire
/// store**, led by one with 1.158 retrievals. Automatic entries were returned
/// 2,4× more often than curated lessons (2,11 vs 0,88 average), so the ACO
/// pheromone was diluted by its own exhaust. They stay stored and searchable via
/// `--include-outcomes`; they just no longer crowd the default channel.
fn filter_outcomes(entries: Vec<serde_json::Value>, include: bool) -> Vec<serde_json::Value> {
    if include {
        return entries;
    }
    entries
        .into_iter()
        .filter(|e| {
            !e.get("key")
                .and_then(|k| k.as_str())
                .is_some_and(|k| k.starts_with(OUTCOME_PREFIX))
        })
        .collect()
}

/// The measured value of a case, in `[-1.0, 1.0]`, or `None` when unobserved.
///
/// Two sources, in order of authority:
///
/// 1. An explicit `outcome_reward`, written by `touring memory store --reward`.
/// 2. The `outcome:<tool>:<sig>:<verdict>` key suffix, which has been recording
///    verdicts all along — 3.448 `:failure` and 29 `:success` entries in the
///    live store on 04/08/2026. The `r` was already there; nothing read it.
///
/// `None` is deliberately NOT 0.0. An unobserved case is not a failed one, and
/// collapsing the two would teach the ranker that "unmeasured" means "bad" —
/// which would bury every curated lesson, since curated lessons are exactly the
/// entries nobody attached a reward to.
fn case_value(entry: &serde_json::Value) -> Option<f64> {
    if let Some(explicit) = entry
        .get("outcome_reward")
        .and_then(serde_json::Value::as_f64)
    {
        return Some(explicit.clamp(-1.0, 1.0));
    }
    // A mined repair carries a VALIDATED fix, so it is checked before the key
    // suffix — its key says `:failure` while its content is a success. See
    // [`repair_from`].
    if repair_from(entry).is_some() {
        return Some(1.0);
    }
    let key = entry.get("key").and_then(|k| k.as_str())?;
    if !key.starts_with(OUTCOME_PREFIX) {
        return None;
    }
    if key.ends_with(":success") {
        Some(1.0)
    } else if key.ends_with(":failure") {
        // Reached only when no resolution is attached — the CEG's blocked
        // dry-runs (30 of 3.478 entries on 04/08/2026, and 0 of them carry a
        // resolution). Those are the bank's only genuine negatives.
        Some(0.0)
    } else {
        None
    }
}

/// The two halves of a mined repair: the error observed, and the input that
/// resolved it.
///
/// # Why a `:failure` key can hold a success
///
/// `transcript_miner::redacted_lesson_value` persists
/// `{tool, error, resolution_input, session_id, timestamp}`. Two facts about
/// that record decide this whole classification:
///
/// 1. `resolution_input` is the input of a `ToolUse` whose `ToolResult` was
///    **not** an error — the miner's chain scan emits a pair only on reaching a
///    success (`transcript_miner.rs:463`, *"Success: this is the resolution
///    candidate"*), and only when it arrived within `RESOLUTION_SCAN_WINDOW`.
///    The miner's own doc states the criterion: *"unresolved failures are not
///    actionable lessons"*. So every mined case is a **validated** fix.
/// 2. The **failed input is deliberately not persisted**. A mined case
///    therefore has no negative half at all; it is a positive case whose
///    situation happens to be an error.
///
/// Measured on the live store (04/08/2026): 3.448 of 3.478 `outcome:*` entries
/// are mined repairs, and 3.448 of those 3.448 carry a `resolution_input`.
/// Classifying them by the `:failure` suffix threw away the entire positive
/// half of the bank — the defect this function exists to close.
///
/// Fail-open: an unparseable or incomplete value yields `None`, so the caller
/// falls back to the key convention rather than losing the entry.
fn repair_from(entry: &serde_json::Value) -> Option<(String, String)> {
    let raw = entry.get("value").and_then(|v| v.as_str())?;
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let when = json_field_as_text(parsed.get("error"))?;
    let then = json_field_as_text(parsed.get("resolution_input"))?;
    Some((when, then))
}

/// Read a JSON field as display text, whether it was stored as a string or as a
/// structured value.
///
/// The live store holds `error` as a **string** and `resolution_input` as an
/// **object** (`{"file_path": …}`, `{"command": …}`). Requiring `as_str()` on
/// both matched 0 of 3.448 real repairs while every hand-built test fixture
/// passed — the fixture asserted the shape I assumed, not the shape the miner
/// writes (cross-audit 04/08/2026).
///
/// Empty strings and JSON nulls/empties are treated as absent: a case with no
/// action to take is not a repair.
fn json_field_as_text(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => (!s.is_empty()).then(|| s.clone()),
        other => {
            let rendered = other.to_string();
            (rendered != "{}" && rendered != "[]").then_some(rendered)
        }
    }
}

/// Render an entry as an actionable case: a repair becomes `when` → `do`.
///
/// Memento presents a positive case as `Question: … Plan: …`; LangMem as
/// `When: … Did: …`. Both surface the *situation* and the *action* as separate,
/// named fields. Touring stored exactly that pair and returned it as an opaque
/// JSON string in `value`, so the actionable half was invisible to the caller
/// even once the entry was ranked first.
fn shape_case(entry: &serde_json::Value) -> serde_json::Value {
    let Some((when, then)) = repair_from(entry) else {
        return entry.clone();
    };
    let mut out = entry.clone();
    if let Some(obj) = out.as_object_mut() {
        obj.insert("when".into(), serde_json::json!(when));
        obj.insert("do".into(), serde_json::json!(then));
        obj.insert("case_kind".into(), serde_json::json!("repair"));
    }
    out
}

/// Reorder a recall result set by measured value, keeping similarity order
/// within each value class.
///
/// This is the Touring form of Memento's read operator (arXiv 2508.16153,
/// Eq. 7/16): retrieval ranked by the *value* of a case, not by resemblance
/// alone. Touring's recall fused BM25 + TF-IDF + ANN through RRF and never
/// consulted an outcome — `recall.rs` contains no `reward`/`value`/`utility`
/// term at all.
///
/// The sort is **stable** and three-classed on purpose:
///
/// - proven-good cases first,
/// - unobserved next (curated lessons live here — they must not be punished for
///   lacking a measurement),
/// - proven-bad last (still returned: knowing what failed is why Memento writes
///   failures to the bank in the first place, Eq. 12).
///
/// Stability is what fuses the two signals: value decides the class, and the
/// RRF similarity ranking decides the order inside it.
fn rerank_by_case_value(mut entries: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    // Rank key: 0 = proven good, 1 = unobserved, 2 = proven bad.
    fn class_of(entry: &serde_json::Value) -> u8 {
        match case_value(entry) {
            Some(v) if v > 0.5 => 0,
            None => 1,
            Some(_) => 2,
        }
    }
    entries.sort_by_key(class_of);
    entries
}

/// Cases shown per value class in the partitioned view.
///
/// Memento's ablation (arXiv 2508.16153, Tab. 3) peaks at **K = 4** retrieved
/// cases on DeepResearcher (64,5 F1) and *declines* past it: K=0→1 is worth
/// +3,7 F1, K=1→4 only +0,9, and K=8/16/32 are all worse than K=4. Its own
/// runtime caps are per class (`MEMORY_MAX_POS_EXAMPLES` /
/// `MEMORY_MAX_NEG_EXAMPLES`), never a single shared budget — and that is the
/// point: a shared top-K cannot guarantee the minority class any slots at all.
///
/// Touring's bank is 99,1 % negative (3.448 failures / 29 successes, measured
/// 04/08/2026), so under one shared budget the positives are mathematically
/// certain to be crowded out. Per-class caps are the only structure that
/// survives that imbalance.
const MAX_CASES_PER_CLASS: usize = 4;

/// Split a recall result into labelled value classes.
///
/// # Why labelling, not just ordering
///
/// [`rerank_by_case_value`] orders by value, which helps a ranked consumer but
/// tells it *nothing about what to do with each item*. Memento's
/// `build_prompt_from_cases` does something categorically stronger: it splits
/// the retrieved cases into `Positive Examples (reward=1)` and
/// `Negative Examples (reward=0)` sections and appends the instruction
/// *"Focus on the positive examples and avoid the patterns shown in negative
/// examples."* LangMem formats episodes the same way — labelled
/// `When/Thought/Did/Result` fields, never an opaque blob.
///
/// That difference is what makes a failure *useful*. Unlabelled, a failed case
/// at rank 15 is indistinguishable from a mediocre success, so the only safe
/// thing to do with a bank of failures is to drop it — which is exactly what
/// the 02/08/2026 prefix filter had to do. Labelled, the same failures become
/// the "avoid this" half of the evidence.
///
/// `unobserved` is kept as its own class rather than folded into either side:
/// curated lessons live there, and they were never scored.
fn partition_cases(entries: &[serde_json::Value]) -> serde_json::Value {
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    let mut unobserved = Vec::new();

    for entry in entries {
        let bucket = match case_value(entry) {
            Some(v) if v > 0.5 => &mut positive,
            None => &mut unobserved,
            Some(_) => &mut negative,
        };
        if bucket.len() < MAX_CASES_PER_CLASS {
            // `shape_case` surfaces a repair's `when`/`do` fields; anything else
            // passes through untouched.
            bucket.push(shape_case(entry));
        }
    }

    serde_json::json!({
        "positive": positive,
        "negative": negative,
        "unobserved": unobserved,
        "guidance": "Reuse the approach in `positive` (cases whose outcome was \
    measured as good). Treat `negative` as patterns to avoid, never as guidance. \
    `unobserved` carries no verdict — judge it on its own merits.",
        "cap_per_class": MAX_CASES_PER_CLASS,
    })
}

/// Attach the measured value to each entry so callers can see WHY the order is
/// what it is. Absent stays absent — never serialised as 0.0.
fn annotate_case_value(entries: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    entries
        .into_iter()
        .map(|mut e| {
            let value = case_value(&e);
            if let (Some(v), Some(obj)) = (value, e.as_object_mut()) {
                obj.insert("case_value".into(), serde_json::json!(v));
            }
            e
        })
        .collect()
}

/// Weight a fresh observation carries when blended into a case's value.
///
/// Deliberately slow: one task's verdict is weak evidence about a case that
/// merely appeared in its context, so a single bad run must not condemn a case
/// that has been useful. Ten consistent observations move the value most of the
/// way; one moves it a fifth.
const CASE_CREDIT_ALPHA: f64 = 0.2;

/// Normalise a query into the key that joins a recall to its later verdict.
///
/// Whitespace and case are noise here — the credit arrives from a different
/// process (a phase close, a gate) that re-states the query, not from the
/// recall's own memory of it.
fn credit_key(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Credit the cases a previous recall served with the verdict of the work they
/// informed.
///
/// This closes the loop the case bank never had: recall → use → measure →
/// reinforce → better recall. It is Memento's episodic-control estimate
/// (arXiv 2508.16153, Eq. 9) reduced to its online form — the value of a case
/// is the running average of the outcomes of interactions with it, kept in the
/// `outcome_reward` column so it feeds straight back into `case_value` and the
/// ranking.
///
/// A recall is claimed **once**: the ledger removes it, so a second verdict for
/// the same query finds nothing rather than double-counting one retrieval.
///
/// Payload: `{"query": "<the recall query>", "reward": <-1.0..1.0>}`.
pub fn cli_memory_credit(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let reward = payload
        .get("reward")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    if query.is_empty() {
        return serde_json::json!({ "error": "query is required" }).to_string();
    }

    let Some(pending) = rt.learning.case_ledger.take(&credit_key(query)) else {
        // Not an error: crediting a query nobody recalled is a no-op, and the
        // count of these is itself the signal that a caller is crediting the
        // wrong key.
        return serde_json::json!({
            "credited": 0, "query": query, "reason": "no pending recall for this query",
        })
        .to_string();
    };

    // Credit across the SAME federated set the recall reads. `memory recall`
    // searches every project's `memory.db` (7 of them on this machine), so a
    // case served from another project could never be credited against the
    // local database alone — the loop would silently stay open for exactly the
    // cross-project lessons federation exists to surface (cross-audit
    // 04/08/2026).
    let memory_db_path = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    let memory_dbs = discover_canonical_dbs(&memory_db_path, &touring_claude_dir(), "memory.db");

    let mut updated = 0usize;
    for db in &memory_dbs {
        let Ok(conn) = rusqlite::Connection::open(db) else {
            continue;
        };
        for key in &pending.payload {
            let prior: Option<f64> = conn
                .query_row(
                    "SELECT outcome_reward FROM memory_entries WHERE key = ?1",
                    params![key],
                    |r| r.get::<_, Option<f64>>(0),
                )
                .ok()
                .flatten();
            let blended = touring_intelligence::rl::bandit::blend_case_value(
                prior,
                reward,
                CASE_CREDIT_ALPHA,
            );
            // `execute` returns ROWS AFFECTED. Counting `is_ok()` scored a
            // no-op update as a credit, so the one number that reports whether
            // attribution is working would have reported success while writing
            // nothing — the failure mode an audit exists to catch.
            if let Ok(rows) = conn.execute(
                "UPDATE memory_entries SET outcome_reward = ?1 WHERE key = ?2",
                params![blended, key],
            ) {
                updated += rows;
            }
        }
    }

    serde_json::json!({
        "credited": updated,
        "query": query,
        "reward": reward,
        "served": pending.payload.len(),
        "ledger_credited_total": rt.learning.case_ledger.credited_count(),
        "ledger_unclaimed_evictions": rt.learning.case_ledger.unclaimed_evictions(),
    })
    .to_string()
}

/// Rows still owed an ANN embedding (or all of them, when `all` is set).
///
/// `embeddings.id` == `memory_entries.key` in the SAME database, so "what is
/// missing" is one LEFT JOIN — no cross-db bookkeeping and no full re-embed just
/// to discover that nothing changed.
fn reindex_candidates(
    conn: &rusqlite::Connection,
    all: bool,
) -> Result<Vec<(String, String)>, String> {
    let select = if all {
        "SELECT key, value FROM memory_entries ORDER BY rowid"
    } else {
        "SELECT me.key, me.value FROM memory_entries me
         LEFT JOIN embeddings em ON em.id = me.key
         WHERE em.id IS NULL ORDER BY me.rowid"
    };
    let mut stmt = conn
        .prepare(select)
        .map_err(|e| format!("failed to prepare SELECT: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("query failed: {e}"))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

/// Reports aggregate memory store statistics (entry counts across knowledge tables) as JSON.
pub fn cli_memory_stats(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    // Destructured to match `gotcha_stats()`'s documented contract —
    // (total_count, total_hits, total_prevented). Naming the 2nd/3rd slots
    // "unresolved"/"resolved" is what produced `unresolved_count: 383107` for
    // 13 gotchas (2026-08-02).
    let (gotcha_total, gotcha_hits, gotcha_prevented) = db.gotcha_stats();
    let file_count: usize = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0) as usize;
    let relation_count: usize = db.all_file_relations().len();
    let bash_count: i64 = db
        .conn_ref()
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_BASH_OUTCOMES),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let edit_count: i64 = db
        .conn_ref()
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_EDIT_HISTORY),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let status = KnowledgeStats {
        file_count,
        relation_count,
        bash_outcome_count: bash_count,
        edit_event_count: edit_count,
        gotcha_stats: GotchaStats {
            total_count: gotcha_total,
            total_hits: gotcha_hits as usize,
            total_prevented: gotcha_prevented as usize,
        },
        memory_entry_count: {
            let memory_db_path =
                touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
            rusqlite::Connection::open(&memory_db_path)
                .and_then(|conn| {
                    conn.query_row("SELECT COUNT(*) FROM memory_entries", [], |r| {
                        r.get::<_, i64>(0)
                    })
                })
                .unwrap_or(0) as usize
        },
    };
    serde_json::to_string(&status)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
/// Recalls memory entries for a query via RRF-fused federated search across canonical databases as JSON.
pub fn cli_memory_recall(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
    if query.is_empty() {
        return serde_json::json!({ "entries" : [], "count" : 0, "query" : "" }).to_string();
    }
    let include_outcomes = payload
        .get("include_outcomes")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let memory_db_path = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    // Federated recall — search the current project's memory.db AND every
    // other project's, so a lesson stored under one project is recallable
    // from any other (the `where` half of "always know where to look").
    let memory_dbs = discover_canonical_dbs(&memory_db_path, &touring_claude_dir(), "memory.db");
    let entries = memory_recall_sql_federated(&memory_dbs, query);
    let ann_results: Vec<serde_json::Value> = {
        let borrow = rt.ctx.ann_recall.borrow();
        if let Some(recall) = borrow.as_ref() {
            let embedding = memory_recall_query_embedding(query);
            let start = std::time::Instant::now();
            let neighbors = recall.search(&embedding, 20);
            let elapsed_us = start.elapsed().as_micros() as u64;
            crate::shared::gate_metrics::record_ann_search_latency_us(elapsed_us);
            neighbors
                .into_iter()
                .map(|r| {
                    serde_json::json!(
                        { "key" : r.id, "value" : r.content, "score" : r.score, "source"
                        : "ann", }
                    )
                })
                .collect()
        } else {
            vec![]
        }
    };
    let tfidf_results: Vec<serde_json::Value> = memory_recall_tfidf(rt, query, 20);
    // The labelled-case view is built from the UNFILTERED candidates, before
    // the prefix filter below removes the auto-recorded outcomes.
    //
    // Those outcomes had to be dropped wholesale on 02/08/2026 because,
    // unlabelled, a bank that is 99,1 % failures is indistinguishable from
    // guidance — the swamping problem (Francis & Ram 1993, cited by Memento
    // §2.3). Labelled AND capped at `MAX_CASES_PER_CLASS`, the same failures
    // become bounded negative evidence: at most 4 of them, explicitly marked
    // "avoid", can never crowd out a curated lesson.
    //
    // `entries` below stays byte-identical to the pre-04/08 behaviour — this
    // is a new channel, not a change to the old one.
    let cases = partition_cases(&memory_recall_rrf_merge_n(
        &[&entries[..], &ann_results[..], &tfidf_results[..]],
        60,
    ));

    // Drop auto-recorded tool outcomes unless explicitly asked for. Filtering
    // BEFORE the RRF merge matters: filtering after would let noise consume the
    // 20 result slots and then be discarded, starving the curated lessons that
    // should have taken them.
    let entries = filter_outcomes(entries, include_outcomes);
    let ann_results = filter_outcomes(ann_results, include_outcomes);
    let tfidf_results = filter_outcomes(tfidf_results, include_outcomes);
    let entries_len = entries.len();
    let merged_entries: Vec<serde_json::Value> =
        if ann_results.is_empty() && tfidf_results.is_empty() {
            entries
        } else {
            memory_recall_rrf_merge_n(&[&entries[..], &ann_results[..], &tfidf_results[..]], 20)
        };
    // Value-ranked read (Memento Eq. 7/16): RRF has ordered by resemblance;
    // now let the MEASURED outcome decide which of those the caller sees first.
    // Stable, so similarity still orders within each value class.
    let merged_entries = annotate_case_value(rerank_by_case_value(merged_entries));

    // Record which cases this recall served, so a later verdict can be joined
    // back onto exactly them (Memento Eq. 9 — episodic control). Nothing else
    // in Touring can answer "did this case help?": `access_count` counts writes
    // and exact-key reads, and this recall path never touches it.
    let served_keys: Vec<String> = merged_entries
        .iter()
        .chain(cases["positive"].as_array().into_iter().flatten())
        .filter_map(|e| e.get("key").and_then(|k| k.as_str()))
        .map(str::to_string)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if !served_keys.is_empty() {
        rt.learning
            .case_ledger
            .record(credit_key(query), served_keys);
    }
    #[cfg(feature = "tantivy-fts")]
    let symbol_context: Vec<serde_json::Value> = {
        crate::tantivy_index::tantivy_for(Some(&rt.project_root))
            .and_then(|idx| idx.search(query, 5).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|hit| {
                serde_json::json!(
                    { "symbol_name" : hit.symbol_name, "file_path" : hit.file_path,
                    "symbol_kind" : hit.symbol_kind, "line_number" : hit.line_number,
                    "score" : hit.score, }
                )
            })
            .collect()
    };
    #[cfg(not(feature = "tantivy-fts"))]
    let symbol_context: Vec<serde_json::Value> = vec![];
    let memory_diagnostics: Vec<serde_json::Value> = {
        use crate::memory_finding::MemoryFinding;
        let mut diags = vec![];
        if merged_entries.is_empty() {
            let f = MemoryFinding::RecallEmpty {
                query: query.to_string(),
            };
            tracing::info!(code = f.code_str(), % query, "recall empty for query");
            diags.push(serde_json::json!(
                { "code" : f.code_str(), "severity" : "info", "message" :
                format!("No memory entries found for query: {query}") }
            ));
        }
        if !tfidf_results.is_empty() {
            let f = MemoryFinding::TfidfActivated {
                candidate_count: tfidf_results.len(),
                corpus_size: entries_len,
            };
            tracing::debug!(
                code = f.code_str(),
                candidate_count = tfidf_results.len(),
                "tfidf activated"
            );
            diags.push(serde_json::json!(
                { "code" : f.code_str(), "severity" : "debug", "message" :
                format!("TF-IDF activated: {} candidates from corpus of {entries_len}",
                tfidf_results.len()) }
            ));
        }
        if !ann_results.is_empty() || !tfidf_results.is_empty() {
            let source_count = usize::from(entries_len > 0)
                + usize::from(!ann_results.is_empty())
                + usize::from(!tfidf_results.is_empty());
            let f = MemoryFinding::RrfFusion {
                source_count,
                merged_count: merged_entries.len(),
            };
            tracing::debug!(code = f.code_str(), source_count, "rrf fusion");
            diags.push(serde_json::json!(
                { "code" : f.code_str(), "severity" : "debug", "message" :
                format!("RRF fusion from {source_count} sources → {} results",
                merged_entries.len()) }
            ));
        }
        diags
    };
    let entry_count = merged_entries.len();
    let ann_count = ann_results.len();
    serde_json::json!(
        { "entries" : merged_entries, "count" : entry_count, "query" : query,
        "ann_results" : ann_count, "symbol_context" : symbol_context, "diagnostics" :
        memory_diagnostics, "cases" : cases, }
    )
    .to_string()
}
/// Query embedding for the ANN recall path. Semantic (with the arctic query
/// prefix) when available, else the raw-query 64-dim hash. The prefix is applied
/// only here, never to stored documents — that asymmetry is what makes
/// retrieval ranking discriminative.
fn memory_recall_query_embedding(query: &str) -> Vec<f32> {
    semantic_text_embedding(&format!("{ARCTIC_QUERY_PREFIX}{query}"))
        .unwrap_or_else(|| crate::ann_memory::query_hash_embedding(query))
}
/// Merge N ranked result lists via Reciprocal Rank Fusion (k=60).
///
/// Each list contributes `1 / (rank + 1 + k)` per entry. Entries with the
/// same `key` accumulate scores across lists. Returns up to `limit` entries
/// sorted by descending combined RRF score.
///
/// **List ordering matters for tie-breaks** (the canonical value taken on
/// duplicate keys is the first occurrence — typically SQL > ANN > TF-IDF).
/// SQL entries carry `tier` / `type` metadata that other sources lack;
/// putting SQL first preserves that fidelity.
///
/// History:
/// - Wave 22: original 2-list signature `(sql, ann)`.
/// - Wave M2 (2026-04-25, Hard Rule #11 reescopo): generalised to N lists so
///   TF-IDF (Wave M1) can plug in as a third orthogonal source.
fn memory_recall_rrf_merge_n(
    lists: &[&[serde_json::Value]],
    limit: usize,
) -> Vec<serde_json::Value> {
    use std::collections::HashMap;
    const RRF_K: f64 = 60.0;
    let mut rrf_map: HashMap<String, (f64, serde_json::Value)> = HashMap::new();
    for list in lists {
        for (rank, entry) in list.iter().enumerate() {
            let key = entry
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if key.is_empty() {
                continue;
            }
            let score = 1.0 / (rank as f64 + 1.0 + RRF_K);
            let e = rrf_map.entry(key).or_insert((0.0, entry.clone()));
            e.0 += score;
        }
    }
    let mut merged: Vec<(f64, serde_json::Value)> = rrf_map.into_values().collect();
    merged.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    merged.into_iter().take(limit).map(|(_, v)| v).collect()
}
/// Build (or load from cache) a TF-IDF index over the touring memory corpus
/// and return up to `top_k` hits as RRF-ready JSON entries.
///
/// Cold path: rebuilds the index when the cache is absent or older than
/// `tfidf_retriever::CACHE_TTL_SECS` (1 hour). Hot path: reuses the cached
/// index. Failures (missing dbs, empty corpus) degrade silently to an empty
/// vec — the consumer (cli_memory_recall) already runs SQL + ANN paths in
/// parallel so a missing third source is non-fatal.
fn memory_recall_tfidf(rt: &mut HookRuntime, query: &str, top_k: usize) -> Vec<serde_json::Value> {
    use crate::tfidf_retriever::{CACHE_TTL_SECS, TfidfIndex, default_cache_path};
    let cache_path = default_cache_path(&rt.project_root);
    let index = match TfidfIndex::load_cache(&cache_path, CACHE_TTL_SECS) {
        Ok(Some(idx)) => idx,
        _ => {
            let memory_db =
                touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
            let knowledge_db =
                touring_foundation::TouringConfig::knowledge_db_canonical(&rt.project_root);
            match TfidfIndex::build_from_db(&memory_db, &knowledge_db) {
                Ok(idx) => {
                    if let Err(e) = idx.save_cache(&cache_path) {
                        tracing::debug!(
                            target : "touring::tfidf", "tfidf cache persist failed: {e}"
                        );
                    }
                    idx
                }
                Err(_) => return Vec::new(),
            }
        }
    };
    index
        .query(query, top_k)
        .into_iter()
        .map(|hit| {
            let normalized_key = hit
                .key
                .strip_prefix("memory:")
                .map(str::to_string)
                .unwrap_or(hit.key);
            serde_json::json!(
                { "key" : normalized_key, "value" : hit.snippet, "score" : hit.score,
                "source" : format!("tfidf:{}", hit.source), }
            )
        })
        .collect()
}
// Carve R (2026-06-10): runtime-service handler moved to touring-hook-runtime::ceg_impls
// (it is a pure HookRuntime capability); re-exported at the historical path.
pub use touring_hook_runtime::ceg_impls::cli_memory_store;
/// Backfill the ANN corpus from all existing `memory_entries` rows. S-04 (2026-05-29).
///
/// Walks every row in `memory_entries`, generates a 64-dim hash embedding for
/// each value, and upserts in batches into the ANN corpus (idempotent via
/// `add_batch`'s INSERT OR REPLACE). This is what populates an empty ANN corpus
/// so `memory recall` returns ANN hits instead of degrading to FTS/TF-IDF only.
/// Payload: `{"batch_size": N}` (default 256).
pub fn cli_memory_reindex(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let batch_size = payload
        .get("batch_size")
        .and_then(|v| v.as_u64())
        .unwrap_or(256) as usize;

    // `all: true` re-embeds every entry; the default only backfills entries that
    // are MISSING from the ANN corpus. `max_entries` bounds a single call.
    let reindex_all = payload
        .get("all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let max_entries = payload
        .get("max_entries")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_REINDEX_BUDGET) as usize;

    if rt.ctx.ann_recall.borrow().is_none() {
        return serde_json::json!({
            "error": "ANN recall not initialised — daemon startup did not call init_ann_memory"
        })
        .to_string();
    }

    let memory_db_path = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    let conn = match rusqlite::Connection::open(&memory_db_path) {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!({ "error": format!("cannot open memory.db: {e}") })
                .to_string();
        }
    };
    let candidates = match reindex_candidates(&conn, reindex_all) {
        Ok(rows) => rows,
        Err(e) => return serde_json::json!({ "error": e }).to_string(),
    };

    let total_candidates = candidates.len();
    let budgeted = total_candidates.min(max_entries);
    let remaining = total_candidates - budgeted;
    let mut indexed = 0usize;
    let mut failed = 0usize;
    for chunk in candidates[..budgeted].chunks(batch_size) {
        let entries: Vec<crate::ann_memory::MemoryEntry> = chunk
            .iter()
            .map(|(key, value)| {
                let emb = semantic_or_hash_embedding(value);
                crate::ann_memory::MemoryEntry::new(key.as_str(), value.as_str(), emb)
            })
            .collect();
        let mut borrow = rt.ctx.ann_recall.borrow_mut();
        if let Some(ann) = borrow.as_mut() {
            match ann.add_batch(entries) {
                Ok(()) => indexed += chunk.len(),
                Err(e) => {
                    tracing::warn!("ANN reindex batch failed: {e}");
                    failed += chunk.len();
                }
            }
        }
    }

    serde_json::json!({
        "mode": if reindex_all { "all" } else { "incremental" },
        "total_candidates": total_candidates,
        "indexed": indexed,
        "failed": failed,
        "remaining": remaining,
        "batch_size": batch_size,
        "max_entries": max_entries,
        "status": if failed > 0 { "partial" }
                  else if remaining > 0 { "incomplete" }
                  else { "ok" },
    })
    .to_string()
}
/// Lists stored memory entries as JSON, ordered by `sort` (default: most-recalled).
///
/// Reads `memory.db/memory_entries` — the SAME store as `store`, `recall` and
/// `stats`. Until 2026-08-02 this queried `knowledge.db/file_knowledge` for rows
/// whose `file_path` began with `__memory__:`, a long-abandoned encoding that
/// stashed memory entries as pseudo-files. Measured that day: `file_knowledge`
/// held 4.270 rows and **zero** with that prefix, while `memory_entries` held
/// 6.923 — so `list` reported `count: 0` on a full store, making the whole
/// surface useless for manual inspection.
pub fn cli_memory_list(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let limit = payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as i64;
    let sort_field = payload
        .get("sort")
        .and_then(|v| v.as_str())
        .unwrap_or("access_count");
    let order = memory_list_order_clause(sort_field);
    let memory_db_path = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    let conn = match rusqlite::Connection::open(&memory_db_path) {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!(
                { "error" : format!("cannot open memory.db: {e}"), "entries" : [], "count" : 0 }
            )
            .to_string();
        }
    };
    // S4 (2026-08-07): the three weight columns join the projection, resolved
    // per-connection because a federated / older memory.db may not have them.
    let reward_col = crate::cli::shared::optional_column_select(&conn, "", "outcome_reward");
    let importance_col = crate::cli::shared::optional_column_select(&conn, "", "importance");
    let pinned_col = crate::cli::shared::optional_column_select(&conn, "", "pinned");
    let superseded_col = crate::cli::shared::optional_column_select(&conn, "", "superseded_by");
    // An ORDER BY naming a column this DB lacks would fail to prepare and empty
    // the listing — the silent-empty failure mode the recall path already
    // learned the hard way. Fall back to the default ordering instead.
    let order = match order {
        "ORDER BY outcome_reward DESC" if reward_col == "NULL" => "ORDER BY access_count DESC",
        "ORDER BY importance DESC" if importance_col == "NULL" => "ORDER BY access_count DESC",
        other => other,
    };
    let query = format!(
        "SELECT key, value, tier, entry_type, access_count, COALESCE(last_accessed_at, ''),
                {reward_col}, {importance_col}, {pinned_col}, {superseded_col}
         FROM memory_entries {order} LIMIT ?1"
    );
    let mut stmt = match conn.prepare(&query) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!(
                { "error" : format!("query failed: {e}"), "entries" : [], "count" : 0 }
            )
            .to_string();
        }
    };
    let entries: Vec<serde_json::Value> = stmt
        .query_map(params![limit], |row| {
            let mut entry = serde_json::json!({
                "key": row.get::<_, String>(0)?,
                "value": row.get::<_, String>(1)?,
                "tier": row.get::<_, String>(2)?,
                "type": row.get::<_, String>(3)?,
                "access_count": row.get::<_, i64>(4)?,
                "last_accessed": row.get::<_, String>(5)?,
            });
            if let Some(obj) = entry.as_object_mut() {
                if let Some(r) = row.get::<_, Option<f64>>(6).ok().flatten() {
                    obj.insert("outcome_reward".into(), serde_json::json!(r));
                }
                if let Some(i) = row.get::<_, Option<i64>>(7).ok().flatten() {
                    obj.insert("importance".into(), serde_json::json!(i));
                }
                if row.get::<_, Option<i64>>(8).ok().flatten().unwrap_or(0) != 0 {
                    obj.insert("pinned".into(), serde_json::json!(true));
                }
                if let Some(s) = row.get::<_, Option<String>>(9).ok().flatten() {
                    obj.insert("superseded_by".into(), serde_json::json!(s));
                }
            }
            Ok(entry)
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    let count = entries.len();
    // The pheromone-health number, reported because it was invisible: 11 of
    // 7360 entries carried a reward when this was written (0,15%), so the
    // `positive`/`negative`/`unobserved` guidance recall prints was operating
    // on a corpus that is 99,85% unobserved. A ratio nobody can see is a ratio
    // nobody feeds.
    let scored: i64 = conn
        .query_row(
            "SELECT COUNT(outcome_reward) FROM memory_entries",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_entries", [], |r| r.get(0))
        .unwrap_or(0);
    serde_json::json!({
        "entries": entries,
        "count": count,
        "corpus": { "total": total, "with_reward": scored },
    })
    .to_string()
}
/// Helper: determine ORDER BY clause for memory list queries.
///
/// Columns are those of `memory_entries`; the previous clauses named
/// `read_count` / `last_read_at`, which exist only in the abandoned
/// `file_knowledge` encoding and would now fail to prepare.
fn memory_list_order_clause(sort_field: &str) -> &'static str {
    match sort_field {
        "last_accessed" | "last_accessed_at" | "last_read_at" => "ORDER BY last_accessed_at DESC",
        "created_at" | "recent" => "ORDER BY created_at DESC",
        "key" => "ORDER BY key ASC",
        // S4: `outcome_reward` was written, read by `case_value`, and yet could
        // not be sorted by — so the one column that says which lessons actually
        // worked was unreachable from the listing that surfaces them.
        // NULLS LAST keeps unscored entries below scored ones without claiming
        // they failed.
        "reward" | "outcome_reward" => "ORDER BY outcome_reward DESC NULLS LAST",
        "importance" | "weight" => "ORDER BY importance DESC NULLS LAST",
        _ => "ORDER BY access_count DESC",
    }
}

#[cfg(test)]
mod memory_surface_tests {
    use super::{filter_outcomes, memory_list_order_clause};

    fn entry(key: &str) -> serde_json::Value {
        serde_json::json!({ "key": key, "value": "v" })
    }

    #[test]
    fn outcomes_are_dropped_by_default_lessons_survive() {
        let src = vec![
            entry("outcome:edit:transcript-e57e3c84:failure"),
            entry("loop-default-modus-operandi:2026-08-02"),
            entry("outcome:bash:unknown:plain:success"),
        ];
        let kept = filter_outcomes(src, false);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0]["key"], "loop-default-modus-operandi:2026-08-02");
    }

    #[test]
    fn include_outcomes_returns_everything_untouched() {
        let src = vec![entry("outcome:bash:x:failure"), entry("lesson:y")];
        assert_eq!(filter_outcomes(src.clone(), true).len(), src.len());
    }

    #[test]
    fn a_key_merely_containing_outcome_is_not_filtered() {
        // Only the NAMESPACE (prefix) marks auto-recorded noise; a curated lesson
        // that happens to discuss outcomes must never be silently dropped.
        let kept = filter_outcomes(vec![entry("lesson:bash-outcome-analysis")], false);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn entries_without_a_key_are_preserved() {
        let kept = filter_outcomes(vec![serde_json::json!({ "value": "no key" })], false);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn order_clauses_name_only_memory_entries_columns() {
        // `read_count` / `last_read_at` belong to the abandoned file_knowledge
        // encoding — naming them here would fail to prepare against memory.db.
        for sort in [
            "access_count",
            "last_accessed",
            "created_at",
            "key",
            "bogus",
        ] {
            let clause = memory_list_order_clause(sort);
            assert!(!clause.contains("read_count"), "{sort}: {clause}");
            assert!(!clause.contains("last_read_at"), "{sort}: {clause}");
            assert!(clause.starts_with("ORDER BY"), "{sort}: {clause}");
        }
        assert_eq!(memory_list_order_clause("key"), "ORDER BY key ASC");
        assert_eq!(
            memory_list_order_clause("nope"),
            "ORDER BY access_count DESC"
        );
    }
}

#[cfg(test)]
mod case_value_tests {
    use super::{annotate_case_value, case_value, rerank_by_case_value};

    fn keyed(key: &str) -> serde_json::Value {
        serde_json::json!({ "key": key, "value": "v" })
    }

    #[test]
    fn value_comes_from_the_key_verdict_that_was_always_there() {
        assert_eq!(case_value(&keyed("outcome:bash:x:success")), Some(1.0));
        assert_eq!(case_value(&keyed("outcome:bash:x:failure")), Some(0.0));
    }

    /// The distinction the whole design rests on: unobserved ≠ failed.
    #[test]
    fn a_curated_lesson_is_unobserved_not_zero() {
        assert_eq!(
            case_value(&keyed("lesson:tantivy-per-project")),
            None,
            "a lesson nobody scored must be None; 0.0 would mark it as failed \
             and bury every curated entry in the store"
        );
    }

    #[test]
    fn an_explicit_reward_outranks_the_key_suffix() {
        let entry = serde_json::json!({
            "key": "outcome:bash:x:failure",
            "outcome_reward": 0.9,
        });
        assert_eq!(
            case_value(&entry),
            Some(0.9),
            "a measured reward is more authoritative than a key convention"
        );
    }

    #[test]
    fn explicit_rewards_are_clamped_to_the_engine_band() {
        let entry = serde_json::json!({ "key": "k", "outcome_reward": 7.5 });
        assert_eq!(case_value(&entry), Some(1.0));
    }

    /// Proven-good first, unobserved next, proven-bad last.
    #[test]
    fn rerank_puts_proven_cases_where_they_belong() {
        let ranked = rerank_by_case_value(vec![
            keyed("outcome:bash:a:failure"),
            keyed("lesson:curated"),
            keyed("outcome:bash:b:success"),
        ]);
        let keys: Vec<&str> = ranked
            .iter()
            .filter_map(|e| e.get("key").and_then(|k| k.as_str()))
            .collect();
        assert_eq!(
            keys,
            vec![
                "outcome:bash:b:success",
                "lesson:curated",
                "outcome:bash:a:failure"
            ]
        );
    }

    /// Value picks the class; RRF similarity still orders WITHIN it.
    ///
    /// Without stability the rerank would throw away the similarity ranking it
    /// is supposed to refine — the fusion would become a replacement.
    #[test]
    fn rerank_is_stable_so_similarity_order_survives_inside_a_class() {
        let ranked = rerank_by_case_value(vec![
            keyed("lesson:most-similar"),
            keyed("lesson:less-similar"),
            keyed("lesson:least-similar"),
        ]);
        let keys: Vec<&str> = ranked
            .iter()
            .filter_map(|e| e.get("key").and_then(|k| k.as_str()))
            .collect();
        assert_eq!(
            keys,
            vec![
                "lesson:most-similar",
                "lesson:less-similar",
                "lesson:least-similar"
            ],
            "equal value must leave the RRF order untouched"
        );
    }

    /// Counter-proof: the rerank must actually MOVE things, or it proves nothing.
    #[test]
    fn rerank_changes_the_order_it_is_given() {
        let input = vec![
            keyed("outcome:bash:a:failure"),
            keyed("outcome:bash:b:success"),
        ];
        let before: Vec<String> = input
            .iter()
            .map(|e| e["key"].as_str().unwrap_or_default().to_string())
            .collect();
        let after: Vec<String> = rerank_by_case_value(input)
            .iter()
            .map(|e| e["key"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_ne!(
            before, after,
            "a rerank that never reorders would pass every other test here \
             while doing nothing"
        );
    }

    #[test]
    fn annotation_exposes_the_value_and_omits_the_unknown() {
        let out = annotate_case_value(vec![
            keyed("outcome:bash:x:success"),
            keyed("lesson:curated"),
        ]);
        assert_eq!(out[0].get("case_value").and_then(|v| v.as_f64()), Some(1.0));
        assert!(
            out[1].get("case_value").is_none(),
            "an unobserved case must not be serialised as a value at all"
        );
    }
}

#[cfg(test)]
mod partition_cases_tests {
    use super::{MAX_CASES_PER_CLASS, partition_cases};

    fn keyed(key: &str) -> serde_json::Value {
        serde_json::json!({ "key": key, "value": "v" })
    }

    fn keys_in(part: &serde_json::Value, class: &str) -> Vec<String> {
        part[class]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.get("key").and_then(|k| k.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn cases_are_split_into_labelled_classes() {
        let part = partition_cases(&[
            keyed("outcome:bash:a:failure"),
            keyed("lesson:curated"),
            keyed("outcome:bash:b:success"),
        ]);
        assert_eq!(keys_in(&part, "positive"), vec!["outcome:bash:b:success"]);
        assert_eq!(keys_in(&part, "negative"), vec!["outcome:bash:a:failure"]);
        assert_eq!(keys_in(&part, "unobserved"), vec!["lesson:curated"]);
    }

    /// The label must travel WITH the cases — a consumer that cannot tell a
    /// failure from a success will treat both as guidance, which is exactly why
    /// an unlabelled bank of failures had to be filtered out wholesale.
    #[test]
    fn the_partition_carries_its_own_usage_instruction() {
        let part = partition_cases(&[keyed("outcome:bash:a:failure")]);
        let guidance = part["guidance"].as_str().unwrap_or_default();
        assert!(
            guidance.contains("avoid"),
            "the negative class must arrive with instructions to avoid it, got: {guidance}"
        );
        assert!(
            guidance.contains("positive"),
            "the positive class must be named as the one to reuse"
        );
    }

    /// The whole reason for PER-CLASS caps: under one shared budget a 99:1
    /// imbalance mathematically guarantees the minority class gets nothing.
    #[test]
    fn a_flood_of_negatives_cannot_crowd_out_the_positives() {
        let mut flood: Vec<serde_json::Value> = (0..200)
            .map(|i| keyed(&format!("outcome:bash:{i}:failure")))
            .collect();
        flood.push(keyed("outcome:bash:winner:success"));

        let part = partition_cases(&flood);

        assert_eq!(
            keys_in(&part, "positive"),
            vec!["outcome:bash:winner:success"],
            "the single positive must survive 200 negatives — a shared top-K \
             would have returned 20 failures and zero successes"
        );
        assert_eq!(
            keys_in(&part, "negative").len(),
            MAX_CASES_PER_CLASS,
            "negatives are bounded, so they cannot swamp the result"
        );
    }

    /// Counter-proof for the cap itself: without it this test passes trivially.
    #[test]
    fn each_class_is_capped_independently() {
        let many: Vec<serde_json::Value> = (0..50)
            .flat_map(|i| {
                [
                    keyed(&format!("outcome:bash:{i}:success")),
                    keyed(&format!("outcome:bash:{i}:failure")),
                    keyed(&format!("lesson:{i}")),
                ]
            })
            .collect();
        let part = partition_cases(&many);
        for class in ["positive", "negative", "unobserved"] {
            assert_eq!(
                keys_in(&part, class).len(),
                MAX_CASES_PER_CLASS,
                "class {class} exceeded the per-class cap"
            );
        }
    }

    /// Order inside a class is the similarity order it arrived in.
    #[test]
    fn class_order_preserves_the_incoming_ranking() {
        let part = partition_cases(&[
            keyed("lesson:first"),
            keyed("lesson:second"),
            keyed("lesson:third"),
        ]);
        assert_eq!(
            keys_in(&part, "unobserved"),
            vec!["lesson:first", "lesson:second", "lesson:third"]
        );
    }

    #[test]
    fn an_empty_recall_yields_empty_classes_not_an_error() {
        let part = partition_cases(&[]);
        for class in ["positive", "negative", "unobserved"] {
            assert!(keys_in(&part, class).is_empty());
        }
    }
}

#[cfg(test)]
mod repair_case_tests {
    use super::{case_value, partition_cases, repair_from, shape_case};

    /// A mined case, in the exact shape `redacted_lesson_value` persists.
    fn mined(key: &str, error: &str, resolution: &str) -> serde_json::Value {
        let value = serde_json::json!({
            "tool": "Read",
            "error": error,
            "resolution_input": resolution,
            "session_id": "s",
            "timestamp": "t",
        })
        .to_string();
        serde_json::json!({ "key": key, "value": value })
    }

    /// The defect this closes: a `:failure` key holding a VALIDATED fix was
    /// scored 0.0 and filed under `negative`, discarding 3.448 of 3.478 cases.
    #[test]
    fn a_mined_repair_is_positive_despite_its_failure_key() {
        let entry = mined(
            "outcome:read:transcript-abc:failure",
            "File has not been read yet.",
            "{\"file_path\":\"/x\"}",
        );
        assert_eq!(
            case_value(&entry),
            Some(1.0),
            "the resolution was a ToolUse whose result was NOT an error — the \
             key suffix describes the trigger, not the verdict"
        );
    }

    /// Counter-proof: a `:failure` WITHOUT a resolution stays negative.
    /// Without this half the test above would pass by treating everything as
    /// positive.
    #[test]
    fn a_blocked_dry_run_without_a_resolution_stays_negative() {
        let entry = serde_json::json!({
            "key": "outcome:bash:ceg-rm-rf:failure",
            "value": "CEG dry-run blocked: destructive delete. Fix: scope the path",
        });
        assert_eq!(
            case_value(&entry),
            Some(0.0),
            "the CEG's blocked dry-runs carry no resolution and ARE the bank's \
             only genuine negatives"
        );
        assert!(repair_from(&entry).is_none());
    }

    #[test]
    fn an_explicit_reward_still_outranks_the_repair_heuristic() {
        let mut entry = mined("outcome:read:transcript-abc:failure", "e", "r");
        entry["outcome_reward"] = serde_json::json!(0.25);
        assert_eq!(case_value(&entry), Some(0.25));
    }

    /// The actionable half must become a named field, not stay inside a blob.
    #[test]
    fn shaping_surfaces_the_situation_and_the_action() {
        let shaped = shape_case(&mined(
            "outcome:read:transcript-abc:failure",
            "File content exceeds maximum allowed tokens",
            "{\"file_path\":\"/x\",\"offset\":100}",
        ));
        assert_eq!(
            shaped.get("when").and_then(|v| v.as_str()),
            Some("File content exceeds maximum allowed tokens")
        );
        assert!(
            shaped
                .get("do")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.contains("offset")),
            "the fix must be readable without parsing `value` by hand"
        );
        assert_eq!(
            shaped.get("case_kind").and_then(|v| v.as_str()),
            Some("repair")
        );
    }

    #[test]
    fn shaping_leaves_a_non_repair_untouched() {
        let plain = serde_json::json!({ "key": "lesson:x", "value": "prose" });
        assert_eq!(shape_case(&plain), plain);
    }

    /// Fail-open: a value that is not the expected JSON must not lose the entry.
    #[test]
    fn an_unparseable_value_falls_back_to_the_key_convention() {
        let entry = serde_json::json!({
            "key": "outcome:bash:transcript-x:failure",
            "value": "not json at all",
        });
        assert!(repair_from(&entry).is_none());
        assert_eq!(case_value(&entry), Some(0.0), "falls back, never panics");
    }

    /// End to end: mined repairs must land in `positive`, shaped.
    #[test]
    fn mined_repairs_reach_the_positive_class_shaped() {
        let part = partition_cases(&[
            mined("outcome:read:transcript-a:failure", "err A", "{\"fix\":1}"),
            serde_json::json!({
                "key": "outcome:bash:ceg-x:failure",
                "value": "CEG dry-run blocked: x",
            }),
        ]);
        let positive = part["positive"].as_array().cloned().unwrap_or_default();
        let negative = part["negative"].as_array().cloned().unwrap_or_default();

        assert_eq!(
            positive.len(),
            1,
            "the repair belongs to the positive class"
        );
        assert_eq!(
            positive[0].get("when").and_then(|v| v.as_str()),
            Some("err A"),
            "and arrives already shaped as situation -> action"
        );
        assert_eq!(negative.len(), 1, "the blocked dry-run stays negative");
    }
}

#[cfg(test)]
mod credit_key_tests {
    use super::credit_key;

    /// The credit arrives from a different process that re-states the query, so
    /// whitespace and case must not decide whether the loop closes.
    #[test]
    fn the_join_key_survives_reformatting_of_the_query() {
        assert_eq!(
            credit_key("  How   To  Page A Large FILE "),
            credit_key("how to page a large file")
        );
    }

    #[test]
    fn distinct_queries_keep_distinct_keys() {
        assert_ne!(credit_key("page a large file"), credit_key("page a file"));
    }

    #[test]
    fn an_empty_query_normalises_to_empty_rather_than_panicking() {
        assert_eq!(credit_key("   "), "");
    }
}

#[cfg(test)]
mod real_shape_regression_tests {
    use super::{case_value, json_field_as_text, partition_cases, repair_from, shape_case};

    /// A case in the shape the miner ACTUALLY writes.
    ///
    /// `error` is a string; `resolution_input` is an **object**. Verified against
    /// the live store on 04/08/2026 — every fixture in the earlier tests used a
    /// string for both, which is why they passed while 0 of 3.448 real entries
    /// classified.
    fn real_mined_case() -> serde_json::Value {
        let value = serde_json::json!({
            "tool": "Read",
            "error": "File content (56869 tokens) exceeds maximum allowed tokens (25000).",
            "resolution_input": { "file_path": "/x/big.rs", "offset": 100, "limit": 200 },
            "session_id": "s",
            "timestamp": "t",
        })
        .to_string();
        serde_json::json!({ "key": "outcome:read:transcript-real:failure", "value": value })
    }

    /// The regression: the object-valued resolution must be recognised.
    #[test]
    fn the_shape_the_miner_really_writes_is_recognised_as_a_repair() {
        let entry = real_mined_case();
        let (when, then) = repair_from(&entry).expect(
            "resolution_input is stored as an OBJECT, not a string — requiring \
             as_str() matched 0 of 3.448 live entries",
        );
        assert!(when.contains("exceeds maximum allowed tokens"));
        assert!(
            then.contains("offset"),
            "the object must be rendered, not dropped; got {then}"
        );
        assert_eq!(case_value(&entry), Some(1.0));
    }

    #[test]
    fn the_object_resolution_reaches_the_positive_class_shaped() {
        let part = partition_cases(&[real_mined_case()]);
        let positive = part["positive"].as_array().cloned().unwrap_or_default();
        assert_eq!(
            positive.len(),
            1,
            "a real mined repair belongs to `positive`"
        );
        assert!(
            positive[0]["do"]
                .as_str()
                .is_some_and(|s| s.contains("file_path")),
            "the actionable half must be readable: {:?}",
            positive[0]["do"]
        );
    }

    /// The string form must keep working — the fix widens, never swaps.
    #[test]
    fn the_string_form_still_classifies() {
        let value = serde_json::json!({
            "error": "e", "resolution_input": "{\"cmd\":\"ls\"}",
        })
        .to_string();
        let entry =
            serde_json::json!({ "key": "outcome:bash:transcript-s:failure", "value": value });
        assert!(repair_from(&entry).is_some());
    }

    /// Counter-proof: an entry with no usable action is NOT a repair, so a
    /// blocked dry-run stays negative and prose stays unobserved.
    #[test]
    fn empty_and_absent_actions_are_not_repairs() {
        for value in [
            serde_json::json!({ "error": "e", "resolution_input": "" }),
            serde_json::json!({ "error": "e", "resolution_input": {} }),
            serde_json::json!({ "error": "e" }),
            serde_json::json!({ "error": "", "resolution_input": { "a": 1 } }),
            serde_json::json!({ "error": "e", "resolution_input": serde_json::Value::Null }),
        ] {
            let entry = serde_json::json!({
                "key": "outcome:bash:transcript-x:failure",
                "value": value.to_string(),
            });
            assert!(
                repair_from(&entry).is_none(),
                "no action means no repair: {value}"
            );
            assert_eq!(
                case_value(&entry),
                Some(0.0),
                "falls back to the key verdict"
            );
        }
    }

    /// Prose must never be mistaken for a case — 3 curated lessons in the live
    /// store contain the literal text `resolution_input` in their prose.
    #[test]
    fn prose_mentioning_the_field_name_is_not_a_repair() {
        let entry = serde_json::json!({
            "key": "lesson:banco-de-casos",
            "value": "O miner grava error e resolution_input no mesmo registro.",
        });
        assert!(repair_from(&entry).is_none());
        assert_eq!(
            case_value(&entry),
            None,
            "a curated lesson stays unobserved"
        );
        assert_eq!(shape_case(&entry), entry, "and is passed through untouched");
    }

    #[test]
    fn field_reader_handles_every_json_shape() {
        assert_eq!(
            json_field_as_text(Some(&serde_json::json!("text"))),
            Some("text".into())
        );
        assert_eq!(
            json_field_as_text(Some(&serde_json::json!({"a":1}))),
            Some("{\"a\":1}".into())
        );
        assert_eq!(
            json_field_as_text(Some(&serde_json::json!([1, 2]))),
            Some("[1,2]".into())
        );
        assert_eq!(json_field_as_text(Some(&serde_json::json!(""))), None);
        assert_eq!(json_field_as_text(Some(&serde_json::json!({}))), None);
        assert_eq!(json_field_as_text(Some(&serde_json::Value::Null)), None);
        assert_eq!(json_field_as_text(None), None);
    }
}

/// S4 (2026-08-07) — the pheromone must evaporate.
///
/// Covers the three mechanisms added to the memory layer: weight
/// (`importance`), pinning, and supersession. Each test states the failure it
/// prevents, because the value of these columns is entirely in what they stop
/// from surfacing.
#[cfg(test)]
mod pheromone_decay_tests {
    use super::memory_list_order_clause;
    use crate::cli::shared::{memory_column_present, optional_column_select, superseded_filter};

    /// A memory DB carrying the S4 columns, built the way the store builds it.
    fn db_with_s4_columns() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE memory_entries (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                tier TEXT NOT NULL DEFAULT 'local',
                entry_type TEXT NOT NULL DEFAULT 'insight',
                access_count INTEGER NOT NULL DEFAULT 0,
                outcome_reward REAL,
                importance INTEGER,
                pinned INTEGER NOT NULL DEFAULT 0,
                superseded_by TEXT
            );",
        )
        .expect("schema");
        conn
    }

    /// A pre-S4 DB — the federated case: another project's store that never ran
    /// the migration.
    fn legacy_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE memory_entries (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                tier TEXT NOT NULL DEFAULT 'local',
                entry_type TEXT NOT NULL DEFAULT 'insight',
                access_count INTEGER NOT NULL DEFAULT 0
            );",
        )
        .expect("schema");
        conn
    }

    #[test]
    fn reward_and_importance_became_sortable() {
        // The regression this closes: `outcome_reward` was written and read by
        // `case_value`, yet `--sort reward` silently fell through to
        // access_count — the column that says which lessons worked could not
        // order the listing that surfaces them.
        assert_eq!(
            memory_list_order_clause("reward"),
            "ORDER BY outcome_reward DESC NULLS LAST"
        );
        assert_eq!(
            memory_list_order_clause("importance"),
            "ORDER BY importance DESC NULLS LAST"
        );
        assert_eq!(
            memory_list_order_clause("whatever"),
            "ORDER BY access_count DESC",
            "unknown sorts keep the historical default"
        );
    }

    #[test]
    fn nulls_last_keeps_unscored_entries_from_reading_as_failures() {
        // An unscored entry must rank below a scored one WITHOUT being ordered
        // as if it had scored badly — the distinction the whole NULL discipline
        // rests on.
        let conn = db_with_s4_columns();
        conn.execute_batch(
            "INSERT INTO memory_entries (key, value, outcome_reward) VALUES ('scored-low', 'v', -0.9);
             INSERT INTO memory_entries (key, value, outcome_reward) VALUES ('scored-high', 'v', 0.9);
             INSERT INTO memory_entries (key, value) VALUES ('unscored', 'v');",
        )
        .expect("seed");
        let order = memory_list_order_clause("reward");
        let keys: Vec<String> = conn
            .prepare(&format!("SELECT key FROM memory_entries {order}"))
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(0))
                    .map(|rows| rows.filter_map(Result::ok).collect())
            })
            .expect("query");
        assert_eq!(keys, vec!["scored-high", "scored-low", "unscored"]);
    }

    #[test]
    fn superseded_entries_stop_surfacing_but_stay_for_audit() {
        let conn = db_with_s4_columns();
        conn.execute_batch(
            "INSERT INTO memory_entries (key, value) VALUES ('lesson:v1', 'the wrong advice');
             INSERT INTO memory_entries (key, value) VALUES ('lesson:v2', 'the correction');
             UPDATE memory_entries SET superseded_by = 'lesson:v2' WHERE key = 'lesson:v1';",
        )
        .expect("seed");

        let filter = superseded_filter(&conn, "");
        assert!(!filter.is_empty(), "column present ⇒ filter applies");
        let visible: Vec<String> = conn
            .prepare(&format!(
                "SELECT key FROM memory_entries WHERE (value LIKE '%advice%' OR value LIKE '%correction%'){filter} ORDER BY key"
            ))
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(0))
                    .map(|rows| rows.filter_map(Result::ok).collect())
            })
            .expect("query");
        assert_eq!(visible, vec!["lesson:v2"], "the retired lesson is hidden");

        let still_stored: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_entries WHERE key = 'lesson:v1'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(still_stored, 1, "retirement is not deletion");
    }

    #[test]
    fn a_legacy_db_without_the_columns_still_recalls() {
        // The failure mode this prevents is the worst kind: a federated recall
        // that returns EMPTY because one project's DB lacks a column, with no
        // error anywhere. Absent column ⇒ NULL projection, no filter, no crash.
        let conn = legacy_db();
        assert!(!memory_column_present(&conn, "importance"));
        assert_eq!(optional_column_select(&conn, "", "importance"), "NULL");
        assert_eq!(optional_column_select(&conn, "e.", "pinned"), "NULL");
        assert_eq!(superseded_filter(&conn, ""), "");

        conn.execute(
            "INSERT INTO memory_entries (key, value) VALUES ('k', 'v')",
            [],
        )
        .expect("seed");
        let importance_col = optional_column_select(&conn, "", "importance");
        let filter = superseded_filter(&conn, "");
        let sql =
            format!("SELECT key, {importance_col} FROM memory_entries WHERE key = 'k'{filter}");
        let (key, importance): (String, Option<i64>) = conn
            .query_row(&sql, [], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("legacy query must still prepare and run");
        assert_eq!(key, "k");
        assert_eq!(importance, None);
    }

    #[test]
    fn present_columns_are_projected_not_nulled() {
        let conn = db_with_s4_columns();
        assert_eq!(optional_column_select(&conn, "", "importance"), "importance");
        assert_eq!(optional_column_select(&conn, "e.", "pinned"), "e.pinned");
        assert_eq!(
            superseded_filter(&conn, "e."),
            " AND e.superseded_by IS NULL"
        );
    }

    #[test]
    fn pinned_then_importance_then_relevance() {
        // Ordering contract of the recall path, asserted on the same SQL shape
        // the recall queries build.
        let conn = db_with_s4_columns();
        conn.execute_batch(
            "INSERT INTO memory_entries (key, value, importance, pinned) VALUES ('c-plain', 'v', NULL, 0);
             INSERT INTO memory_entries (key, value, importance, pinned) VALUES ('b-weighted', 'v', 4, 0);
             INSERT INTO memory_entries (key, value, importance, pinned) VALUES ('a-pinned', 'v', 1, 1);",
        )
        .expect("seed");
        let keys: Vec<String> = conn
            .prepare(
                "SELECT key FROM memory_entries
                 ORDER BY COALESCE(pinned, 0) DESC, COALESCE(importance, 0) DESC, key",
            )
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(0))
                    .map(|rows| rows.filter_map(Result::ok).collect())
            })
            .expect("query");
        assert_eq!(
            keys,
            vec!["a-pinned", "b-weighted", "c-plain"],
            "pin beats weight; weight beats an unweighted entry"
        );
    }
}
