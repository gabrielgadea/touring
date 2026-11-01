//! B5 (01/11/2026) — what the context window costs in TOKENS, with the cache accounted.
//!
//! `context_budget` measures BYTES injected per hook family. Bytes are not cost: the prefix is
//! cached and a cache read bills a fraction of a fresh token, so a judgement made in bytes can
//! optimise what is already cheap. This section reads the same transcripts for the number the
//! provider actually billed — the `usage` of every assistant call — and attributes it.
//!
//! # What is measured and what is estimated
//!
//! * **Measured**: `input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens` and
//!   `output_tokens` per API call (deduplicated by message id: Claude Code writes one assistant
//!   record per content block and every record repeats the usage).
//! * **Estimated**: the split of those input tokens. Every call re-sends the whole window, so a
//!   byte that entered the window at call *j* is paid again at every call after it. The window is
//!   rebuilt from the transcript (reset at a `compact_boundary`), each category's bytes are
//!   converted with ONE declared constant ([`TOKENS_PER_BYTE`]), and summed over the calls that
//!   carried them. The categories are (a) injected hook families (the classifier is
//!   `context_budget`'s), (b) the announcement surface, (c) tool results, (d) the rest — user
//!   text, assistant output, other attachments and the **unseen prefix**: what the call billed
//!   beyond every byte the transcript can show (system prompt, tool schemas).
//! * **Dollars**: only from [`PRICES`], keyed by exact model id. A model that is not in the table
//!   (or a non-standard speed) makes the dollar figure `null` and is listed in
//!   `unpriced_models` — never guessed (TokenEconomizer `pricing.py:135-160` hard-fails the same
//!   way).
//!
//! # Reconciliation
//!
//! The unseen prefix is the per-call residual, so the attributed tokens equal the billed ones by
//! construction EXCEPT where the estimate exceeds the bill. That over-attribution is the only way
//! the identity can fail, and it is what `reconciliation_error` reports
//! (`Σ max(0, estimated − billed) / Σ billed`, acceptance ≤ 2%). The unseen share is shown beside
//! it, because a residual that swallows most of the bill is a ruler with little to say.
//!
//! `cost-state`, Claude Code's own running total, is an independent measure: it also holds the
//! subagents and side calls this transcript does not, so the transcript's share of it is the
//! honest coverage — and it validates the price table where it reaches 1.
//!
//! Nothing here keeps transcript text: the payload carries counts, names and numbers.

use crate::cli::context_budget::{attributed_family, hook_injections, transcript_dir_for};
use crate::cli::surface_usage::{SurfaceTracker, surface_report};
use crate::cli::transcript_scan::{
    KPI_CAPS, ScanCaps, is_sidechain, parse_announcement, scan_dir,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Bytes per token the estimate assumes — the common rule of thumb, declared rather than hidden.
/// It is a LOWER bound on tokens for code and non-English text (those tokenise denser), which is
/// the conservative direction for a "how much does this cost" question: the real figure is at
/// least this. The residual (`unseen_prefix`) absorbs whatever the constant under-counts.
pub const BYTES_PER_TOKEN: u64 = 4;

/// Tokens per byte — derived from [`BYTES_PER_TOKEN`] so the two cannot disagree.
pub const TOKENS_PER_BYTE: f64 = 1.0 / BYTES_PER_TOKEN as f64;

/// The acceptance bar for `reconciliation_error` (backlog B5).
pub const RECONCILIATION_TOLERANCE: f64 = 0.02;

/// One model's price, in USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPrice {
    /// The exact model id the transcript records (no `[1m]` suffix, no date).
    pub model: &'static str,
    /// Uncached input.
    pub input: f64,
    /// Output (thinking included).
    pub output: f64,
    /// Cache read.
    pub cache_read: f64,
    /// Cache write, 5-minute TTL.
    pub cache_write_5m: f64,
    /// Cache write, 1-hour TTL.
    pub cache_write_1h: f64,
}

/// The price table, USD per million tokens, keyed by exact model id. A model becomes priced HERE
/// and nowhere else; every row carries all five rates or it does not exist.
///
/// Source: the claude-api reference's model table (cached 06/10/2026) for input, output and the
/// cache-read rate of Opus 5.5 and Sonnet 5.5; cache writes at Anthropic's published
/// multipliers of the input rate (5 min 1.25×, 1 h 2×). The table is checked against
/// `cost-state`'s own `costUSD` per model in every payload (`cost_state.by_model.*.usd_ratio`),
/// so a stale rate shows as a ratio that does not reach 1 where the transcript is the whole
/// session. Models whose cache-read rate is not documented (Haiku 5.5) are left OUT on purpose.
pub const PRICES: &[ModelPrice] = &[
    ModelPrice {
        model: "claude-opus-5-5",
        input: 4.0,
        output: 20.0,
        cache_read: 0.20,
        cache_write_5m: 5.0,
        cache_write_1h: 8.0,
    },
    ModelPrice {
        model: "claude-sonnet-5-5",
        input: 2.0,
        output: 10.0,
        cache_read: 0.20,
        cache_write_5m: 2.5,
        cache_write_1h: 4.0,
    },
];

/// The price of `model`, or `None` — exact id only.
#[must_use]
pub fn price_for(model: &str) -> Option<&'static ModelPrice> {
    PRICES.iter().find(|p| p.model == model)
}

/// Token counts by kind, as billed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct TokenTotals {
    input: u64,
    cache_creation: u64,
    cache_read: u64,
    output: u64,
}

impl TokenTotals {
    fn input_total(&self) -> u64 {
        self.input + self.cache_creation + self.cache_read
    }

    fn add(&mut self, o: &Self) {
        self.input += o.input;
        self.cache_creation += o.cache_creation;
        self.cache_read += o.cache_read;
        self.output += o.output;
    }

    fn json(&self) -> Value {
        json!({
            "input": self.input,
            "cache_creation": self.cache_creation,
            "cache_read": self.cache_read,
            "output": self.output,
            "input_total": self.input_total(),
            "method": "measured: assistant `usage`, one count per message id",
        })
    }
}

/// One API call's usage block.
struct CallUsage {
    tokens: TokenTotals,
    /// Of `tokens.cache_creation`, the 1-hour-TTL part.
    cache_creation_1h: u64,
    speed: Option<String>,
}

impl CallUsage {
    fn from_message(msg: &Value) -> Self {
        let u = msg.get("usage").unwrap_or(&Value::Null);
        let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
        Self {
            tokens: TokenTotals {
                input: n("input_tokens"),
                cache_creation: n("cache_creation_input_tokens"),
                cache_read: n("cache_read_input_tokens"),
                output: n("output_tokens"),
            },
            cache_creation_1h: u
                .pointer("/cache_creation/ephemeral_1h_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            speed: u.get("speed").and_then(Value::as_str).map(str::to_string),
        }
    }

    /// The label a call is billed under: the model, plus the speed when it is not `standard`.
    fn billing_label(&self, model: &str) -> String {
        match self.speed.as_deref() {
            Some(s) if s != "standard" => format!("{model}@{s}"),
            _ => model.to_string(),
        }
    }

    /// `(input side, output side)` in USD at `p`'s rates.
    fn usd(&self, p: &ModelPrice) -> (f64, f64) {
        let t = &self.tokens;
        let one_hour = self.cache_creation_1h.min(t.cache_creation);
        let five_min = t.cache_creation - one_hour;
        let per_m = 1_000_000.0;
        let input = (t.input as f64 * p.input
            + five_min as f64 * p.cache_write_5m
            + one_hour as f64 * p.cache_write_1h
            + t.cache_read as f64 * p.cache_read)
            / per_m;
        (input, t.output as f64 * p.output / per_m)
    }
}

/// Per billing label: calls, tokens and dollars (priced calls only).
#[derive(Debug, Default, Clone, PartialEq)]
struct ModelAcc {
    calls: u64,
    tokens: TokenTotals,
    usd: f64,
}

/// One attribution category: estimated tokens and the dollars of the input side they carry.
#[derive(Debug, Default, Clone, PartialEq)]
struct CatAcc {
    tokens: f64,
    usd: f64,
}

/// What `cost-state` says about one model (its key without a `[1m]`-style suffix).
#[derive(Debug, Default, Clone, PartialEq)]
struct CostStateModel {
    input_total: u64,
    output: u64,
    cost_usd: f64,
}

/// Claude Code's own running total for the session — the independent check.
#[derive(Debug, Default, Clone, PartialEq)]
struct CostState {
    total_cost_usd: Option<f64>,
    by_model: BTreeMap<String, CostStateModel>,
}

impl CostState {
    fn from_record(rec: &Value) -> Self {
        let mut by_model: BTreeMap<String, CostStateModel> = BTreeMap::new();
        let usage = rec.get("modelUsage").and_then(Value::as_object);
        for (key, m) in usage.into_iter().flatten() {
            let n = |k: &str| m.get(k).and_then(Value::as_u64).unwrap_or(0);
            let e = by_model.entry(strip_context_suffix(key).to_string()).or_default();
            e.input_total +=
                n("inputTokens") + n("cacheReadInputTokens") + n("cacheCreationInputTokens");
            e.output += n("outputTokens");
            e.cost_usd += m.get("costUSD").and_then(Value::as_f64).unwrap_or(0.0);
        }
        Self {
            total_cost_usd: rec.get("totalCostUSD").and_then(Value::as_f64),
            by_model,
        }
    }
}

/// `claude-opus-5-5[1m]` → `claude-opus-5-5`.
fn strip_context_suffix(key: &str) -> &str {
    key.split_once('[').map_or(key, |(head, _)| head)
}

/// The accumulators of one session — or, absorbed, of the project.
#[derive(Debug, Default, Clone)]
pub struct CostSummary {
    calls: u64,
    tokens: TokenTotals,
    by_model: BTreeMap<String, ModelAcc>,
    unpriced_models: BTreeSet<String>,
    priced_calls: u64,
    unpriced_calls: u64,
    usd_priced: f64,
    cats: BTreeMap<String, CatAcc>,
    overshoot_tokens: f64,
    compactions: u64,
    sidechain_skipped: u64,
    cost_state: Option<CostState>,
}

impl CostSummary {
    /// Add one session's summary to this PROJECT summary. `cost-state` is per session and is not
    /// summed: a running total summed over sessions would double-count a resumed one.
    pub fn absorb(&mut self, s: &Self) {
        self.calls += s.calls;
        self.tokens.add(&s.tokens);
        for (label, m) in &s.by_model {
            let e = self.by_model.entry(label.clone()).or_default();
            e.calls += m.calls;
            e.tokens.add(&m.tokens);
            e.usd += m.usd;
        }
        self.unpriced_models.extend(s.unpriced_models.iter().cloned());
        self.priced_calls += s.priced_calls;
        self.unpriced_calls += s.unpriced_calls;
        self.usd_priced += s.usd_priced;
        for (leaf, c) in &s.cats {
            let e = self.cats.entry(leaf.clone()).or_default();
            e.tokens += c.tokens;
            e.usd += c.usd;
        }
        self.overshoot_tokens += s.overshoot_tokens;
        self.compactions += s.compactions;
        self.sidechain_skipped += s.sidechain_skipped;
    }
}

const LEAF_UNSEEN: &str = "unseen_prefix";
const LEAF_TOOL_RESULTS: &str = "tool_results";
const LEAF_USER: &str = "user_text";
const LEAF_ASSISTANT: &str = "assistant_output";
const LEAF_OTHER: &str = "other_attachments";

/// Walks a session's records, keeping the window and the running totals.
#[derive(Debug, Default)]
pub struct CostTracker {
    /// Bytes now in the window, per leaf category.
    window: BTreeMap<String, u64>,
    /// Message ids already billed.
    seen: BTreeSet<String>,
    summary: CostSummary,
}

/// Bytes of a tool result: a string as is, a list of text blocks by their text, anything else
/// by its serialisation.
fn result_bytes(content: &Value) -> u64 {
    match content {
        Value::String(s) => s.len() as u64,
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match b.get("text").and_then(Value::as_str) {
                Some(t) => t.len() as u64,
                None => b.to_string().len() as u64,
            })
            .sum(),
        other => other.to_string().len() as u64,
    }
}

/// Bytes of the text an attachment carries that is not an announcement or a hook injection.
fn other_attachment_bytes(att: &Value) -> u64 {
    ["content", "prompt"]
        .iter()
        .filter_map(|k| att.get(*k))
        .map(|v| match v {
            Value::Null => 0,
            Value::String(s) => s.len() as u64,
            other => other.to_string().len() as u64,
        })
        .sum()
}

/// Bytes of the blocks an assistant record adds to the window.
fn assistant_bytes(msg: &Value) -> u64 {
    let Some(content) = msg.get("content") else {
        return 0;
    };
    if let Some(s) = content.as_str() {
        return s.len() as u64;
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .map(|b| match b.get("type").and_then(Value::as_str) {
            Some("text") => b.get("text").and_then(Value::as_str).map_or(0, str::len) as u64,
            Some("thinking") => b.get("thinking").and_then(Value::as_str).map_or(0, str::len) as u64,
            Some("tool_use") => {
                b.get("input").map_or(0, |i| i.to_string().len()) as u64
                    + b.get("name").and_then(Value::as_str).map_or(0, str::len) as u64
            }
            _ => 0,
        })
        .sum()
}

impl CostTracker {
    fn add_bytes(&mut self, leaf: &str, bytes: u64) {
        if bytes > 0 {
            *self.window.entry(leaf.to_string()).or_default() += bytes;
        }
    }

    /// Fold one transcript record into the window and the totals.
    pub fn observe(&mut self, rec: &Value) {
        if is_sidechain(rec) {
            self.summary.sidechain_skipped += 1;
            return;
        }
        match rec.get("type").and_then(Value::as_str) {
            Some("system") => self.observe_system(rec),
            Some("attachment") => self.observe_attachment(rec),
            Some("user") => self.observe_user(rec),
            Some("assistant") => self.observe_assistant(rec),
            Some("cost-state") => self.summary.cost_state = Some(CostState::from_record(rec)),
            _ => {}
        }
    }

    /// [`observe`](Self::observe) over raw JSONL lines; a line that is not JSON is skipped.
    #[cfg(test)]
    pub fn observe_lines<'a>(&mut self, lines: impl Iterator<Item = &'a str>) {
        for line in lines {
            if let Ok(rec) = serde_json::from_str::<Value>(line) {
                self.observe(&rec);
            }
        }
    }

    fn observe_system(&mut self, rec: &Value) {
        if rec.get("subtype").and_then(Value::as_str) == Some("compact_boundary") {
            self.window.clear();
            self.summary.compactions += 1;
        }
    }

    fn observe_attachment(&mut self, rec: &Value) {
        let Some(att) = rec.get("attachment") else {
            return;
        };
        if att.get("type").and_then(Value::as_str) == Some("hook_success") {
            for inj in hook_injections(rec) {
                let family = attributed_family(&inj.text, inj.emitter.as_deref());
                self.add_bytes(&format!("hook/{family}"), inj.text.len() as u64);
            }
        } else if let Some(a) = parse_announcement(att) {
            self.add_bytes(&format!("announce/{}", a.kind.as_str()), a.total_bytes);
        } else {
            self.add_bytes(LEAF_OTHER, other_attachment_bytes(att));
        }
    }

    fn observe_user(&mut self, rec: &Value) {
        let Some(content) = rec.pointer("/message/content") else {
            return;
        };
        if let Some(text) = content.as_str() {
            self.add_bytes(LEAF_USER, text.len() as u64);
            return;
        }
        for b in content.as_array().into_iter().flatten() {
            match b.get("type").and_then(Value::as_str) {
                Some("tool_result") => {
                    self.add_bytes(LEAF_TOOL_RESULTS, b.get("content").map_or(0, result_bytes));
                }
                Some("text") => {
                    let text = b.get("text").and_then(Value::as_str).unwrap_or("");
                    self.add_bytes(LEAF_USER, text.len() as u64);
                }
                _ => {}
            }
        }
    }

    fn observe_assistant(&mut self, rec: &Value) {
        let Some(msg) = rec.get("message") else {
            return;
        };
        if let Some(id) = msg.get("id").and_then(Value::as_str)
            && self.seen.insert(id.to_string())
        {
            self.bill_call(msg);
        }
        // What the assistant wrote enters the window AFTER the call that wrote it.
        self.add_bytes(LEAF_ASSISTANT, assistant_bytes(msg));
    }

    /// Count one API call and attribute the input it was billed for.
    fn bill_call(&mut self, msg: &Value) {
        let model = msg.get("model").and_then(Value::as_str).unwrap_or("unknown");
        let usage = CallUsage::from_message(msg);
        if model == "<synthetic>" || usage.tokens.input_total() == 0 {
            return;
        }
        let label = usage.billing_label(model);
        let usd = price_for(&label).map(|p| usage.usd(p));
        let s = &mut self.summary;
        s.calls += 1;
        s.tokens.add(&usage.tokens);
        let acc = s.by_model.entry(label.clone()).or_default();
        acc.calls += 1;
        acc.tokens.add(&usage.tokens);
        match usd {
            Some((input, output)) => {
                acc.usd += input + output;
                s.usd_priced += input + output;
                s.priced_calls += 1;
            }
            None => {
                s.unpriced_models.insert(label);
                s.unpriced_calls += 1;
            }
        }
        self.attribute_window(usage.tokens.input_total(), usd.map(|(input, _)| input));
    }

    /// Spread one call's billed input over the window it re-sent: each leaf at its estimated
    /// tokens, the remainder to `unseen_prefix`; an estimate above the bill is the overshoot.
    fn attribute_window(&mut self, billed: u64, input_usd: Option<f64>) {
        let billed = billed as f64;
        let visible = self.window.values().sum::<u64>() as f64 * TOKENS_PER_BYTE;
        let unseen = (billed - visible).max(0.0);
        let denominator = billed.max(visible);
        self.summary.overshoot_tokens += (visible - billed).max(0.0);
        let leaves = self
            .window
            .iter()
            .map(|(leaf, bytes)| (leaf.as_str(), *bytes as f64 * TOKENS_PER_BYTE))
            .chain(std::iter::once((LEAF_UNSEEN, unseen)));
        for (leaf, tokens) in leaves {
            let cat = self.summary.cats.entry(leaf.to_string()).or_default();
            cat.tokens += tokens;
            if let Some(usd) = input_usd {
                cat.usd += usd * tokens / denominator;
            }
        }
    }

    /// The session's accumulators.
    #[must_use]
    pub fn finish(self) -> CostSummary {
        self.summary
    }
}

/// `v` rounded to `places` decimals — shared with `surface_usage`.
pub(crate) fn round_to(v: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (v * k).round() / k
}

fn ratio(num: f64, den: f64) -> Value {
    if den > 0.0 {
        json!(round_to(num / den, 6))
    } else {
        Value::Null
    }
}

/// The leaves under `prefix/`, as `(sub-key, acc)`.
fn leaves_under<'a>(
    cats: &'a BTreeMap<String, CatAcc>,
    prefix: &str,
) -> Vec<(&'a str, &'a CatAcc)> {
    let head = format!("{prefix}/");
    cats.iter()
        .filter_map(|(k, c)| k.strip_prefix(head.as_str()).map(|sub| (sub, c)))
        .collect()
}

fn cat_of(cats: &BTreeMap<String, CatAcc>, leaf: &str) -> CatAcc {
    cats.get(leaf).cloned().unwrap_or_default()
}

fn bucket_json(tokens: f64, usd: f64, attributed: f64, method: &str) -> Value {
    json!({
        "tokens": round_to(tokens, 2),
        "share": ratio(tokens, attributed),
        "usd_priced_calls": round_to(usd, 6),
        "method": method,
    })
}

fn by_sub_json(subs: &[(&str, &CatAcc)]) -> Value {
    Value::Object(
        subs.iter()
            .map(|(k, c)| {
                (
                    (*k).to_string(),
                    json!({"tokens": round_to(c.tokens, 2), "usd_priced_calls": round_to(c.usd, 6)}),
                )
            })
            .collect(),
    )
}

const ESTIMATE: &str = "estimated: window bytes × tokens_per_byte, summed over the calls that re-sent them";

fn attribution_json(s: &CostSummary) -> Value {
    let attributed: f64 = s.cats.values().map(|c| c.tokens).sum();
    let sum = |subs: &[(&str, &CatAcc)]| {
        (
            subs.iter().map(|(_, c)| c.tokens).sum::<f64>(),
            subs.iter().map(|(_, c)| c.usd).sum::<f64>(),
        )
    };
    let hooks = leaves_under(&s.cats, "hook");
    let ann = leaves_under(&s.cats, "announce");
    let (hook_t, hook_u) = sum(&hooks);
    let (ann_t, ann_u) = sum(&ann);
    let tool = cat_of(&s.cats, LEAF_TOOL_RESULTS);
    let parts: Vec<(&str, CatAcc)> = [LEAF_USER, LEAF_ASSISTANT, LEAF_OTHER, LEAF_UNSEEN]
        .iter()
        .map(|l| (*l, cat_of(&s.cats, l)))
        .collect();
    let rest_t: f64 = parts.iter().map(|(_, c)| c.tokens).sum();
    let rest_u: f64 = parts.iter().map(|(_, c)| c.usd).sum();

    let mut hooks_out = bucket_json(hook_t, hook_u, attributed, ESTIMATE);
    hooks_out["by_family"] = by_sub_json(&hooks);
    let mut ann_out = bucket_json(ann_t, ann_u, attributed, ESTIMATE);
    ann_out["by_kind"] = by_sub_json(&ann);
    let mut rest_out = bucket_json(
        rest_t,
        rest_u,
        attributed,
        "estimated for user/assistant/attachments; `unseen_prefix` is the RESIDUAL: billed input minus every estimated byte (system prompt, tool schemas)",
    );
    rest_out["parts"] = Value::Object(
        parts
            .iter()
            .map(|(k, c)| ((*k).to_string(), json!(round_to(c.tokens, 2))))
            .collect(),
    );
    json!({
        "injected_hooks": hooks_out,
        "announcement": ann_out,
        "tool_results": bucket_json(tool.tokens, tool.usd, attributed, ESTIMATE),
        "rest": rest_out,
        "attributed_tokens": round_to(attributed, 2),
        "tokens_per_byte": TOKENS_PER_BYTE,
    })
}

fn reconciliation_json(s: &CostSummary) -> (Value, Value) {
    let measured = s.tokens.input_total() as f64;
    let attributed: f64 = s.cats.values().map(|c| c.tokens).sum();
    let unseen = cat_of(&s.cats, LEAF_UNSEEN).tokens;
    let error = ratio(s.overshoot_tokens, measured);
    let within = error.as_f64().map(|e| e <= RECONCILIATION_TOLERANCE);
    let detail = json!({
        "measured_input_tokens": s.tokens.input_total(),
        "attributed_tokens": round_to(attributed, 2),
        "unseen_tokens": round_to(unseen, 2),
        "unseen_share": ratio(unseen, measured),
        "overshoot_tokens": round_to(s.overshoot_tokens, 2),
        "error": error,
        "tolerance": RECONCILIATION_TOLERANCE,
        "within_tolerance": within,
        "method": "error = Σ max(0, estimated − billed) per call ÷ Σ billed; the residual is the unseen prefix, so under-attribution is shown as `unseen_share`, not hidden",
    });
    (detail, error)
}

fn cost_json(s: &CostSummary) -> Value {
    let by_model: serde_json::Map<String, Value> = s
        .by_model
        .iter()
        .map(|(label, m)| {
            let priced = price_for(label).is_some();
            (
                label.clone(),
                json!({
                    "calls": m.calls,
                    "input_total_tokens": m.tokens.input_total(),
                    "output_tokens": m.tokens.output,
                    "priced": priced,
                    "usd": if priced { json!(round_to(m.usd, 6)) } else { Value::Null },
                }),
            )
        })
        .collect();
    let usd = if s.calls > 0 && s.unpriced_calls == 0 {
        json!(round_to(s.usd_priced, 6))
    } else {
        Value::Null
    };
    json!({
        "usd": usd,
        "usd_priced_calls": round_to(s.usd_priced, 6),
        "priced_calls": s.priced_calls,
        "unpriced_calls": s.unpriced_calls,
        "unpriced_models": s.unpriced_models,
        "by_model": by_model,
        "method": "measured tokens × PRICES (exact model id); `usd` is null while any call is unpriced",
    })
}

fn cost_state_json(s: &CostSummary) -> Value {
    let Some(cs) = &s.cost_state else {
        return Value::Null;
    };
    let by_model: serde_json::Map<String, Value> = cs
        .by_model
        .iter()
        .map(|(model, m)| {
            let ours = s.by_model.get(model);
            let usd_ratio = ours
                .filter(|_| price_for(model).is_some())
                .map_or(Value::Null, |o| ratio(o.usd, m.cost_usd));
            (
                model.clone(),
                json!({
                    "input_total_tokens": m.input_total,
                    "output_tokens": m.output,
                    "cost_usd": round_to(m.cost_usd, 6),
                    "input_total_ratio": ours.map_or(Value::Null, |o| ratio(o.tokens.input_total() as f64, m.input_total as f64)),
                    "usd_ratio": usd_ratio,
                }),
            )
        })
        .collect();
    // The per-model ratio needs the transcript and `cost-state` to name a model alike. A session
    // routed through another backend labels its calls with the backend's name while `cost-state`
    // books the tokens under the alias that was asked for — measured on this project's sister
    // sessions — so the all-model ratio is the coverage that survives a label mismatch.
    let cs_input: u64 = cs.by_model.values().map(|m| m.input_total).sum();
    json!({
        "total_cost_usd": cs.total_cost_usd,
        "input_total_tokens": cs_input,
        "input_total_ratio": ratio(s.tokens.input_total() as f64, cs_input as f64),
        "by_model": by_model,
        "method": "Claude Code's own running total; it also holds subagents and side calls, so a ratio below 1 is coverage, and a ratio that misses 1 where the transcript is the whole session means a stale price",
    })
}

/// The `context_cost` payload of one summary (a session, or the project).
#[must_use]
pub fn cost_payload(s: &CostSummary) -> Value {
    let (reconciliation, error) = reconciliation_json(s);
    json!({
        "calls": s.calls,
        "tokens": s.tokens.json(),
        "attribution": attribution_json(s),
        "reconciliation": reconciliation,
        "reconciliation_error": error,
        "cost": cost_json(s),
        "compactions": s.compactions,
        "sidechain_records_skipped": s.sidechain_skipped,
        "cost_state": cost_state_json(s),
    })
}

fn constants_json() -> Value {
    let table: Vec<Value> = PRICES
        .iter()
        .map(|p| {
            json!({
                "model": p.model, "usd_per_mtok": {
                    "input": p.input, "output": p.output, "cache_read": p.cache_read,
                    "cache_write_5m": p.cache_write_5m, "cache_write_1h": p.cache_write_1h}
            })
        })
        .collect();
    json!({
        "bytes_per_token": BYTES_PER_TOKEN,
        "tokens_per_byte": TOKENS_PER_BYTE,
        "reconciliation_tolerance": RECONCILIATION_TOLERANCE,
        "price_table": table,
        "method": "bytes → tokens with ONE declared constant (an estimate, a lower bound for code and non-English text); tokens and dollars of the bill are measured",
    })
}

/// What the walk covered, shared by both sections.
fn scan_meta(sessions: usize, bytes: u64, capped: bool, skipped: usize, malformed: u64) -> Value {
    json!({
        "available": true,
        "unit": "project",
        "sessions_scanned": sessions,
        "bytes_scanned": bytes,
        "capped": capped,
        "skipped_by_cap": skipped,
        "malformed_lines": malformed,
    })
}

fn merge(into: &mut Value, extra: Value) {
    if let (Some(dst), Value::Object(src)) = (into.as_object_mut(), extra) {
        dst.extend(src);
    }
}

/// The `context_cost` and `surface_usage` sections for `project_root`, under the KPI caps.
/// One walk serves both; absence is displayed in each (`available: false` and why).
#[must_use]
pub fn context_cost_and_surface(project_root: &Path) -> (Value, Value) {
    let Some(home) = std::env::var_os("HOME") else {
        let absent = json!({"available": false, "reason": "HOME unset"});
        return (absent.clone(), absent);
    };
    context_cost_and_surface_in(Path::new(&home), project_root, &KPI_CAPS)
}

/// [`context_cost_and_surface`] against an explicit `home` and `caps` — the testable body.
#[must_use]
pub fn context_cost_and_surface_in(
    home: &Path,
    project_root: &Path,
    caps: &ScanCaps,
) -> (Value, Value) {
    let dir = transcript_dir_for(home, project_root);
    let scan = scan_dir::<(CostTracker, SurfaceTracker)>(&dir, caps, &mut |state, rec| {
        state.0.observe(rec);
        state.1.observe(rec);
    });
    let scan = match scan {
        Ok(scan) => scan,
        Err(reason) => {
            let absent = json!({"available": false, "reason": reason});
            return (absent.clone(), absent);
        }
    };
    let malformed: u64 = scan.sessions.iter().map(|s| s.malformed_lines).sum();
    let meta = scan_meta(
        scan.sessions.len(),
        scan.bytes_scanned,
        scan.capped,
        scan.skipped_by_cap,
        malformed,
    );
    let mut project = CostSummary::default();
    let mut rows = Vec::new();
    let mut surfaces = Vec::new();
    for s in scan.sessions {
        let (cost, surface) = s.state;
        let summary = cost.finish();
        project.absorb(&summary);
        let mut row = cost_payload(&summary);
        merge(
            &mut row,
            json!({"session_id": s.id, "unit": "session", "bytes_scanned": s.bytes}),
        );
        rows.push(row);
        surfaces.push(surface);
    }
    let mut cost = cost_payload(&project);
    if let Some(o) = cost.as_object_mut() {
        o.remove("cost_state");
    }
    merge(&mut cost, meta.clone());
    merge(&mut cost, json!({"constants": constants_json(), "by_session": rows}));
    let mut surface = surface_report(&surfaces.iter().collect::<Vec<_>>());
    merge(&mut surface, meta);
    (cost, surface)
}

#[cfg(test)]
#[path = "context_cost_tests.rs"]
mod tests;
