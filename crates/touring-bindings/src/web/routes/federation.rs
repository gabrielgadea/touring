//! `/federation` — cross-workspace Sentrux atlas (Elite W2, SPEC §5.5).
//!
//! Elite re-skin of the Wave 3/W4 federation page onto the shared chrome
//! (`PageHero` + `KpiStrip` + numbered `Panel`s inside `el-main-editorial`).
//! The data layer keeps the Wave 3 federation payload contract intact and
//! renders **only real data**:
//!
//! * `/api/quality/federation` proxies `touring status -j --federate …`
//!   and unwraps the `"federation"` key. Without a configured
//!   `workspaces` spec the endpoint answers `{"error":"missing
//!   workspaces"}` — the page shows that message **verbatim** in a warn
//!   banner plus the CLI equivalence, never fabricated rows.
//! * When per-workspace rows are present (Wave 3 `FederationEntry`
//!   shape — `{workspace_id, workspace_root, signal:{…}}` — or a
//!   flattened `{name, signal_0_10000, bottleneck}` variant under a
//!   `workspaces`/`entries` key) the full atlas renders: distribution
//!   `MiniBars`, bottleneck histogram and the per-workspace table.
//! * When only the aggregate [`FederationSummary`] is present (the shape
//!   `compute_federation_snapshot` emits today: `{entries_input,
//!   entries_aggregated, errors, summary}`), the hero/KPIs and the
//!   histogram render from the real aggregate while the per-workspace
//!   panels state honestly that the payload carried no entry rows.

use std::collections::BTreeMap;

use leptos::prelude::*;
use leptos_meta::Title;

use crate::web::components::{KpiCell, KpiStrip, MiniBars, PageHero, Panel, ProgressTrack};
use crate::web::ctx::use_refresh_bus;
use crate::web::models::quality::FederationSummary;
use crate::web::services::fetch_json;

/// CLI equivalence shown next to the data (and in every empty state).
const FEDERATION_CMD: &str = "touring quality federation";

/// Canonical Sentrux bottleneck axes (Wave 3 `bottleneck_label` order).
pub const BOTTLENECK_AXES: [&str; 5] = [
    "modularity",
    "acyclicity",
    "depth",
    "equality",
    "redundancy",
];

/// One federated workspace distilled from the payload.
#[derive(Debug, Clone, PartialEq)]
pub struct FedWorkspace {
    /// Display identifier — workspace id or filesystem root.
    pub name: String,
    /// Composite signal in `[0, 10_000]`.
    pub signal: f64,
    /// Dominant bottleneck axis label (lowercase canonical).
    pub bottleneck: String,
}

/// Classified federation payload — drives the honest view states.
#[derive(Debug, Clone)]
pub enum FedState {
    /// Error envelope (e.g. `{"error":"missing workspaces"}`).
    Error(String),
    /// Per-workspace entries present (Wave 3 `FederationEntry` shape).
    Workspaces(Vec<FedWorkspace>),
    /// Aggregate-only payload (`summary` present, no per-entry rows).
    SummaryOnly(Box<FederationSummary>),
    /// Nothing parseable — honest empty.
    Empty,
}

/// Detect an error envelope (`{"error": "…"}`) in the payload.
pub fn payload_error(v: &serde_json::Value) -> Option<String> {
    v.get("error")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// Parse per-workspace rows from the federation payload.
///
/// Accepts the Wave 3 `FederationEntry` serialization
/// (`{workspace_id, workspace_root, signal: {signal_0_10000, bottleneck, …}}`)
/// and a flattened `{name, signal_0_10000, bottleneck}` variant, under
/// either a `workspaces` or an `entries` key. Rows without a resolvable
/// name + numeric signal are skipped (never invented).
pub fn parse_workspaces(v: &serde_json::Value) -> Vec<FedWorkspace> {
    let arr = ["workspaces", "entries"]
        .iter()
        .find_map(|k| v.get(*k).and_then(serde_json::Value::as_array));
    let Some(items) = arr else { return Vec::new() };
    items.iter().filter_map(parse_workspace_item).collect()
}

/// Parse a single per-workspace row (nested Wave 3 or flattened shape).
fn parse_workspace_item(item: &serde_json::Value) -> Option<FedWorkspace> {
    let name = [
        "workspace_id",
        "name",
        "workspace",
        "workspace_root",
        "root",
        "path",
    ]
    .iter()
    .find_map(|k| item.get(*k).and_then(serde_json::Value::as_str))?
    .to_string();
    let nested = item.get("signal").filter(|s| s.is_object());
    let signal = nested
        .and_then(|s| s.get("signal_0_10000"))
        .or_else(|| item.get("signal_0_10000"))
        .or_else(|| item.get("signal").filter(|s| s.is_number()))
        .and_then(serde_json::Value::as_f64)?;
    let bottleneck = nested
        .and_then(|s| s.get("bottleneck"))
        .or_else(|| item.get("bottleneck"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("—")
        .to_string();
    Some(FedWorkspace {
        name,
        signal,
        bottleneck,
    })
}

/// Classify the raw payload into one of the four honest view states.
pub fn classify_payload(v: &serde_json::Value) -> FedState {
    if let Some(msg) = payload_error(v) {
        return FedState::Error(msg);
    }
    let ws = parse_workspaces(v);
    if !ws.is_empty() {
        return FedState::Workspaces(ws);
    }
    if let Some(summary) = v
        .get("summary")
        .and_then(|s| serde_json::from_value::<FederationSummary>(s.clone()).ok())
    {
        if summary.entries > 0 {
            return FedState::SummaryOnly(Box::new(summary));
        }
    }
    FedState::Empty
}

/// Arithmetic mean (µ) of the per-workspace signals (`0.0` when empty).
pub fn mean_signal(ws: &[FedWorkspace]) -> f64 {
    if ws.is_empty() {
        return 0.0;
    }
    ws.iter().map(|w| w.signal).sum::<f64>() / ws.len() as f64
}

/// Population standard deviation (σ) of the signals
/// (`0.0` when fewer than two entries — matches the aggregator contract).
pub fn stddev_signal(ws: &[FedWorkspace]) -> f64 {
    if ws.len() < 2 {
        return 0.0;
    }
    let mu = mean_signal(ws);
    let var = ws.iter().map(|w| (w.signal - mu).powi(2)).sum::<f64>() / ws.len() as f64;
    var.sqrt()
}

/// Count workspaces per canonical bottleneck axis (fixed 5-axis order).
pub fn bottleneck_histogram(ws: &[FedWorkspace]) -> Vec<(&'static str, usize)> {
    BOTTLENECK_AXES
        .iter()
        .map(|axis| {
            let count = ws
                .iter()
                .filter(|w| w.bottleneck.eq_ignore_ascii_case(axis))
                .count();
            (*axis, count)
        })
        .collect()
}

/// Project a summary `bottleneck_distribution` map onto the canonical axes.
pub fn histogram_from_distribution(dist: &BTreeMap<String, u64>) -> Vec<(&'static str, u64)> {
    BOTTLENECK_AXES
        .iter()
        .map(|axis| (*axis, dist.get(*axis).copied().unwrap_or(0)))
        .collect()
}

/// Tail segment of a workspace path for compact display
/// (`/home/g/rust` → `rust`; names without `/` pass through).
pub fn path_tail(name: &str) -> String {
    name.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(name)
        .to_string()
}

/// Signed integer delta of a signal vs. the federation mean ("+212" / "-34" / "0").
pub fn format_delta(signal: f64, mu: f64) -> String {
    let d = (signal - mu).round() as i64;
    if d > 0 {
        format!("+{d}")
    } else {
        d.to_string()
    }
}

/// Color token for a signed delta string (positive green / negative rose).
pub fn delta_color_token(delta: &str) -> &'static str {
    match delta.trim_start().chars().next() {
        Some('+') => "var(--el-pos)",
        Some('-') => "var(--el-neg)",
        _ => "var(--el-fg-4)",
    }
}

/// `/federation` — cross-workspace atlas (Elite W2 §5.5).
#[component]
pub fn Federation() -> impl IntoView {
    let bus = use_refresh_bus();
    let res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move {
            fetch_json("/api/quality/federation")
                .await
                .unwrap_or_else(|e| serde_json::json!({ "error": e }))
        }
    });

    view! {
        <div class="page pg-federation">
            <Title text="Touring — Federation"/>
            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                {move || res.get().map(|payload| {
                    let state = classify_payload(&payload);
                    let stat_str = match &state {
                        FedState::Workspaces(ws) => format!("{:.0}", mean_signal(ws)),
                        FedState::SummaryOnly(s) => s.aggregate_signal_0_10000.to_string(),
                        _ => "—".to_string(),
                    };
                    let body = match state {
                        FedState::Error(msg) => error_body(msg).into_any(),
                        FedState::Workspaces(ws) => workspaces_body(ws).into_any(),
                        FedState::SummaryOnly(s) => summary_body(*s).into_any(),
                        FedState::Empty => empty_body().into_any(),
                    };
                    view! {
                        <div class="el-main-editorial">
                            <PageHero
                                eyebrow="SENTRUX · FEDERATION"
                                title="Federation"
                                title_em="atlas"
                                sub="Cross-workspace Sentrux atlas — every federated root contributes one composite signal; the spread between best and worst shows where the fleet needs attention first."
                                stat=Signal::derive(move || stat_str.clone())
                                stat_label="MEAN SIGNAL"
                                stat_denom="/ 10 000"
                            >
                                <a class="el-btn" href="/quality">"Quality"</a>
                                <a class="el-btn el-btn-primary" href="/dashboard">"Dashboard"</a>
                            </PageHero>
                            {body}
                        </div>
                    }
                })}
            </Suspense>
        </div>
    }
}

/// Honest error state — verbatim payload message, CLI equivalence, empty body.
fn error_body(msg: String) -> impl IntoView {
    view! {
        <div class="el-banner el-banner-warn" role="alert">
            <span class="pg-federation-banner-title">"federation unavailable — "</span>
            <span>{msg}</span>
        </div>
        <pre class="el-code">{FEDERATION_CMD}</pre>
        <div class="el-empty">
            <div class="el-eyebrow">"NO FEDERATED WORKSPACES"</div>
            <p class="pg-federation-empty-hint">
                "The API returned no federation data, so nothing is rendered — this page never fabricates rows. Provide a workspaces spec to the endpoint (ws1:/path1,ws2:/path2) and refresh."
            </p>
        </div>
    }
}

/// Honest empty — payload had neither error, workspaces nor summary.
fn empty_body() -> impl IntoView {
    view! {
        <pre class="el-code">{FEDERATION_CMD}</pre>
        <div class="el-empty">
            <div class="el-eyebrow">"NO FEDERATION DATA"</div>
            <p class="pg-federation-empty-hint">
                "The payload carried no error, no workspaces and no summary — nothing to render."
            </p>
        </div>
    }
}

/// Full atlas — KPIs, signal distribution, bottleneck histogram and the
/// per-workspace table (ascending by signal; worst row tinted).
fn workspaces_body(mut ws: Vec<FedWorkspace>) -> impl IntoView {
    ws.sort_by(|a, b| {
        a.signal
            .partial_cmp(&b.signal)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let n = ws.len();
    let mu = mean_signal(&ws);
    let sigma = stddev_signal(&ws);
    let best = ws.last().map(|w| w.signal).unwrap_or_default();
    let worst = ws.first().map(|w| w.signal).unwrap_or_default();
    let hist = bottleneck_histogram(&ws);
    let bars: Vec<f64> = ws.iter().map(|w| w.signal).collect();
    // Bars at/above the mean render accent — the below-µ tail stays muted.
    // (position is always Some for non-empty input since max >= µ; the
    // fallback `n` simply disables highlighting.)
    let highlight_from = ws.iter().position(|w| w.signal >= mu).unwrap_or(n);

    let count_str = n.to_string();
    let best_str = format!("{best:.0}");
    let worst_str = format!("{worst:.0}");
    let sigma_str = format!("{sigma:.0}");
    let dist_note = format!("µ {mu:.0} · σ {sigma:.0} · {n} workspaces, ascending");

    view! {
        <KpiStrip>
            <KpiCell
                label="Workspaces"
                value=Signal::derive(move || count_str.clone())
                sub="federated roots"
            />
            <KpiCell
                label="Best"
                value=Signal::derive(move || best_str.clone())
                sub="max signal"
            />
            <KpiCell
                label="Worst"
                value=Signal::derive(move || worst_str.clone())
                sub="min signal"
            />
            <KpiCell
                label="Sigma"
                value=Signal::derive(move || sigma_str.clone())
                sub="signal stddev"
            />
        </KpiStrip>

        <section class="pg-federation-grid">
            <Panel num="01" eyebrow="Distribution" cmd="signals ascending">
                <div class="pg-federation-dist">
                    <MiniBars
                        values=Signal::derive(move || bars.clone())
                        highlight_from=highlight_from
                        width=240.0
                        height=48.0
                    />
                    <div class="pg-federation-dist-note el-mono">{dist_note}</div>
                </div>
            </Panel>

            <Panel num="02" eyebrow="Bottleneck histogram">
                <div class="pg-federation-hist">
                    {hist.into_iter().map(|(axis, count)| {
                        let frac = if n > 0 { count as f64 / n as f64 } else { 0.0 };
                        view! {
                            <div class="pg-federation-hist-row">
                                <span class="pg-federation-hist-label">{axis}</span>
                                <div class="pg-federation-hist-track">
                                    <ProgressTrack
                                        value=Signal::derive(move || frac)
                                        color="var(--el-accent)"
                                        show_value=false
                                    />
                                </div>
                                <span class="el-mono pg-federation-hist-count">{count.to_string()}</span>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </Panel>
        </section>

        <Panel num="03" eyebrow="Workspaces" cmd="touring quality federation">
            <table class="el-table">
                <thead>
                    <tr>
                        <th>"Workspace"</th>
                        <th>"Signal"</th>
                        <th>"Bottleneck"</th>
                        <th>"Δ vs µ"</th>
                    </tr>
                </thead>
                <tbody>
                    {ws.into_iter().enumerate().map(|(i, w)| {
                        let is_worst = i == 0 && n > 1;
                        let frac = (w.signal / 10_000.0).clamp(0.0, 1.0);
                        let delta = format_delta(w.signal, mu);
                        let delta_col = delta_color_token(&delta);
                        let tag_class = if is_worst { "el-tag el-tag-warn" } else { "el-tag" };
                        let row_style = if is_worst { "background:var(--el-neg-soft)" } else { "" };
                        view! {
                            <tr style=row_style>
                                <td class="el-mono">{path_tail(&w.name)}</td>
                                <td>
                                    <div class="pg-federation-sig-cell">
                                        <span class="el-mono">{format!("{:.0}", w.signal)}</span>
                                        <div class="pg-federation-sig-bar">
                                            <ProgressTrack
                                                value=Signal::derive(move || frac)
                                                color="var(--el-accent)"
                                                show_value=false
                                            />
                                        </div>
                                    </div>
                                </td>
                                <td><span class=tag_class>{w.bottleneck.clone()}</span></td>
                                <td class="el-mono" style=format!("color:{delta_col}")>{delta}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </Panel>
    }
}

/// Aggregate-only state — real summary numbers; honest about missing rows.
fn summary_body(s: FederationSummary) -> impl IntoView {
    let n = s.entries;
    let count_str = n.to_string();
    let best_str = s.max_signal.to_string();
    let worst_str = s.min_signal.to_string();
    let sigma_str = format!("{:.0}", s.stddev_signal);
    let hist = histogram_from_distribution(&s.bottleneck_distribution);
    let best_name = s.best_workspace.clone().unwrap_or_else(|| "—".to_string());
    let worst_name = s.worst_workspace.clone().unwrap_or_else(|| "—".to_string());

    view! {
        <KpiStrip>
            <KpiCell
                label="Workspaces"
                value=Signal::derive(move || count_str.clone())
                sub="federated roots"
            />
            <KpiCell
                label="Best"
                value=Signal::derive(move || best_str.clone())
                sub="max signal"
            />
            <KpiCell
                label="Worst"
                value=Signal::derive(move || worst_str.clone())
                sub="min signal"
            />
            <KpiCell
                label="Sigma"
                value=Signal::derive(move || sigma_str.clone())
                sub="signal stddev"
            />
        </KpiStrip>

        <section class="pg-federation-grid">
            <Panel num="01" eyebrow="Distribution" cmd="aggregate only">
                <div class="el-empty">
                    <div class="el-eyebrow">"NO PER-WORKSPACE ROWS"</div>
                    <p class="pg-federation-empty-hint">
                        "The payload carried only the aggregate summary — per-workspace signals were not included, so no bars are drawn."
                    </p>
                </div>
            </Panel>

            <Panel num="02" eyebrow="Bottleneck histogram">
                <div class="pg-federation-hist">
                    {hist.into_iter().map(|(axis, count)| {
                        let frac = if n > 0 { count as f64 / n as f64 } else { 0.0 };
                        view! {
                            <div class="pg-federation-hist-row">
                                <span class="pg-federation-hist-label">{axis}</span>
                                <div class="pg-federation-hist-track">
                                    <ProgressTrack
                                        value=Signal::derive(move || frac)
                                        color="var(--el-accent)"
                                        show_value=false
                                    />
                                </div>
                                <span class="el-mono pg-federation-hist-count">{count.to_string()}</span>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </Panel>
        </section>

        <Panel num="03" eyebrow="Workspaces" cmd="touring quality federation">
            <div class="el-empty">
                <div class="el-eyebrow">"ENTRIES NOT LISTED IN PAYLOAD"</div>
                <p class="pg-federation-empty-hint">
                    {format!("{n} workspaces were aggregated server-side, but the endpoint returned summary fields only — no per-workspace rows to list.")}
                </p>
                <div class="pg-federation-summary-tags">
                    <span class="el-tag">{format!("best · {best_name}")}</span>
                    <span class="el-tag el-tag-warn">{format!("worst · {worst_name}")}</span>
                </div>
            </div>
        </Panel>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fed(name: &str, signal: f64, bottleneck: &str) -> FedWorkspace {
        FedWorkspace {
            name: name.to_string(),
            signal,
            bottleneck: bottleneck.to_string(),
        }
    }

    #[test]
    fn error_payload_yields_honest_error_state() {
        let payload = serde_json::json!({ "error": "missing workspaces" });
        assert_eq!(
            payload_error(&payload),
            Some("missing workspaces".to_string())
        );
        match classify_payload(&payload) {
            FedState::Error(msg) => assert_eq!(msg, "missing workspaces"),
            other => panic!("expected Error state, got {other:?}"),
        }
        // No rows are ever invented for an error payload.
        assert!(parse_workspaces(&payload).is_empty());
    }

    #[test]
    fn parses_wave3_federation_entry_shape() {
        let payload = serde_json::json!({
            "workspaces": [
                {
                    "workspace_id": "rust",
                    "workspace_root": "/home/g/.claude/rust",
                    "signal": { "signal_0_10000": 7616, "bottleneck": "acyclicity" },
                    "timestamp_ms": 1
                },
                {
                    "workspace_id": "konverter",
                    "workspace_root": "/home/g/projects/konverter",
                    "signal": { "signal_0_10000": 6200, "bottleneck": "modularity" },
                    "timestamp_ms": 1
                }
            ]
        });
        let ws = parse_workspaces(&payload);
        assert_eq!(
            ws,
            vec![
                fed("rust", 7616.0, "acyclicity"),
                fed("konverter", 6200.0, "modularity")
            ]
        );
        assert!(matches!(classify_payload(&payload), FedState::Workspaces(v) if v.len() == 2));
    }

    #[test]
    fn parses_flattened_rows_under_entries_key() {
        let payload = serde_json::json!({
            "entries": [
                { "name": "a", "signal_0_10000": 1000, "bottleneck": "depth" },
                { "name": "b", "signal": 3000 },
                { "bottleneck": "depth" }
            ]
        });
        let ws = parse_workspaces(&payload);
        // Third row has no name/signal — skipped, never invented.
        assert_eq!(ws, vec![fed("a", 1000.0, "depth"), fed("b", 3000.0, "—")]);
    }

    #[test]
    fn summary_only_payload_classifies_with_real_aggregate() {
        let payload = serde_json::json!({
            "entries_input": 2,
            "entries_aggregated": 2,
            "errors": [],
            "summary": {
                "entries": 2,
                "aggregate_signal_0_10000": 6908,
                "min_signal": 6200,
                "max_signal": 7616,
                "stddev_signal": 708.0,
                "bottleneck_distribution": { "acyclicity": 1, "modularity": 1 },
                "worst_workspace": "konverter",
                "best_workspace": "rust"
            }
        });
        match classify_payload(&payload) {
            FedState::SummaryOnly(s) => {
                assert_eq!(s.entries, 2);
                assert_eq!(s.aggregate_signal_0_10000, 6908);
                assert_eq!(s.worst_workspace.as_deref(), Some("konverter"));
                let hist = histogram_from_distribution(&s.bottleneck_distribution);
                assert_eq!(hist[0], ("modularity", 1));
                assert_eq!(hist[1], ("acyclicity", 1));
                assert_eq!(hist[2], ("depth", 0));
            }
            other => panic!("expected SummaryOnly state, got {other:?}"),
        }
    }

    #[test]
    fn zero_entry_summary_and_junk_payloads_are_empty() {
        let zero = serde_json::json!({ "summary": { "entries": 0 } });
        assert!(matches!(classify_payload(&zero), FedState::Empty));
        let junk = serde_json::json!({ "unrelated": true });
        assert!(matches!(classify_payload(&junk), FedState::Empty));
    }

    #[test]
    fn mean_and_population_stddev() {
        let ws = vec![fed("a", 1000.0, "depth"), fed("b", 3000.0, "depth")];
        assert_eq!(mean_signal(&ws), 2000.0);
        assert_eq!(stddev_signal(&ws), 1000.0);
        // Degenerate inputs stay honest zeros.
        assert_eq!(mean_signal(&[]), 0.0);
        assert_eq!(stddev_signal(&[fed("a", 5000.0, "depth")]), 0.0);
    }

    #[test]
    fn histogram_counts_canonical_axes_in_order() {
        let ws = vec![
            fed("a", 1.0, "acyclicity"),
            fed("b", 2.0, "acyclicity"),
            fed("c", 3.0, "Equality"), // case-insensitive
            fed("d", 4.0, "tied"),     // non-canonical → not counted
        ];
        let hist = bottleneck_histogram(&ws);
        assert_eq!(
            hist,
            vec![
                ("modularity", 0),
                ("acyclicity", 2),
                ("depth", 0),
                ("equality", 1),
                ("redundancy", 0),
            ]
        );
    }

    #[test]
    fn delta_formatting_and_color_tokens() {
        assert_eq!(format_delta(7616.0, 6908.0), "+708");
        assert_eq!(format_delta(6200.0, 6908.0), "-708");
        assert_eq!(format_delta(6908.0, 6908.0), "0");
        assert_eq!(delta_color_token("+708"), "var(--el-pos)");
        assert_eq!(delta_color_token("-708"), "var(--el-neg)");
        assert_eq!(delta_color_token("0"), "var(--el-fg-4)");
    }

    #[test]
    fn path_tail_extracts_leaf() {
        assert_eq!(path_tail("/home/g/.claude/rust"), "rust");
        assert_eq!(path_tail("/home/g/projects/konverter/"), "konverter");
        assert_eq!(path_tail("bare-name"), "bare-name");
        assert_eq!(path_tail("/"), "/");
    }
}
