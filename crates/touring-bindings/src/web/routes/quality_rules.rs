//! /quality/rules — TOML ruleset editor with real save (Elite W3, SPEC §5.3).
//!
//! Three live endpoints back this page (shapes captured via curl against the
//! running server on 2026-06-12 — see helper doc-comments for the verbatim
//! payloads):
//!
//! * `GET  /api/quality/rules` → `{exists, path, content}` — when the file
//!   does not exist yet, `content` carries the canonical MetricRuleSet v1.0
//!   default template.
//! * `PUT  /api/quality/rules` → `{saved: true, path, bytes}` on success or
//!   `{error: "invalid TOML: ..."}` with the real parser message (line/col).
//! * `GET  /api/quality/rules/evaluate?ruleset_path=<path>` → output of
//!   `touring quality-signal -j --rules <path>`.
//!
//! Design note (deliberate W3 scope cut): the editor is a plain mono
//! `<textarea>` — a syntax-highlight overlay needs a scroll-synced mirror
//! div + tokenizer and adds zero data value, so it is intentionally not
//! attempted here. The save flow is real: PUT body → render the server's
//! response verbatim → on success, re-run the evaluation automatically.

use crate::web::components::{PageHero, Panel};
use crate::web::services::{fetch_json, put_text};
use leptos::prelude::*;
use leptos_meta::Title;
use serde_json::Value;

/// One parsed rule violation row, ready for rendering.
///
/// Field names mirror the canonical `MetricViolation` serde shape from
/// `touring-analysis/src/rules/types.rs:197` (severity serializes
/// lowercase: `"deny" | "warn" | "info"`; metric is snake_case, e.g.
/// `"cycle_count"`; op is lowercase, e.g. `"eq"`).
#[derive(Debug, Clone, PartialEq)]
pub struct RuleViolation {
    /// Name of the rule that fired (`[[rule]] name`).
    pub rule_name: String,
    /// `"deny" | "warn" | "info"` (lowercase serde of `Severity`).
    pub severity: String,
    /// Metric inspected (snake_case serde of `MetricKind`).
    pub metric: String,
    /// Comparison operator copied from the rule (lowercase serde of `Op`).
    pub op: String,
    /// Threshold value copied from the rule.
    pub threshold: Option<f64>,
    /// Actual metric value observed in the workspace.
    pub actual: Option<f64>,
    /// `Some("path")` / `Some("file:fn")` for scoped rules, `None` for
    /// workspace-level rules.
    pub location: Option<String>,
    /// Human-readable message (rule `message` field or a default).
    pub message: String,
}

/// Parse the `GET /api/quality/rules` response into `(exists, path, content)`.
///
/// Captured real shape (2026-06-12, server on :3000):
///
/// ```json
/// {"content":"version = \"1.0\"\n\n[[rule]]\nname = \"no-god-files\"...",
///  "exists":false,
///  "path":"/home/gabrielgadea/.claude/rust/.claude/touring/quality-rules.toml"}
/// ```
pub fn parse_rules_response(v: &Value) -> (bool, String, String) {
    let exists = v.get("exists").and_then(Value::as_bool).unwrap_or(false);
    let path = v
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let content = v
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    (exists, path, content)
}

/// Parse the `PUT /api/quality/rules` response into `Ok((path, bytes))` or
/// `Err(parser_message)`.
///
/// Captured real shapes (2026-06-12, server on :3000):
///
/// ```json
/// {"bytes":271,
///  "path":"/home/gabrielgadea/.claude/rust/.claude/touring/quality-rules.toml",
///  "saved":true}
/// ```
///
/// ```json
/// {"error":"invalid TOML: TOML parse error at line 2, column 7\n  |\n2 | [[rule]\n  |       ^\ninvalid table header\nexpected `.`, `]]`\n"}
/// ```
pub fn parse_save_response(v: &Value) -> Result<(String, u64), SaveResponseError> {
    if let Some(err) = v.get("error").and_then(Value::as_str) {
        return Err(SaveResponseError::Backend(err.to_string()));
    }
    if !v.get("saved").and_then(Value::as_bool).unwrap_or(false) {
        return Err(SaveResponseError::Unexpected(format!("{v}")));
    }
    let path = v
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let bytes = v.get("bytes").and_then(Value::as_u64).unwrap_or(0);
    Ok((path, bytes))
}

/// Error from [`parse_save_response`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
pub enum SaveResponseError {
    /// The backend returned an `{"error": ...}` payload.
    #[error("{0}")]
    Backend(String),
    /// The response did not contain the expected `"saved": true`.
    #[error("unexpected save response: {0}")]
    Unexpected(String),
}

/// Extract rule violations from the evaluate response.
///
/// Captured real shape (2026-06-12): the current released binary behind
/// `GET /api/quality/rules/evaluate` (`touring quality-signal -j --rules
/// <path>`) returns the workspace signal with top-level keys
/// `["bottleneck","computed_at","diagnostics","raw","root","root_causes",
/// "signal_0_10000","signal_normalized"]` and **no** `violations` array.
/// The daemon MCP variant (`touring-server/src/server/tools_quality_signal.rs`)
/// emits `"violations": [MetricViolation, ...]` where each entry serializes
/// per `touring-analysis/src/rules/types.rs:197` as:
///
/// ```json
/// {"rule_name":"no-cycles","severity":"deny","metric":"cycle_count",
///  "op":"eq","threshold":0.0,"actual":11.0,"message":"..."}
/// ```
///
/// (`location` is present only for per-file / per-function rules.)
///
/// This parser therefore consumes the `violations` array when present
/// (forward-compatible with the CLI gaining the same emission) and
/// returns an empty vec otherwise — the page then falls back to the
/// real composite-signal line so the panel never shows fabricated data.
pub fn violations_of(v: &Value) -> Vec<RuleViolation> {
    let Some(items) = v.get("violations").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let rule_name = item.get("rule_name").and_then(Value::as_str)?.to_string();
            Some(RuleViolation {
                rule_name,
                severity: item
                    .get("severity")
                    .and_then(Value::as_str)
                    .unwrap_or("warn")
                    .to_string(),
                metric: item
                    .get("metric")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                op: item
                    .get("op")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                threshold: item.get("threshold").and_then(Value::as_f64),
                actual: item.get("actual").and_then(Value::as_f64),
                location: item
                    .get("location")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                message: item
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect()
}

/// Mono detail line for a violation card: `metric op threshold · got actual [· location]`.
pub fn violation_detail(v: &RuleViolation) -> String {
    let thr = v
        .threshold
        .map(|t| format!("{t}"))
        .unwrap_or_else(|| "—".to_string());
    let act = v
        .actual
        .map(|a| format!("{a}"))
        .unwrap_or_else(|| "—".to_string());
    let loc = v
        .location
        .as_deref()
        .map(|l| format!(" · {l}"))
        .unwrap_or_default();
    format!("{} {} {} · got {}{}", v.metric, v.op, thr, act, loc)
}

/// Run the real evaluation for `path` and store the raw response (or a
/// synthetic `{"error": ...}` on transport failure) into `eval_state`.
async fn evaluate_into(
    path: String,
    eval_busy: RwSignal<bool>,
    eval_state: RwSignal<Option<Value>>,
) {
    eval_busy.set(true);
    let url = format!("/api/quality/rules/evaluate?ruleset_path={path}");
    match fetch_json(&url).await {
        Ok(v) => eval_state.set(Some(v)),
        Err(e) => eval_state.set(Some(serde_json::json!({ "error": e }))),
    }
    eval_busy.set(false);
}

/// `/quality/rules` — TOML ruleset editor with real PUT save + evaluate.
#[component]
pub fn QualityRules() -> impl IntoView {
    let initial = LocalResource::new(move || async move { fetch_json("/api/quality/rules").await });

    view! {
        <div class="page pg-qrules">
            <Title text="Touring — Quality Rules"/>
            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                {move || initial.get().map(|res| match res {
                    Ok(v) => view! { <RulesBody init=v/> }.into_any(),
                    Err(e) => view! {
                        <div class="el-banner el-banner-warn">
                            {format!("failed to load ruleset: {e}")}
                        </div>
                    }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

/// Page body, created once the initial `GET /api/quality/rules` resolves —
/// the editor signal is seeded from the real server payload (existing file
/// content, or the canonical default template when `exists=false`).
#[component]
fn RulesBody(init: Value) -> impl IntoView {
    let (exists0, path0, content0) = parse_rules_response(&init);

    let content = RwSignal::new(content0);
    let file_path = RwSignal::new(path0);
    let exists = RwSignal::new(exists0);
    let save_state = RwSignal::new(Option::<Result<(String, u64), String>>::None);
    let save_busy = RwSignal::new(false);
    let eval_state = RwSignal::new(Option::<Value>::None);
    let eval_busy = RwSignal::new(false);

    // PUT the editor content; on a successful save, re-run the evaluation
    // automatically against the real path.
    let on_save = move |_| {
        if save_busy.get_untracked() {
            return;
        }
        let body = content.get_untracked();
        save_busy.set(true);
        leptos::task::spawn_local(async move {
            match put_text("/api/quality/rules", &body).await {
                Ok(v) => {
                    // `parse_save_response` returns a typed `SaveResponseError`;
                    // the UI signal unifies it with `put_text`'s error into one
                    // `String` display channel, so stringify here.
                    let parsed = parse_save_response(&v).map_err(|e| e.to_string());
                    let saved_ok = parsed.is_ok();
                    save_state.set(Some(parsed));
                    if saved_ok {
                        exists.set(true);
                        evaluate_into(file_path.get_untracked(), eval_busy, eval_state).await;
                    }
                }
                Err(e) => save_state.set(Some(Err(e))),
            }
            save_busy.set(false);
        });
    };

    let on_evaluate = move |_| {
        if eval_busy.get_untracked() {
            return;
        }
        leptos::task::spawn_local(evaluate_into(
            file_path.get_untracked(),
            eval_busy,
            eval_state,
        ));
    };

    view! {
        <div class="el-main-editorial">
            <PageHero
                eyebrow="SENTRUX · RULESET"
                title="Rules"
                title_em="editor"
                sub="Edit the MetricRuleSet TOML evaluated by the Sentrux signal. Each [[rule]] compares a raw counter against a threshold; severity deny blocks the pre-edit hook, warn and info only score. Saving writes the real file and re-runs the evaluation."
            >
                <a class="el-btn" href="/quality">"Quality"</a>
                <button
                    class="el-btn el-btn-primary"
                    disabled=move || save_busy.get()
                    on:click=on_save
                >
                    {move || if save_busy.get() { "Saving…" } else { "Save" }}
                </button>
            </PageHero>

            <section class="pg-qrules-grid">
                // ── 01 · rules.toml editor ─────────────────────────────
                <Panel eyebrow="RULES.TOML" num="01" cmd="GET · PUT /api/quality/rules">
                    <div class="pg-qrules-pathrow">
                        // `Panel::cmd` is &'static str, so the dynamic real
                        // path lives here in the body instead.
                        <span class="el-cmd-label pg-qrules-path">
                            {move || file_path.get()}
                        </span>
                        <span class="el-mono pg-qrules-meta">
                            {move || {
                                let c = content.get();
                                format!("{} lines · {} bytes", c.lines().count(), c.len())
                            }}
                        </span>
                    </div>
                    {move || (!exists.get()).then(|| view! {
                        <div class="el-banner pg-qrules-newfile">
                            "file does not exist yet — saving will create it"
                        </div>
                    })}
                    <textarea
                        class="pg-qrules-editor"
                        spellcheck="false"
                        prop:value=move || content.get()
                        on:input=move |e| content.set(event_target_value(&e))
                    ></textarea>
                </Panel>

                <div class="pg-qrules-right">
                    // ── 02 · Save state — last PUT response, verbatim ──
                    <Panel eyebrow="SAVE STATE" num="02">
                        {move || match save_state.get() {
                            None => view! {
                                <div class="pg-qrules-hint">
                                    "// no save in this session yet"
                                </div>
                            }.into_any(),
                            Some(Ok((path, bytes))) => view! {
                                <div class="pg-qrules-saved">
                                    <span class="el-tag el-tag-pos">"saved"</span>
                                    <div class="el-mono pg-qrules-save-path">{path}</div>
                                    <div class="pg-qrules-save-bytes">
                                        {format!("{bytes} bytes written")}
                                    </div>
                                </div>
                            }.into_any(),
                            Some(Err(msg)) => view! {
                                <div class="el-banner el-banner-warn pg-qrules-err">{msg}</div>
                            }.into_any(),
                        }}
                    </Panel>

                    // ── 03 · Evaluate — real quality-signal run ─────────
                    <Panel eyebrow="EVALUATE" num="03" cmd="touring quality-signal -j --rules">
                        <div class="pg-qrules-pathrow">
                            <span class="el-cmd-label pg-qrules-path">
                                {move || file_path.get()}
                            </span>
                            <button
                                class="el-btn el-btn-sm"
                                disabled=move || eval_busy.get()
                                on:click=on_evaluate
                            >
                                {move || if eval_busy.get() { "Evaluating…" } else { "Evaluate now" }}
                            </button>
                        </div>
                        {move || {
                            if eval_busy.get() {
                                return view! {
                                    <div class="el-skeleton" aria-label="evaluating"></div>
                                }.into_any();
                            }
                            match eval_state.get() {
                                None => view! {
                                    <div class="pg-qrules-hint">
                                        "// not evaluated yet — save, or press Evaluate now"
                                    </div>
                                }.into_any(),
                                Some(v) => {
                                    if let Some(err) = v.get("error").and_then(Value::as_str) {
                                        let msg = err.to_string();
                                        return view! {
                                            <div class="el-banner el-banner-warn pg-qrules-err">
                                                {format!("evaluate failed: {msg}")}
                                            </div>
                                        }.into_any();
                                    }
                                    let vios = violations_of(&v);
                                    if vios.is_empty() {
                                        let signal = v.get("signal_0_10000").and_then(Value::as_u64);
                                        let bottleneck = v
                                            .get("bottleneck")
                                            .and_then(Value::as_str)
                                            .unwrap_or("—")
                                            .to_string();
                                        view! {
                                            <div class="pg-qrules-pass">
                                                <span class="el-tag el-tag-pos">"all rules pass"</span>
                                                <div class="el-mono pg-qrules-signal">
                                                    {match signal {
                                                        Some(s) => format!(
                                                            "signal {s} / 10 000 · bottleneck {bottleneck} · no violations reported"
                                                        ),
                                                        None => "no violations reported".to_string(),
                                                    }}
                                                </div>
                                            </div>
                                        }.into_any()
                                    } else {
                                        let deny = vios.iter().filter(|x| x.severity == "deny").count();
                                        let warn = vios.iter().filter(|x| x.severity == "warn").count();
                                        view! {
                                            <div class="pg-qrules-vios">
                                                <div class="el-mono pg-qrules-vio-count">
                                                    {format!(
                                                        "{} violation(s) · {deny} deny · {warn} warn",
                                                        vios.len()
                                                    )}
                                                </div>
                                                {vios.into_iter().map(|vio| {
                                                    let card_class = match vio.severity.as_str() {
                                                        "deny" => "el-card pg-qrules-vio pg-qrules-vio-deny",
                                                        "warn" => "el-card pg-qrules-vio pg-qrules-vio-warn",
                                                        _ => "el-card pg-qrules-vio",
                                                    };
                                                    let tag_class = match vio.severity.as_str() {
                                                        "deny" => "el-tag el-tag-neg",
                                                        "warn" => "el-tag el-tag-warn",
                                                        _ => "el-tag",
                                                    };
                                                    let detail = violation_detail(&vio);
                                                    view! {
                                                        <div class=card_class>
                                                            <div class="pg-qrules-vio-head">
                                                                <span class="pg-qrules-vio-name">
                                                                    {vio.rule_name.clone()}
                                                                </span>
                                                                <span class=tag_class>
                                                                    {vio.severity.clone()}
                                                                </span>
                                                            </div>
                                                            <div class="pg-qrules-vio-msg">
                                                                {vio.message.clone()}
                                                            </div>
                                                            <div class="el-mono pg-qrules-vio-metric">
                                                                {detail}
                                                            </div>
                                                        </div>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        }.into_any()
                                    }
                                }
                            }
                        }}
                    </Panel>
                </div>
            </section>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── parse_rules_response ────────────────────────────────────────────

    #[test]
    fn parse_rules_response_real_get_shape() {
        // Verbatim shape captured 2026-06-12 from GET /api/quality/rules.
        let v = json!({
            "content": "version = \"1.0\"\n\n[[rule]]\nname = \"no-cycles\"\n",
            "exists": false,
            "path": "/home/gabrielgadea/.claude/rust/.claude/touring/quality-rules.toml"
        });
        let (exists, path, content) = parse_rules_response(&v);
        assert!(!exists);
        assert_eq!(
            path,
            "/home/gabrielgadea/.claude/rust/.claude/touring/quality-rules.toml"
        );
        assert!(content.starts_with("version = \"1.0\""));
    }

    #[test]
    fn parse_rules_response_tolerates_malformed_payload() {
        let (exists, path, content) = parse_rules_response(&json!({"error": "boom"}));
        assert!(!exists);
        assert!(path.is_empty());
        assert!(content.is_empty());
    }

    // ── parse_save_response ─────────────────────────────────────────────

    #[test]
    fn parse_save_response_real_success_shape() {
        // Verbatim shape captured 2026-06-12 from PUT /api/quality/rules.
        let v = json!({
            "bytes": 271,
            "path": "/home/gabrielgadea/.claude/rust/.claude/touring/quality-rules.toml",
            "saved": true
        });
        assert_eq!(
            parse_save_response(&v).unwrap(),
            (
                "/home/gabrielgadea/.claude/rust/.claude/touring/quality-rules.toml".to_string(),
                271
            )
        );
    }

    #[test]
    fn parse_save_response_preserves_real_parser_error() {
        // Verbatim parser message captured 2026-06-12 (invalid table header).
        let msg = "invalid TOML: TOML parse error at line 2, column 7\n  |\n2 | [[rule]\n  |       ^\ninvalid table header\nexpected `.`, `]]`\n";
        let v = json!({ "error": msg });
        assert_eq!(parse_save_response(&v).unwrap_err().to_string(), msg);
    }

    #[test]
    fn parse_save_response_rejects_unexpected_shape() {
        let res = parse_save_response(&json!({}));
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("unexpected save response")
        );
    }

    // ── violations_of ───────────────────────────────────────────────────

    #[test]
    fn violations_of_empty_on_real_signal_shape() {
        // Verbatim top-level shape captured 2026-06-12 from
        // GET /api/quality/rules/evaluate (no `violations` key today).
        let v = json!({
            "bottleneck": "acyclicity",
            "computed_at": 1781300832.4318202,
            "diagnostics": {"acyclicity": {"cycle_paths": []}},
            "raw": {"cycle_count": 11, "max_depth": 12},
            "root": "/home/gabrielgadea/.claude/rust",
            "root_causes": {"acyclicity": 0.0833},
            "signal_0_10000": 3702,
            "signal_normalized": 0.3701985182429154
        });
        assert!(violations_of(&v).is_empty());
    }

    #[test]
    fn violations_of_parses_canonical_metric_violation() {
        // Canonical MetricViolation serde shape
        // (touring-analysis/src/rules/types.rs:197, lowercase severity).
        let v = json!({
            "violations": [{
                "rule_name": "no-cycles",
                "severity": "deny",
                "metric": "cycle_count",
                "op": "eq",
                "threshold": 0.0,
                "actual": 11.0,
                "message": "dependency cycles violate the budget"
            }, {
                "rule_name": "no-god-files",
                "severity": "warn",
                "metric": "file_lines",
                "op": "lt",
                "threshold": 1000.0,
                "actual": 1024.0,
                "location": "crates/x/src/big.rs",
                "message": "files should be < 1000 LOC"
            }]
        });
        let vios = violations_of(&v);
        assert_eq!(vios.len(), 2);
        assert_eq!(vios[0].rule_name, "no-cycles");
        assert_eq!(vios[0].severity, "deny");
        assert_eq!(vios[0].metric, "cycle_count");
        assert_eq!(vios[0].op, "eq");
        assert_eq!(vios[0].threshold, Some(0.0));
        assert_eq!(vios[0].actual, Some(11.0));
        assert_eq!(vios[0].location, None);
        assert_eq!(vios[1].location.as_deref(), Some("crates/x/src/big.rs"));
    }

    #[test]
    fn violations_of_skips_entries_without_rule_name() {
        let v = json!({
            "violations": [
                {"severity": "deny", "metric": "cycle_count"},
                {"rule_name": "kept", "severity": "info", "metric": "max_depth",
                 "op": "lte", "threshold": 10.0, "actual": 12.0, "message": "m"}
            ]
        });
        let vios = violations_of(&v);
        assert_eq!(vios.len(), 1);
        assert_eq!(vios[0].rule_name, "kept");
    }

    // ── violation_detail ────────────────────────────────────────────────

    #[test]
    fn violation_detail_formats_metric_line() {
        let vio = RuleViolation {
            rule_name: "no-cycles".into(),
            severity: "deny".into(),
            metric: "cycle_count".into(),
            op: "eq".into(),
            threshold: Some(0.0),
            actual: Some(11.0),
            location: None,
            message: "m".into(),
        };
        assert_eq!(violation_detail(&vio), "cycle_count eq 0 · got 11");

        let scoped = RuleViolation {
            location: Some("crates/x/src/big.rs".into()),
            ..vio
        };
        assert_eq!(
            violation_detail(&scoped),
            "cycle_count eq 0 · got 11 · crates/x/src/big.rs"
        );
    }
}
