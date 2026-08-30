//! CEG IoC implementations — `LearnRuntime` / `CegRuntime` for [`HookRuntime`]
//! plus the three runtime-service handlers they delegate to.
//!
//! Carve R (2026-06-10): these inherent trait impls moved here from the cli
//! dispatch hub because Rust's orphan rule requires `impl Trait for Type` to
//! live in the crate that owns the trait or the type — `HookRuntime` now
//! lives in this crate. The three handlers (`cli_learning_reward`,
//! `cli_gotcha_add`, `cli_memory_store`) are pure runtime capabilities
//! (RL reward, gotcha DB, memory store) with zero cli/ coupling; the cli
//! modules re-export them at their historical paths.

use crate::embeddings::semantic_or_hash_embedding;
use crate::hook_runtime::HookRuntime;

/// Injects an immediate RL reward for a tool into the runtime Q-table, clamping the value to `[-1.0, 1.0]`.
pub fn cli_learning_reward(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let tool = payload
        .get("tool_name")
        .or_else(|| payload.get("tool"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let value = payload
        .get("reward")
        .or_else(|| payload.get("value"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let context = payload
        .get("context")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if tool.is_empty() {
        return serde_json::json!({ "error" : "tool name is required" }).to_string();
    }
    let clamped = value.clamp(-1.0, 1.0);
    let reward = touring_intelligence::rl::ImmediateReward {
        tool_name: tool.to_string(),
        accepted: clamped > 0.0,
        latency_ms: 0,
        error_count: if clamped < 0.0 { 1 } else { 0 },
        cila_level: 2,
        file_type: 3,
        quality_score: Some(clamped),
    };
    let mut qtable = rt.learning.qtable_cache.take().unwrap_or_default();
    rt.process_immediate_reward(&reward, &mut qtable);
    rt.learning.qtable_cache = Some(qtable);
    // W3 d2/S-3.1 — a reward for a snippet memory ALSO feeds its measured
    // trust ladder (`snippet_stats`): success = clamped > 0; `sig_hash` (the
    // dependency-surface digest) rides in the payload when the caller has one.
    // Fail-open: a stats failure never blocks the reward path.
    let snippet_trust = if touring_intelligence::rl::memory::snippet_stats::is_snippet_key(tool) {
        use touring_intelligence::rl::memory::snippet_stats;
        let sig = payload
            .get("sig_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        open_tagged_memory_db(rt)
            .ok()
            .and_then(|conn| snippet_stats::record_execution(&conn, tool, clamped > 0.0, sig).ok())
            .map(|t| t.as_str())
    } else {
        None
    };
    let mut out = serde_json::json!(
        { "tool" : tool, "value" : clamped, "context" : context, "status" :
        "reward_injected" }
    );
    if let Some(t) = snippet_trust {
        out["snippet_trust"] = serde_json::json!(t);
    }
    out.to_string()
}

/// Adds a pitfall pattern with description and severity to the knowledge gotcha database.
pub fn cli_gotcha_add(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let pattern = payload
        .get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let description = payload
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let severity = payload
        .get("severity")
        .and_then(|v| v.as_str())
        .unwrap_or("medium");
    if pattern.is_empty() || description.is_empty() {
        return serde_json::json!({ "error" : "pattern and description are required" }).to_string();
    }
    match rt
        .ctx
        .knowledge
        .add_gotcha(pattern, description, severity, None)
    {
        Ok(id) => serde_json::json!(
            { "id" : id, "pattern" : pattern, "description" : description, "severity"
            : severity, "status" : "added" }
        )
        .to_string(),
        Err(e) => serde_json::json!({ "error" : format!("failed to add gotcha: {e}") }).to_string(),
    }
}

/// Parsed fields of a `cli_memory_store` payload (H1 extraction, 2026-08-12 —
/// keeps the handler under the CC budget and the mapping auditable in one place).
struct MemoryStorePayload<'a> {
    key: &'a str,
    value: &'a str,
    tier: &'a str,
    entry_type: &'a str,
    file_path: Option<&'a str>,
    outcome_reward: Option<f64>,
    outcome_context: Option<&'a str>,
    importance: Option<i64>,
    pinned: bool,
    supersedes: Option<&'a str>,
    explicit_tags: Vec<String>,
}

/// Parses and validates the store payload. `Err` is the user-facing message.
fn parse_memory_store_payload(payload: &serde_json::Value) -> Result<MemoryStorePayload<'_>, String> {
    let key = payload.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let value = payload.get("value").and_then(|v| v.as_str()).unwrap_or("");
    if key.is_empty() || value.is_empty() {
        return Err("key and value are required".to_string());
    }
    // CLI (`touring memory store`) sends "entry_type"; in-process callers
    // (post_tool_rl) send "type". Accept both so the ANN-indexed store path is
    // reached regardless of caller. (S-04 2026-05-29.)
    let entry_type = payload
        .get("type")
        .or_else(|| payload.get("entry_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("insight");
    // The measured outcome of this case, if the caller knows it. Absent stays
    // NULL: an unobserved case is not a failed one, and recording 0.0 for
    // "unknown" would teach a value-ranked recall that unmeasured means bad.
    let outcome_reward = payload
        .get("reward")
        .or_else(|| payload.get("outcome_reward"))
        .and_then(serde_json::Value::as_f64)
        .map(|r| r.clamp(-1.0, 1.0));
    // S4 (2026-08-07): weight, pinning and supersession — the three mechanisms
    // that let the ACO pheromone evaporate. All three stay NULL/absent unless
    // the caller sets them: an unweighted entry has not been judged, and
    // inventing a default importance would be exactly the "approximation that
    // erases the signal" this work removes.
    let importance = payload
        .get("importance")
        .and_then(serde_json::Value::as_i64)
        .map(|i| i.clamp(1, 5));
    let explicit_tags = payload
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(MemoryStorePayload {
        key,
        value,
        tier: payload.get("tier").and_then(|v| v.as_str()).unwrap_or("local"),
        entry_type,
        file_path: payload.get("file_path").and_then(|v| v.as_str()),
        outcome_reward,
        outcome_context: payload
            .get("outcome_context")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty()),
        importance,
        pinned: payload
            .get("pinned")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        supersedes: payload
            .get("supersedes")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty()),
        explicit_tags,
    })
}

/// Persists a key/value memory entry into the SQLite store and mirrors it into the ANN recall corpus.
pub fn cli_memory_store(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let parsed = match parse_memory_store_payload(payload) {
        Ok(p) => p,
        Err(msg) => return serde_json::json!({ "error": msg }).to_string(),
    };
    let memory_db_path = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    if let Some(parent) = memory_db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    // H1 (2026-08-12): the RPC surface delegates to the unified write path.
    // `RlmMemory::new` runs the canonical schema + additive migrations (incl.
    // the composite-PK rebuild) and `store_rich` holds the single conflict
    // semantics — the inline CREATE/ALTER/INSERT that used to live here moved
    // into `touring_intelligence::rl::memory::rlm` (single source of truth;
    // gotcha:ceg-impls-dual-memory-schema closed by construction).
    let key = parsed.key;
    let value = parsed.value;
    let tier = parsed.tier;
    let entry_type = parsed.entry_type;
    let result = touring_intelligence::rl::memory::rlm::RlmMemory::new(&memory_db_path)
        .and_then(|mem| {
            let mut entry = touring_intelligence::rl::memory::rlm::RichMemoryEntry::new(
                parsed.key,
                parsed.tier,
                parsed.value,
            );
            entry.entry_type = Some(parsed.entry_type);
            entry.file_path = parsed.file_path;
            entry.outcome_reward = parsed.outcome_reward;
            entry.outcome_context = parsed.outcome_context;
            entry.importance = parsed.importance;
            entry.pinned = parsed.pinned;
            entry.supersedes = parsed.supersedes;
            entry.explicit_tags = &parsed.explicit_tags;
            mem.store_rich(&entry)
        })
        .map_err(|e| e.to_string());
    match result {
        Ok(ignored_tags) => {
            // S-04 (2026-05-29): mirror the stored entry into the ANN corpus so
            // `memory recall` returns ANN hits (RRF no longer degrades to FTS/
            // TF-IDF only). Embedding = 64-dim deterministic hash vector (no GPU
            // / model). `ann_recall` is initialized at daemon startup
            // (init_ann_memory, daemon.rs:1446). Fail-open: the SQL store already
            // succeeded; an ANN failure only sets ann_indexed=false.
            let ann_indexed = {
                let mut borrow = rt.ctx.ann_recall.borrow_mut();
                if let Some(ann) = borrow.as_mut() {
                    let entry = crate::ann_memory::MemoryEntry::new(
                        key,
                        value,
                        semantic_or_hash_embedding(value),
                    );
                    match ann.add_memory(entry) {
                        Ok(()) => true,
                        Err(e) => {
                            tracing::warn!("ANN upsert failed (SQL store ok): {e}");
                            false
                        }
                    }
                } else {
                    false
                }
            };
            // Contract P1 (2026-08-30): `ignored_facets` is ALWAYS present —
            // an empty array is "all tags accepted", a missing key would be
            // ambiguous with an old daemon ("absence has two causes"). Each
            // element teaches the fix: reason names the 7 canonical facets,
            // `suggestion` the closest one.
            //
            // Contract P3: for the curated kinds (semantic lesson/decision/
            // diagnostico) the response also carries the ADVISORY contract
            // scorecard — links are usually created in the same batch right
            // AFTER the store, so `linked`/`provenance` false here is the
            // nudge to do it now, never a veto (the enforcement ladder starts
            // at advisory + KPI; a gate born blocking becomes a routed-around
            // gate).
            // Cross-audit 2026-08-30 (F-1): curated-kind detection reads the
            // FACET vocabulary too — a store carrying `#kind:decision` with a
            // default entry_type is exactly as curated as `--type decision`
            // (the contract governs by facet; entry_type is the legacy field).
            let curated_kind = matches!(entry_type, "lesson" | "decision" | "diagnostico")
                || parsed.explicit_tags.iter().any(|t| {
                    matches!(
                        t.trim().trim_start_matches('#'),
                        "kind:lesson" | "kind:decision" | "kind:diagnostico"
                    )
                });
            let contract = (tier == "semantic" && curated_kind).then(|| {
                use touring_intelligence::rl::memory::tags;
                let (linked, provenance) = rusqlite::Connection::open(&memory_db_path)
                    .ok()
                    .and_then(|conn| tags::fetch_links(&conn, key).ok())
                    .map(|links| {
                        let prov = links.iter().any(|l| l.rel == tags::LinkRel::GeneratedBy);
                        (!links.is_empty(), prov)
                    })
                    .unwrap_or((false, false));
                serde_json::json!({
                    "key_shape": tags::key_shape_ok(key),
                    "faceted": !parsed.explicit_tags.is_empty()
                        && ignored_tags.len() < parsed.explicit_tags.len(),
                    "linked": linked,
                    "provenance": provenance,
                })
            });
            serde_json::json!(
                { "key" : key, "tier" : tier, "type" : entry_type, "status" : "stored", "ann_indexed" : ann_indexed,
                  "ignored_facets" : ignored_tags.iter().map(|t| t.to_json()).collect::<Vec<_>>(),
                  "contract" : contract }
            )
                .to_string()
        }
        Err(e) => {
            serde_json::json!({ "error" : format!("failed to store memory: {}", e) }).to_string()
        }
    }
}

// =============================================================================
// Hashtag library (F1, 2026-08-11) — faceted memory tagging on the RPC path.
//
// Schema + tag SQL live in `touring_intelligence::rl::memory::tags`; these
// handlers are thin RPC adapters over them, so the in-process `RlmMemory`
// path and this CLI path can never drift into divergent tag semantics.
// =============================================================================


/// Opens the canonical memory.db and guarantees the tag schema exists.
fn open_tagged_memory_db(rt: &HookRuntime) -> rusqlite::Result<rusqlite::Connection> {
    let path = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = rusqlite::Connection::open(path)?;
    touring_intelligence::rl::memory::tags::ensure_tag_schema(&conn)?;
    Ok(conn)
}

/// Attaches one `#facet:value` tag to a memory entry (`touring memory tag add`).
pub fn cli_memory_tag_add(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::memory::tags;
    let key = payload.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let raw_tag = payload.get("tag").and_then(|v| v.as_str()).unwrap_or("");
    if key.is_empty() || raw_tag.is_empty() {
        return serde_json::json!({ "error": "key and tag are required" }).to_string();
    }
    let (parsed, warns) = tags::validate_tag(raw_tag);
    let tag = match parsed {
        Ok(t) => t,
        Err(errs) => {
            return serde_json::json!({ "error": format!("invalid tag {raw_tag}: {errs:?}") })
                .to_string();
        }
    };
    match open_tagged_memory_db(rt).map(|conn| tags::upsert_tag(&conn, key, &tag, tags::TagSource::Explicit)) {
        Ok(Ok(())) => serde_json::json!({
            "status": "tagged",
            "key": key,
            "tag": tag.full_tag,
            "warnings": warns.len(),
        })
        .to_string(),
        Ok(Err(e)) | Err(e) => {
            serde_json::json!({ "error": format!("failed to tag memory: {e}") }).to_string()
        }
    }
}

/// Lists the tags of one memory entry (`touring memory tags <key>`).
pub fn cli_memory_tags(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::memory::tags;
    let key = payload.get("key").and_then(|v| v.as_str()).unwrap_or("");
    if key.is_empty() {
        return serde_json::json!({ "error": "key is required" }).to_string();
    }
    match open_tagged_memory_db(rt).map(|conn| tags::fetch_tags(&conn, key)) {
        Ok(Ok(tags)) => serde_json::json!({
            "key": key,
            "count": tags.len(),
            "tags": tags.iter().map(|t| t.full_tag.clone()).collect::<Vec<_>>(),
        })
        .to_string(),
        Ok(Err(e)) | Err(e) => {
            serde_json::json!({ "error": format!("failed to list tags: {e}") }).to_string()
        }
    }
}

/// Conjunctive facet search (`touring memory query "texto #kind:snippet"`).
/// `#facet:value` tokens become exact filters over `memory_tags`; the
/// remaining text runs through `memories_fts` BM25, and the intersection is
/// returned when both are present (embedding ANN fuses in F3).
pub fn cli_memory_query(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::memory::tags;
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(10) as usize;
    if query.trim().is_empty() {
        return serde_json::json!({ "error": "query is required" }).to_string();
    }
    // Contract P1 (2026-08-30): a `#facet:value` token whose facet is not
    // canonical silently became TEXT — and matched by textual accident
    // whenever the value appeared in a body (measured: `#kind:lesson
    // #classe:x` returned the right entry for the wrong reason). The
    // reporting split surfaces those tokens so the response can say so.
    let (required, text, unknown) = tags::split_query_tags_reporting(query);
    let result = open_tagged_memory_db(rt).and_then(|conn| {
        let tag_keys = if required.is_empty() {
            None
        } else {
            Some(tags::entry_keys_with_all_tags(
                &conn,
                &required,
                // The allowed corpus must be (near-)complete: capping it at the
                // OUTPUT limit truncated the filter to the first N keys and the
                // later FTS intersection came out empty for anything older
                // (cross-audit F1 E2E regression, 2026-08-12).
                100_000,
            )?)
        };
        if text.is_empty() {
            fetch_entries_by_keys(&conn, tag_keys.as_deref().unwrap_or(&[]), limit)
        } else {
            fts_search_entries(&conn, &text, tag_keys.as_deref(), limit)
        }
    });
    match result {
        // W0 S-0.3 — honest pagination: a count without the universe is
        // unreadable as a universe (the 2026-08-24 retraction: a default
        // limit of 10 read as "only 10 exist"). Every paginated response
        // names shown/total/truncated; `count` stays for compatibility.
        Ok((hits, total)) => serde_json::json!({
            "query": query,
            "tags": required.iter().map(|t| t.full_tag.clone()).collect::<Vec<_>>(),
            "text": text,
            "count": hits.len(),
            "shown": hits.len(),
            "total": total,
            "truncated": total > hits.len(),
            // P1: always present — empty means "every #token filtered";
            // entries here fell back to TEXT search (a hit can be textual
            // accident, and a total of 0 here is not "the facet is empty").
            "unknown_facets": unknown.iter().map(|t| t.to_json()).collect::<Vec<_>>(),
            "results": hits,
        })
        .to_string(),
        Err(e) => serde_json::json!({ "error": format!("memory query failed: {e}") }).to_string(),
    }
}

/// BM25 half of the query: `memories_fts` MATCH, restricted to the
/// tag-filtered corpus when one was computed. FTS tokens are double-quoted so
/// user input cannot inject FTS5 operators.
fn fts_search_entries(
    conn: &rusqlite::Connection,
    text: &str,
    tag_keys: Option<&[String]>,
    limit: usize,
) -> rusqlite::Result<(Vec<serde_json::Value>, usize)> {
    let fts_query = text
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ");
    let mut stmt = conn.prepare(
        "SELECT e.key, e.value, e.entry_type, e.tier
         FROM memories_fts f
         JOIN memory_entries e ON e.key = f.key
         WHERE memories_fts MATCH ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![fts_query], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    // W0 S-0.3: the whole match set is walked so `total` names the real
    // universe — only the DELIVERED slice stops at `limit`.
    let mut total = 0usize;
    for row in rows {
        let (key, value, entry_type, tier) = row?;
        if let Some(keys) = tag_keys
            && !keys.contains(&key)
        {
            continue;
        }
        total += 1;
        if out.len() < limit {
            out.push(serde_json::json!({
                "key": key, "value": value, "entry_type": entry_type, "tier": tier,
            }));
        }
    }
    Ok((out, total))
}

/// Tag-only half of the query: fetch the entries behind the filtered keys.
fn fetch_entries_by_keys(
    conn: &rusqlite::Connection,
    keys: &[String],
    limit: usize,
) -> rusqlite::Result<(Vec<serde_json::Value>, usize)> {
    let mut out = Vec::new();
    for key in keys.iter().take(limit) {
        let row = conn.query_row(
            "SELECT value, entry_type, tier FROM memory_entries WHERE key = ?1",
            rusqlite::params![key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        );
        if let Ok((value, entry_type, tier)) = row {
            out.push(serde_json::json!({
                "key": key, "value": value, "entry_type": entry_type, "tier": tier,
            }));
        }
    }
    // W0 S-0.3: the tag-filtered key set IS the universe of this listing.
    Ok((out, keys.len()))
}

/// Creates a typed memory↔memory edge (`touring memory link <src> <dst>
/// --rel extends`). Ids are deterministic (`{src}|{rel}|{dst}`, REGRA #17):
/// re-asserting the same relation is a no-op, never a duplicate row.
pub fn cli_memory_link(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::memory::tags;
    let src = payload.get("src").and_then(|v| v.as_str()).unwrap_or("");
    let dst = payload.get("dst").and_then(|v| v.as_str()).unwrap_or("");
    let rel_raw = payload.get("rel").and_then(|v| v.as_str()).unwrap_or("");
    if src.is_empty() || dst.is_empty() || rel_raw.is_empty() {
        return serde_json::json!({ "error": "src, dst and --rel are required" }).to_string();
    }
    let Some(rel) = tags::LinkRel::from_str_ci(rel_raw) else {
        let valid = tags::LinkRel::ALL
            .iter()
            .map(|r| r.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return serde_json::json!({ "error": format!("unknown rel {rel_raw}; valid: {valid}") })
            .to_string();
    };
    let result = open_tagged_memory_db(rt)
        .and_then(|conn| tags::upsert_link(&conn, src, rel, dst));
    match result {
        Ok(edge) => serde_json::json!({
            "status": "linked", "id": edge.id, "src": edge.src, "dst": edge.dst,
            "rel": edge.rel.as_str(),
        })
        .to_string(),
        Err(e) => serde_json::json!({ "error": format!("link failed: {e}") }).to_string(),
    }
}

/// Lists the 1-hop link neighbourhood of a memory (`touring memory links <key>`).
pub fn cli_memory_links(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::memory::tags;
    // P5 (graph contract, 2026-08-30): `suggest: true` returns DERIVED edge
    // candidates from durable co-service — pairs the recall keeps serving
    // together that no typed edge connects yet. Suggestion only, never an
    // automatic write (the enforcement ladder measures before it acts);
    // every row carries the exact apply command (the nudge delivers the
    // program, never an exhortation).
    if payload.get("suggest").and_then(serde_json::Value::as_bool) == Some(true) {
        let min_co = payload
            .get("min_co")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(3) as i64;
        let limit = payload
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(20) as i64;
        let result = open_tagged_memory_db(rt).and_then(|conn| {
            let mut stmt = conn.prepare(
                "SELECT c.a, c.b, c.count FROM memory_coserved c
                 WHERE c.count >= ?1
                   AND NOT EXISTS (SELECT 1 FROM memory_links l
                                   WHERE (l.src = c.a AND l.dst = c.b)
                                      OR (l.src = c.b AND l.dst = c.a))
                 ORDER BY c.count DESC, c.pair LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![min_co, limit], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        });
        return match result {
            Ok(pairs) => serde_json::json!({
                "min_co": min_co,
                "count": pairs.len(),
                "suggestions": pairs.iter().map(|(a, b, n)| serde_json::json!({
                    "a": a, "b": b, "co_served": n,
                    "rel": "relates-to", "derived": true,
                    "apply": format!("touring memory link {a} {b} --rel relates-to"),
                })).collect::<Vec<_>>(),
            })
            .to_string(),
            Err(e) if e.to_string().contains("no such table") => serde_json::json!({
                "min_co": min_co, "count": 0, "suggestions": [],
                "note": "no co-service recorded yet — memory_coserved is \
                         written by every recall from this build on",
            })
            .to_string(),
            Err(e) => {
                serde_json::json!({ "error": format!("suggest-links failed: {e}") }).to_string()
            }
        };
    }
    let key = payload.get("key").and_then(|v| v.as_str()).unwrap_or("");
    if key.is_empty() {
        return serde_json::json!({ "error": "key is required" }).to_string();
    }
    let result = open_tagged_memory_db(rt).and_then(|conn| tags::fetch_links(&conn, key));
    match result {
        Ok(edges) => serde_json::json!({
            "key": key,
            "count": edges.len(),
            "links": edges.iter().map(|e| serde_json::json!({
                "id": e.id, "rel": e.rel.as_str(), "src": e.src, "dst": e.dst,
                "direction": if e.src == key { "out" } else { "in" },
            })).collect::<Vec<_>>(),
        })
        .to_string(),
        Err(e) => serde_json::json!({ "error": format!("links failed: {e}") }).to_string(),
    }
}

/// Lists the emergent communities of the whole store (`touring memory
/// communities [--domain <d>]`): label propagation over strong tags + typed
/// links. The read side of F5 — the MOC is the per-topic render; this is the
/// global structural view.
pub fn cli_memory_communities(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::memory::moc;
    let domain = payload.get("domain").and_then(|v| v.as_str());
    let result =
        open_tagged_memory_db(rt).and_then(|conn| moc::detect_communities(&conn, domain));
    match result {
        Ok(communities) => serde_json::json!({
            "domain": domain,
            "count": communities.len(),
            "communities": communities.iter().map(|c| serde_json::json!({
                "label": c.label, "size": c.members.len(), "members": c.members,
            })).collect::<Vec<_>>(),
        })
        .to_string(),
        Err(e) => serde_json::json!({ "error": format!("communities failed: {e}") }).to_string(),
    }
}

/// Removes one typed link by its deterministic id (`touring memory unlink
/// "<src>|<rel>|<dst>"`) — curation of the link graph.
pub fn cli_memory_unlink(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::memory::tags;
    let id = payload.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if id.is_empty() {
        return serde_json::json!({ "error": "id is required" }).to_string();
    }
    let result = open_tagged_memory_db(rt).and_then(|conn| tags::delete_link(&conn, id));
    match result {
        Ok(0) => serde_json::json!({ "status": "not_found", "id": id }).to_string(),
        Ok(_) => serde_json::json!({ "status": "unlinked", "id": id }).to_string(),
        Err(e) => serde_json::json!({ "error": format!("unlink failed: {e}") }).to_string(),
    }
}

/// Generates a Map of Content for a topic (`touring memory moc <topic>`):
/// emergent communities over the topic's corpus rendered as OKF markdown with
/// `[[key]]` links. Default stdout; `--out <path>` writes the document.
pub fn cli_memory_moc(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::memory::moc;
    let topic = payload.get("topic").and_then(|v| v.as_str()).unwrap_or("");
    if topic.is_empty() {
        return serde_json::json!({ "error": "topic is required" }).to_string();
    }
    let out_path = payload.get("out").and_then(|v| v.as_str());
    let result = open_tagged_memory_db(rt)
        .and_then(|conn| moc::render_moc(&conn, topic));
    match result {
        Ok(markdown) => {
            if let Some(path) = out_path {
                if let Some(parent) = std::path::Path::new(path).parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                match std::fs::write(path, &markdown) {
                    Ok(()) => serde_json::json!({
                        "status": "written", "path": path, "bytes": markdown.len(),
                    })
                    .to_string(),
                    Err(e) => serde_json::json!({ "error": format!("write {path}: {e}") })
                        .to_string(),
                }
            } else {
                serde_json::json!({
                    "status": "ok", "topic": topic, "markdown": markdown,
                })
                .to_string()
            }
        }
        Err(e) => serde_json::json!({ "error": format!("moc failed: {e}") }).to_string(),
    }
}

/// Conservative backfill of faceted tags over pre-existing entries
/// (`touring memory backfill-tags`). Only entries with ZERO tags are
/// touched — the backfill fills the void, it never second-guesses an
/// `explicit`/`auto`/`code_sync` judgement — and every row it writes is
/// marked `source='backfill'` so trust ordering keeps it below all other
/// strata. Bounded per call (`--limit`, default 2000, the reindex budget
/// pattern) so it cannot monopolise the daemon actor; `remaining` in the
/// response tells the caller to run again. `--dry-run` measures without
/// writing.
pub fn cli_memory_backfill_tags(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::memory::tags;
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(2_000) as usize;
    let dry_run = payload
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let result = open_tagged_memory_db(rt).and_then(|conn| {
        let mut stmt = conn.prepare(
            "SELECT key, entry_type, file_path FROM memory_entries
             WHERE key NOT IN (SELECT entry_key FROM memory_tags)
             LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut tagged_entries = 0usize;
        let mut tags_written = 0usize;
        if !dry_run {
            for (key, entry_type, file_path) in &rows {
                let derived =
                    tags::derive_tags(key, entry_type, file_path.as_deref());
                for tag in &derived {
                    tags::upsert_tag(&conn, key, tag, tags::TagSource::Backfill)?;
                }
                if !derived.is_empty() {
                    tagged_entries += 1;
                    tags_written += derived.len();
                }
            }
        }
        let remaining: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memory_entries
             WHERE key NOT IN (SELECT entry_key FROM memory_tags)",
            [],
            |r| r.get(0),
        )?;
        Ok(serde_json::json!({
            "processed": rows.len(),
            "tagged_entries": tagged_entries,
            "tags_written": tags_written,
            "remaining": remaining,
            "dry_run": dry_run,
        }))
    });
    match result {
        Ok(v) => {
            let mut v = v;
            v["status"] = serde_json::json!(if dry_run { "measured" } else { "backfilled" });
            v.to_string()
        }
        Err(e) => serde_json::json!({ "error": format!("backfill-tags failed: {e}") }).to_string(),
    }
}

/// Re-harvests `#tags:` codetag anchors from source into snippet memories
/// (`touring memory sync-tags`). `--file` syncs one path (the post-write /
/// post-edit incremental path); `--dir` walks a tree (the batch backfill).
/// Tombstones keep the store faithful to the code: a deleted anchor deletes
/// its snippet entry.
pub fn cli_memory_sync_tags(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use touring_intelligence::rl::memory::codetag;
    let file = payload.get("file").and_then(|v| v.as_str());
    let dir = payload.get("dir").and_then(|v| v.as_str());
    if file.is_none() && dir.is_none() {
        return serde_json::json!({ "error": "--file or --dir is required" }).to_string();
    }
    let result: Result<serde_json::Value, String> = open_tagged_memory_db(rt)
        .map_err(|e| e.to_string())
        .and_then(|conn| {
            if let Some(f) = file {
                sync_one_tag_file(rt, &conn, f)
            } else {
                let root = dir
                    .map(std::path::PathBuf::from)
                    .map(|d| {
                        if d.is_absolute() {
                            d
                        } else {
                            rt.project_root.join(d)
                        }
                    })
                    .unwrap_or_else(|| rt.project_root.clone());
                // Keys are project-relative even when scanning a subdirectory
                // (A-2): same-named files in different dirs must never share
                // a snippet key.
                codetag::sync_tree(&conn, &root, &rt.project_root)
                    .map(|r| {
                        serde_json::json!({
                            "dir": root.display().to_string(),
                            "upserted": r.upserted,
                            "tombstoned": r.tombstoned,
                        })
                    })
                    .map_err(|e| e.to_string())
            }
        });
    match result {
        Ok(mut v) => {
            v["status"] = serde_json::json!("synced");
            v.to_string()
        }
        Err(e) => serde_json::json!({ "error": format!("sync-tags failed: {e}") }).to_string(),
    }
}

/// Syncs one file's anchors (the incremental post-write/post-edit path).
/// Keys stay project-relative regardless of the caller's cwd, so a hook and
/// a manual run address the same snippet entries.
fn sync_one_tag_file(
    rt: &HookRuntime,
    conn: &rusqlite::Connection,
    file: &str,
) -> Result<serde_json::Value, String> {
    use touring_intelligence::rl::memory::codetag;
    let path = std::path::Path::new(file);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        rt.project_root.join(path)
    };
    let rel = abs
        .strip_prefix(&rt.project_root)
        .unwrap_or(&abs)
        .to_string_lossy()
        .to_string();
    let text = std::fs::read_to_string(&abs).map_err(|e| format!("read {}: {e}", abs.display()))?;
    let report = codetag::sync_file(conn, &rel, &text).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "file": rel,
        "upserted": report.upserted,
        "tombstoned": report.tombstoned,
    }))
}

// =============================================================================
// S-13 (2026-06-06) — CEG X9 LEARN dependency-inversion seam.
// =============================================================================
//
// `gateway::deps::LearnRuntime` is the trait the gateway's X9 LEARN stage
// (`gateway/learn.rs`) uses to persist RL rewards, gotchas, and memory lessons
// *without* importing this `cli_handlers` module. Implementing it HERE — on the
// parent side — inverts the old `gateway → cli_handlers` edge into
// `cli_handlers → gateway::deps`, which is the correct direction for a future
// CEG crate extraction (parent depends on the leaf, never the reverse). Each
// method delegates to the canonical handler re-exported above, so there is no
// behavioural change and no logic duplication — only the dependency direction
// flips. Fail-open by construction: the handlers return a JSON string and never
// panic. See `crates/touring-hooks/src/gateway/deps.rs`.
impl crate::gateway::deps::LearnRuntime for crate::runtime::HookRuntime {
    fn learning_reward(&mut self, payload: &serde_json::Value) -> String {
        cli_learning_reward(self, payload)
    }

    fn gotcha_add(&mut self, payload: &serde_json::Value) -> String {
        cli_gotcha_add(self, payload)
    }

    fn memory_store(&mut self, payload: &serde_json::Value) -> String {
        cli_memory_store(self, payload)
    }
}

// S-13 (2026-06-06) — the full X9 LEARN runtime-service seam. Closes the last CEG
// edge: with this impl, `gateway/learn.rs` is generic over `CegRuntime` and no
// longer names `crate::runtime::HookRuntime`. Each method delegates to the
// `HookRuntime` field it abstracts; fail-open by construction.
impl crate::gateway::deps::CegRuntime for crate::runtime::HookRuntime {
    fn record_tool_outcome(&self, event_type: &str, payload: &[u8]) {
        if let (Some(actor), Some(ledger)) = (&self.actor_id, &self.cross_agent_ledger)
            && let Err(e) = ledger.write_event(actor, event_type, payload)
        {
            tracing::debug!(error = %e, "cross-agent ledger write_event failed (fail-open)");
        }
    }

    fn drift_cache_get(&self, scope: &str, key: &str) -> Option<String> {
        self.ctx.result_cache.get_result(scope, key)
    }

    fn drift_cache_put(&self, scope: &str, key: &str, value: String) {
        self.ctx.result_cache.cache_result(scope, key, value);
    }

    fn contract_attestation(&self) -> Option<&crate::gateway::harness_contract::HarnessContract> {
        self.contract_attestation.as_ref()
    }
}

#[cfg(test)]
mod memory_store_migration_tests {
    /// G2 regression guard (gotcha:ceg-impls-dual-memory-schema): a legacy DB
    /// without `last_accessed_at` used to be DROP+CREATE'd by this path —
    /// every memory lost. Migrations are additive-only now: the old row must
    /// survive a store against the legacy shape.
    #[test]
    fn legacy_db_is_migrated_not_destroyed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db = touring_foundation::TouringConfig::memory_db_canonical(tmp.path());
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        // Build the LEGACY shape BEFORE the runtime can create the modern one:
        // no last_accessed_at, no S4 columns.
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE memory_entries (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    tier TEXT NOT NULL DEFAULT 'local',
                    entry_type TEXT NOT NULL DEFAULT 'insight',
                    access_count INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                INSERT INTO memory_entries(key, value) VALUES ('ancient-lesson', 'precious');",
            )
            .unwrap();
        }
        let mut rt = crate::hook_runtime::HookRuntime::new(tmp.path()).unwrap();
        let out = super::cli_memory_store(
            &mut rt,
            &serde_json::json!({"key": "new-lesson", "value": "fresh", "entry_type": "lesson"}),
        );
        assert!(out.contains("stored"), "store should succeed: {out}");
        let conn = rusqlite::Connection::open(&db).unwrap();
        let survived: String = conn
            .query_row(
                "SELECT value FROM memory_entries WHERE key = 'ancient-lesson'",
                [],
                |r| r.get(0),
            )
            .expect("legacy row must survive the store");
        assert_eq!(survived, "precious");
        // And the new columns arrived additively.
        let has_col: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('memory_entries') WHERE name = 'last_accessed_at'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_col, "additive migration added the column");
    }
}
