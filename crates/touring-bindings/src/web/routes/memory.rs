//! /memory — Memory palace. Elite W3 (SPEC 2026-06-12 §5.12):
//! composed hero stat (entry count, pt-BR thousands), KPI strip from
//! `/api/memory/stats`, an honest isometric palace whose wings are
//! DERIVED from real memory-key prefixes (one level only — the store
//! exposes flat keys, not a wing/room/closet/drawer hierarchy, so the
//! page never pretends it does), a live RRF recall console over
//! `/api/memory/recall?q=` and a wing-filterable entries list.
//!
//! Verified entry shape (live capture 2026-06-12):
//! `{key, source_db, tier, type, value}` — entries carry no numeric
//! score field, so no per-entry ProgressTrack is rendered (honesty
//! over decoration). Top-level recall payload:
//! `{ann_results, count, entries, diagnostics}` where diagnostics are
//! `{code, message, severity}` rows (e.g. M-510 TF-IDF, M-520 RRF).

use crate::web::components::{IsoPalace, KpiCell, KpiStrip, PageHero, Panel, iso_palace::Wing};
use crate::web::ctx::use_refresh_bus;
use crate::web::services::{fetch_memory_recall, fetch_memory_stats};
use leptos::prelude::*;
use leptos_meta::Title;

/// Generic seed query for the palace derivation (panel 01) and the
/// initial entries list (panel 03) before any user recall.
const PALACE_SEED_QUERY: &str = "touring";

/// Wing cap — top key prefixes by count rendered as isometric blocks.
const PALACE_WING_CAP: usize = 6;

/// Key length cap in list rows (the full key stays in `title`).
const KEY_DISPLAY_CHARS: usize = 60;

/// Namespace prefix of a memory key — the text before the first `':'`.
/// Keys without a separator (or with an empty head) group as `"misc"`.
pub fn key_prefix(key: &str) -> String {
    match key.find(':') {
        Some(0) | None => "misc".to_string(),
        Some(i) => key[..i].to_string(),
    }
}

/// Group recall `entries` by key prefix → top `top_n` `(prefix, count)`
/// pairs ordered by count desc, then prefix asc (deterministic wings).
pub fn wing_prefixes(payload: &serde_json::Value, top_n: usize) -> Vec<(String, usize)> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    if let Some(arr) = payload.get("entries").and_then(|v| v.as_array()) {
        for entry in arr {
            if let Some(key) = entry.get("key").and_then(|k| k.as_str()) {
                *counts.entry(key_prefix(key)).or_insert(0) += 1;
            }
        }
    }
    let mut wings: Vec<(String, usize)> = counts.into_iter().collect();
    wings.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    wings.truncate(top_n);
    wings
}

/// pt-BR thousands separator (`'.'`): `5296` → `"5.296"`.
pub fn fmt_thousands(n: u64) -> String {
    let digits = n.to_string();
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

/// Char-boundary-safe truncation with a trailing ellipsis.
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// Reactive KPI string for a (possibly nested) u64 counter inside the
/// `/api/memory/stats` payload — `"—"` until the fetch resolves.
fn kpi_count(
    stats: LocalResource<serde_json::Value>,
    path: &'static [&'static str],
) -> Signal<String> {
    Signal::derive(move || {
        let mut cur = stats.get().unwrap_or(serde_json::Value::Null);
        for seg in path {
            cur = cur.get(seg).cloned().unwrap_or(serde_json::Value::Null);
        }
        cur.as_u64()
            .map(fmt_thousands)
            .unwrap_or_else(|| "—".to_string())
    })
}

/// Memory palace page — hero · KPI strip · palace/recall grid · entries.
#[component]
pub fn MemoryRecall() -> impl IntoView {
    let bus = use_refresh_bus();

    // Aggregate counters (`touring memory stats`) — refresh-bus aware.
    let stats = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move {
            fetch_memory_stats()
                .await
                .ok()
                .unwrap_or(serde_json::Value::Null)
        }
    });

    // Palace seed — a generic recall whose key prefixes derive the wings.
    let palace_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move {
            fetch_memory_recall(PALACE_SEED_QUERY)
                .await
                .ok()
                .unwrap_or(serde_json::Value::Null)
        }
    });

    // User recall — Null until a non-empty query is submitted.
    let (query, set_query) = signal(String::new());
    let (submitted, set_submitted) = signal(String::new());
    let recall_res = LocalResource::new(move || {
        let q = submitted.get();
        async move {
            let q = q.trim().to_string();
            if q.is_empty() {
                serde_json::Value::Null
            } else {
                fetch_memory_recall(&q)
                    .await
                    .ok()
                    .unwrap_or(serde_json::Value::Null)
            }
        }
    });

    // Wings derived from REAL key prefixes of the seed recall — the
    // panel label says so ("derived from key prefixes", SPEC §5.12).
    let selected: RwSignal<Option<usize>> = RwSignal::new(None);
    let wings: Signal<Vec<Wing>> = Signal::derive(move || {
        let json = palace_res.get().unwrap_or(serde_json::Value::Null);
        wing_prefixes(&json, PALACE_WING_CAP)
            .into_iter()
            .map(|(label, count)| Wing { label, count })
            .collect()
    });
    let selected_prefix: Signal<Option<String>> = Signal::derive(move || {
        selected
            .get()
            .and_then(|i| wings.get().get(i).map(|w| w.label.clone()))
    });

    // Active payload — the user recall once submitted, else the seed.
    let active_json: Signal<serde_json::Value> = Signal::derive(move || {
        if submitted.get().trim().is_empty() {
            palace_res.get().unwrap_or(serde_json::Value::Null)
        } else {
            recall_res.get().unwrap_or(serde_json::Value::Null)
        }
    });

    // Panel 03 rows: (key, prefix) of the last recall, wing-filtered.
    let filtered_entries: Signal<Vec<(String, String)>> = Signal::derive(move || {
        let json = active_json.get();
        let sel = selected_prefix.get();
        let mut rows = Vec::new();
        if let Some(arr) = json.get("entries").and_then(|v| v.as_array()) {
            for entry in arr {
                if let Some(key) = entry.get("key").and_then(|k| k.as_str()) {
                    let prefix = key_prefix(key);
                    if sel.as_deref().map(|s| s == prefix).unwrap_or(true) {
                        rows.push((key.to_string(), prefix));
                    }
                }
            }
        }
        rows
    });

    // RRF fusion metadata + diagnostics — real fields of the payload.
    let recall_meta: Signal<Option<(u64, u64)>> = Signal::derive(move || {
        let json = active_json.get();
        match (
            json.get("ann_results").and_then(|v| v.as_u64()),
            json.get("count").and_then(|v| v.as_u64()),
        ) {
            (Some(ann), Some(count)) => Some((ann, count)),
            _ => None,
        }
    });
    let diagnostics: Signal<Vec<(String, String, String)>> = Signal::derive(move || {
        active_json
            .get()
            .get("diagnostics")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|d| {
                        (
                            d.get("code")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string(),
                            d.get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("")
                                .to_string(),
                            d.get("severity")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    });

    let entries_kpi = kpi_count(stats, &["memory_entry_count"]);
    let files_kpi = kpi_count(stats, &["file_count"]);
    let relations_kpi = kpi_count(stats, &["relation_count"]);
    let gotchas_kpi = kpi_count(stats, &["gotcha_stats", "total_count"]);

    view! {
        <div class="page pg-memory">
            <Title text="Touring — Memory"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="CODE INTELLIGENCE · MEMORY"
                    title="Memory"
                    title_em="palace"
                    sub="FTS5 + semantic store fused by RRF. The palace below is honest: one wing per real key-namespace prefix, block height by entry count — a single derived level, never an invented hierarchy."
                    stat=kpi_count(stats, &["memory_entry_count"])
                    stat_label="ENTRIES"
                >
                    <button
                        class="el-btn"
                        disabled=true
                        title="CLI-only: touring diary write — no web write surface"
                    >
                        "Diary write"
                    </button>
                    <a class="el-btn el-btn-primary" href="#memory-recall-form">"Recall"</a>
                </PageHero>

                <KpiStrip>
                    <KpiCell label="Entries" value=entries_kpi sub="FTS5 + semantic store"/>
                    <KpiCell label="Files tracked" value=files_kpi sub="co-edit graph"/>
                    <KpiCell label="Relations" value=relations_kpi sub="CRDT knowledge graph"/>
                    <KpiCell label="Gotchas" value=gotchas_kpi sub="pitfall database"/>
                </KpiStrip>

                <section class="pg-memory-grid">
                    // ── 01 · Palace — wings DERIVED from real key prefixes ──
                    <Panel num="01" eyebrow="Palace" cmd="derived from key prefixes">
                        <Suspense fallback=|| view! {
                            <div class="el-skeleton" aria-label="loading palace"></div>
                        }>
                            {move || palace_res.get().map(|_| view! {
                                <div class="pg-memory-palace">
                                    <IsoPalace wings=wings selected=selected/>
                                </div>
                            })}
                        </Suspense>
                        <p class="pg-memory-palace-note">
                            "Wings are derived from real memory-key prefixes (text before the first ':'), top 6 by count over a generic seed recall. Click a wing to filter the entries panel; click again to clear."
                        </p>
                    </Panel>

                    // ── 02 · Recall console over /api/memory/recall ─────────
                    <Panel num="02" eyebrow="Recall" cmd="touring memory recall <q>">
                        <form
                            class="pg-memory-form"
                            id="memory-recall-form"
                            on:submit=move |ev| {
                                ev.prevent_default();
                                set_submitted.set(query.get());
                            }
                        >
                            <input
                                type="text"
                                class="pg-memory-input"
                                placeholder="Recall past lessons… e.g. quality"
                                prop:value=move || query.get()
                                on:input=move |ev| set_query.set(event_target_value(&ev))
                            />
                            <button type="submit" class="el-btn el-btn-primary">"Recall"</button>
                        </form>

                        {move || {
                            let q = submitted.get().trim().to_string();
                            if q.is_empty() {
                                return view! {
                                    <p class="pg-memory-hint">
                                        "Type a topic and press Recall — RRF fusion over FTS5 + TF-IDF + ANN. The panels are seeded with a generic \"touring\" recall."
                                    </p>
                                }.into_any();
                            }
                            match recall_res.get() {
                                None => view! {
                                    <div class="el-skeleton" aria-label="recalling"></div>
                                }.into_any(),
                                Some(json) => {
                                    // Verified shape: entries carry key/source_db/
                                    // tier/type/value — no numeric score field, so
                                    // no ProgressTrack per row (real data only).
                                    let keys: Vec<String> = json
                                        .get("entries")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|e| {
                                                    e.get("key")
                                                        .and_then(|k| k.as_str())
                                                        .map(|s| s.to_string())
                                                })
                                                .collect()
                                        })
                                        .unwrap_or_default();
                                    if keys.is_empty() {
                                        view! {
                                            <p class="pg-memory-hint">
                                                "No entries recalled for "
                                                <span class="el-mono">{q}</span>
                                                "."
                                            </p>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="pg-memory-results">
                                                {keys.into_iter().map(|key| {
                                                    let full = key.clone();
                                                    let shown = truncate_chars(&key, KEY_DISPLAY_CHARS);
                                                    view! {
                                                        <div class="el-row">
                                                            <span class="el-mono pg-memory-key" title=full>
                                                                {shown}
                                                            </span>
                                                        </div>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        }.into_any()
                                    }
                                }
                            }
                        }}

                        {move || recall_meta.get().map(|(ann, count)| view! {
                            <div class="el-mono pg-memory-meta">
                                {format!("ann_results: {ann} · count: {count} — RRF fused")}
                            </div>
                        })}

                        // Real RRF diagnostics (M-510 / M-520 …) — panel footer.
                        {move || {
                            let diags = diagnostics.get();
                            (!diags.is_empty()).then(|| view! {
                                <div class="pg-memory-diag">
                                    {diags.into_iter().map(|(code, message, severity)| view! {
                                        <div class="el-mono pg-memory-diag-row">
                                            <span class="pg-memory-diag-code">{code}</span>
                                            " · "
                                            {message}
                                            " ["
                                            {severity}
                                            "]"
                                        </div>
                                    }).collect_view()}
                                </div>
                            })
                        }}
                    </Panel>
                </section>

                // ── 03 · Entries of the last recall, wing-filtered ──────────
                <Panel num="03" eyebrow="Entries" cmd="grouped by key prefix">
                    {move || {
                        let rows = filtered_entries.get();
                        let head = match selected_prefix.get() {
                            Some(p) => format!(
                                "{} entries · wing \u{201c}{p}\u{201d}",
                                rows.len()
                            ),
                            None => format!("{} entries · all wings", rows.len()),
                        };
                        view! {
                            <div class="pg-memory-entries">
                                <div class="el-mono pg-memory-entries-head">{head}</div>
                                {if rows.is_empty() {
                                    view! {
                                        <p class="pg-memory-hint">
                                            "No entries in this wing for the current recall."
                                        </p>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="pg-memory-results">
                                            {rows.into_iter().map(|(key, prefix)| {
                                                let full = key.clone();
                                                let shown = truncate_chars(&key, KEY_DISPLAY_CHARS);
                                                view! {
                                                    <div class="el-row">
                                                        <span class="el-mono pg-memory-key" title=full>
                                                            {shown}
                                                        </span>
                                                        <span class="el-tag">{prefix}</span>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }}
                            </div>
                        }
                    }}
                </Panel>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_prefix_splits_on_first_colon() {
        assert_eq!(key_prefix("vgp:create:crates/x.rs"), "vgp");
        assert_eq!(key_prefix("lesson:wave1"), "lesson");
        assert_eq!(key_prefix("nocolon"), "misc");
        assert_eq!(key_prefix(":leading"), "misc");
    }

    #[test]
    fn wing_prefixes_groups_sorts_and_caps() {
        let payload = serde_json::json!({"entries": [
            {"key": "vgp:create:a"}, {"key": "vgp:other"},
            {"key": "create:y"},
            {"key": "nocolon"},
            {"key": "lesson:z"}, {"key": "lesson:w"}, {"key": "lesson:v"},
        ]});
        let wings = wing_prefixes(&payload, 3);
        assert_eq!(
            wings,
            vec![
                ("lesson".to_string(), 3),
                ("vgp".to_string(), 2),
                // count tie (create=1, misc=1) breaks alphabetically.
                ("create".to_string(), 1),
            ]
        );
    }

    #[test]
    fn wing_prefixes_handles_empty_and_malformed_payloads() {
        assert!(wing_prefixes(&serde_json::Value::Null, 6).is_empty());
        assert!(wing_prefixes(&serde_json::json!({"entries": []}), 6).is_empty());
        assert!(wing_prefixes(&serde_json::json!({"error": "boom"}), 6).is_empty());
    }

    #[test]
    fn fmt_thousands_pt_br_dots() {
        assert_eq!(fmt_thousands(0), "0");
        assert_eq!(fmt_thousands(999), "999");
        assert_eq!(fmt_thousands(5296), "5.296");
        assert_eq!(fmt_thousands(1477), "1.477");
        assert_eq!(fmt_thousands(1_234_567), "1.234.567");
    }

    #[test]
    fn truncate_chars_is_char_boundary_safe() {
        assert_eq!(truncate_chars("short", 60), "short");
        let long = "x".repeat(80);
        let cut = truncate_chars(&long, 60);
        assert_eq!(cut.chars().count(), 60);
        assert!(cut.ends_with('…'));
        // Multibyte input must not split a char boundary.
        let accented = "é".repeat(70);
        let cut2 = truncate_chars(&accented, 60);
        assert_eq!(cut2.chars().count(), 60);
    }
}
