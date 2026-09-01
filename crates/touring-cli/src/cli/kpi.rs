//! `cli_kpi` — Wave R2: Falsifiable Commitments Dashboard.
//!
//! Reads `~/projects/touring/docs/kpi/commitments.yaml` (versioned source of
//! truth — the canonical workspace; `~/.claude/rust` is the FROZEN tree),
//! checks each commitment against its declared threshold/direction, and
//! returns a structured snapshot consumable by Gabriel or by CI gates.
//!
//! # Source parsing
//!
//! Each commitment declares a `source` in one of two forms:
//! - `daemon:<handler-name>:<json-pointer>` — invoke an in-process handler
//!   and extract the value via RFC-6901 JSON pointer (e.g. `/orphan_count`).
//! - `derived:<name>` — a value computed from already-collected data with no
//!   new instrumentation (`health_delta_net`, `world_model_success`). Powers
//!   the `touring.coupling.*` effectiveness family (F1 telemetry).
//! - `external:<id>` — resolved from `docs/kpi/external/<id>.json`, the file a
//!   subprocess gate (cargo nextest, llvm-cov, a peer session) writes. Since
//!   28/08/2026 these resolve REAL values; a missing/stale/value-less file
//!   returns `status: "STUB"` with a `stub_reason` naming the cause and the
//!   remedy (`ExternalStub` — Missing/Stale/Declared/Malformed).
//!
//! # CLI flags (handled by `touring-server::cli::kpi`)
//!
//! - `-j` / no flag: JSON dashboard
//! - `--check`: signal failure when any commitment fails (CLI exits non-zero)
//! - `--snapshot`: persist to `docs/kpi/YYYY-MM/YYYY-MM-DD.json`
//!
//! # Output schema
//!
//! ```json
//! {
//!   "schema": "kpi-commitments-v1",
//!   "snapshot_date": "2026-04-25",
//!   "checks": [
//!     {"id": "touring.wiring.orphans", "actual": 9106, "threshold": 100,
//!      "direction": "lte", "status": "FAIL"}
//!   ],
//!   "summary": {"total": 8, "passed": 4, "failed": 3, "stub": 2, "advisory": 0, "regressions": 0}
//! }
//! ```

use crate::runtime::HookRuntime;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

// ─────────────────────────────────────────────────────────────────────────────
// Public types — kept minimal so the YAML schema can grow without breaking.
// ─────────────────────────────────────────────────────────────────────────────

/// One commitment row, deserialised from `commitments.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Commitment {
    /// Stable identifier (e.g. `touring.wiring.orphans`).
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// Numeric target — compared via `direction`.
    pub threshold: f64,
    /// Comparison direction: `gte`, `lte`, or `eq`.
    pub direction: String,
    /// Source spec: `daemon:<handler>:<pointer>` or `external:<command>`.
    pub source: String,
    /// Human rationale (echoed for context, not checked).
    #[serde(default)]
    pub rationale: String,
    /// When true, a missed threshold is reported as `ADVISORY` (not `FAIL`)
    /// and excluded from the `--check` exit-code gate — for KPIs under
    /// calibration (the `touring.coupling.*` family, 2-week advisory window).
    #[serde(default)]
    pub advisory: bool,
}

/// Top-level YAML structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitmentsFile {
    /// Version string of the commitments file format.
    pub version: String,
    /// Schema identifier the file conforms to.
    pub schema: String,
    /// The KPI commitments declared in this file.
    pub commitments: Vec<Commitment>,
}

/// Per-commitment check result.
#[derive(Debug, Clone, Serialize)]
pub struct CommitmentCheck {
    /// Identifier of the commitment being checked.
    pub id: String,
    /// Human-readable name of the commitment.
    pub name: String,
    /// Target threshold the actual value is compared against.
    pub threshold: f64,
    /// Comparison direction (whether the actual must be above or below the threshold).
    pub direction: String,
    /// Source metric kind the actual value was read from.
    pub source_kind: &'static str,
    /// Observed value, or `None` when the metric could not be resolved.
    pub actual: Option<f64>,
    /// Pass/fail/unknown outcome of the threshold comparison.
    pub status: &'static str,
    /// Human rationale echoed from the commitment for context.
    pub rationale: String,
    /// Whether this commitment is advisory (missed threshold → `ADVISORY`,
    /// excluded from the `--check` failure gate).
    pub advisory: bool,
    /// Why an `external:` STUB has no value — never-measured, stale, or
    /// malformed, each named with its remedy. `None` for non-external sources
    /// or when a value resolved: "measured and failed" (FAIL) must never be
    /// confusable with "nobody measured" (a bare STUB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_reason: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public handler
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-kpi` handler — produces the falsifiable commitments dashboard.
///
/// Payload optional fields:
/// - `"check": true` → set `summary.check_failed` flag when any commitment FAIL
/// - `"snapshot": true` → persist to `docs/kpi/YYYY-MM/YYYY-MM-DD.json`
/// - `"yaml_path": "/abs/path"` → override default commitments file (testing)
pub fn cli_kpi(rt: &mut HookRuntime, payload: &Value) -> String {
    let yaml_override = payload.get("yaml_path").and_then(Value::as_str);
    let snapshot = payload
        .get("snapshot")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let check = payload
        .get("check")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let yaml_path = yaml_override
        .map(PathBuf::from)
        .unwrap_or_else(default_commitments_path);
    let file = match load_commitments(&yaml_path) {
        Ok(f) => f,
        Err(e) => {
            return json!({
                "error": format!("failed to load commitments.yaml: {e}"),
                "yaml_path": yaml_path.display().to_string(),
            })
            .to_string();
        }
    };
    // Measurements for `external:` sources live next to the contract itself
    // (`<commitments-dir>/external/<id>.json` — versionable, dated, auditable);
    // see `resolve_external`.
    let external_dir = yaml_path
        .parent()
        .map(|p| p.join("external"))
        .unwrap_or_else(|| PathBuf::from("external"));
    let checks: Vec<CommitmentCheck> = file
        .commitments
        .iter()
        .map(|c| check_one(rt, c, &external_dir))
        .collect();
    let summary = summarize(&checks);
    let snapshot_date = today_iso();
    // Investigation 2026-07-01: several sources resolve per-project (orphans,
    // ema_reward), so the SAME commitment reports different numbers depending
    // on the caller's cwd. Label every output with the project it measured so
    // readers (and the datated series) can tell which project a number is from.
    let mut out = json!({
        "schema": file.schema,
        "snapshot_date": snapshot_date,
        "project_root": rt.project_root.display().to_string(),
        "checks": checks,
        "summary": summary,
    });

    // F6 (telemetry §10/§11.1) — attach the latest A/B causal-attribution block
    // from `run_bench.py --compare`; `null` when no A/B has run (honest absence).
    out["ab"] = build_ab_block(&default_ab_path()).unwrap_or(Value::Null);

    // F7 (telemetry §12) — when requested, run the refinement engine over the live
    // coupling signals + the A/B gate, surfacing the recommended actuators (advisory
    // unless the A/B confirms the coupling — see `RefinementAction::actionable`).
    if payload
        .get("refine")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        out["refinements"] = json!(collect_and_recommend(rt, &out["ab"]));
    }

    if check && summary["failed"].as_u64().unwrap_or(0) > 0 {
        out["check_failed"] = json!(true);
    }
    // MED-1 (28/08) — a régua de aderência do `touring run`, do journal
    // durável (o processo CLI morre; o journal fica). Ausência exibida (E4).
    out["code_mode_adherence"] = code_mode_adherence();
    // F6 S4 (2026-09-01) — distinct SDK hooks invoked at runtime, do
    // mirror (~/.claude/touring/sdk_signal_mirror.jsonl). Piso M3 ≥ 6/8
    // antes de F5 promover a Block; por enquanto advisory.
    out["code_mode_signal_use"] = code_mode_signal_use();
    if snapshot {
        match persist_snapshot(&out, &snapshot_date, &rt.project_root) {
            Ok(path) => out["snapshot_path"] = json!(path.display().to_string()),
            Err(e) => out["snapshot_error"] = json!(e.to_string()),
        }
    }
    out.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Loading + path resolution
// ─────────────────────────────────────────────────────────────────────────────

/// P5b (achado de Gabriel, 2026-08-26) — dos programas que o modelo rodou,
/// qual fração FUNDIU round-trips.
///
/// `adoption_ratio` responde "usou a ferramenta certa?" — o CANAL. Ela sobe
/// igual quando o modelo funde dez inspeções numa chamada e quando embrulha uma
/// leitura trivial dez vezes, porque `scan_class_of` não reconhece
/// `touring run` e nenhum gate vê a rota sancionada. Este KPI é a metade que
/// faltava: `economicas / tomadas`, a ECONOMIA.
///
/// Ele fala da APRESENTAÇÃO, não da disciplina do modelo: sob `code` a chamada
/// atômica é negada, então a casca sobre um alvo é obrigatória. Uma economia
/// baixa com adoção alta é o retrato de um braço obedecido e caro.
fn code_mode_economy_ratio(rt: &HookRuntime) -> Option<f64> {
    let path = rt.project_root.join(".claude/touring/code_mode_arm.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let (mut tomadas, mut economicas) = (0.0f64, 0.0f64);
    for arm in ["native", "both", "code"] {
        if let Some(n) = v.get(arm) {
            tomadas += n.get("followed").and_then(Value::as_f64).unwrap_or(0.0);
            economicas += n.get("economical").and_then(Value::as_f64).unwrap_or(0.0);
        }
    }
    if tomadas < crate::cli_suggester::ARM_MIN_SAMPLE as f64 {
        return None;
    }
    Some(economicas / tomadas)
}

/// P2 (decisão (b), 2026-08-26) — taxa de adesão de um braço da apresentação,
/// lida da **mesma fonte que a política lê**.
///
/// Ler os contadores de `gate-metrics` aqui seria mostrar um número diferente
/// do que decide: eles são de processo e zeram no restart (medido 26/08:
/// `t3_turn_first_passed` caiu 2 → 0 em dois minutos), enquanto a política lê
/// `<projeto>/.claude/touring/code_mode_arm.json`. Promover olhando um medidor
/// que não é o do juiz é a classe `verificador-usa-menos-que-o-extrator`.
///
/// `None` abaixo do piso ⇒ STUB, nunca um 0 falso: amostra insuficiente é
/// desconhecido, e o KPI reporta ADVISORY em vez de acusar adesão nula.
fn code_mode_arm_rate(rt: &HookRuntime, arm: &str) -> Option<f64> {
    let path = rt
        .project_root
        .join(".claude/touring/code_mode_arm.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let node = v.get(arm)?;
    let offered = node.get("offered").and_then(Value::as_f64)?;
    let followed = node.get("followed").and_then(Value::as_f64)?;
    if offered < crate::cli_suggester::ARM_MIN_SAMPLE as f64 {
        return None;
    }
    Some(followed / offered)
}

/// W0 S-0.1 — pure ratio for `touring.code_mode.adoption_ratio`. `None` when
/// the denominator has not observed a single Bash action yet (absent signal is
/// unknown, never zero — Lei L2); `Some(0.0)` only when Bash actions exist and
/// none was a code-mode run (a measured zero).
fn code_mode_adoption(runs: f64, bash_calls: f64) -> Option<f64> {
    if bash_calls <= 0.0 {
        return None;
    }
    Some(runs / bash_calls)
}

/// S3 — fração da inspeção que a rajada capturou.
///
/// `denied / (denied + first_passed)`: das inspeções que o gate VIU sob o modo
/// `code`, quantas eram rajada. A medição de 27/08/2026 em 115 transcripts
/// previu ~0,775 com janela de 300s, e é contra esse número que o uso real
/// julga a calibração:
///
/// * muito ABAIXO ⇒ a janela ou o limiar estão apertados demais e o gate quase
///   não fala — foi o destino do T3-B, que media zero;
/// * muito ACIMA ⇒ está pegando inspeção que não é rajada, e a fricção voltou
///   pela porta dos fundos.
///
/// `None` enquanto nenhuma inspeção foi observada: ausência de sinal é
/// desconhecido, nunca zero (Lei L2). `Some(0.0)` só quando houve inspeção e
/// nenhuma virou rajada — um zero MEDIDO.
fn inspect_burst_share(denied: f64, first_passed: f64) -> Option<f64> {
    let total = denied + first_passed;
    if total <= 0.0 {
        return None;
    }
    Some(denied / total)
}

/// P2 elos-exponenciais (29/08): a política discrimina ou é constante? A
/// QTable era um órgão quase write-only — treinada por 3 caminhos, consultada
/// por ~nenhum decisor — e o consumidor já observado devolvia 0.990 para tudo
/// (`uma-execucao-nao-distingue-constante`). Antes de LIGAR a política a
/// decisões de produção, instrumentar: fração dos estados multi-ação cuja
/// dispersão de Q (max−min) supera 0.01. `None`/STUB sem estados multi-ação —
/// tabela rasa é desconhecido, nunca "política constante".
fn policy_discrimination(project_root: &Path) -> Option<f64> {
    let db = touring_foundation::TouringConfig::graph_db_canonical(project_root);
    let conn = rusqlite::Connection::open_with_flags(
        &db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok()?;
    let rows: Vec<(String, f64)> = conn
        .prepare("SELECT state_action, q_value FROM learning_qtable")
        .ok()?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .ok()?
        .filter_map(std::result::Result::ok)
        .collect();
    let mut by_state: std::collections::HashMap<String, (f64, f64, u32)> =
        std::collections::HashMap::new();
    for (sa, q) in rows {
        let state = sa.split(':').next().unwrap_or("").to_string();
        let e = by_state.entry(state).or_insert((f64::MAX, f64::MIN, 0));
        e.0 = e.0.min(q);
        e.1 = e.1.max(q);
        e.2 += 1;
    }
    let multi: Vec<_> = by_state.values().filter(|(_, _, n)| *n >= 2).collect();
    if multi.is_empty() {
        return None;
    }
    let discriminating = multi.iter().filter(|(lo, hi, _)| hi - lo > 0.01).count();
    Some(discriminating as f64 / multi.len() as f64)
}

/// P3 (graph contract, 2026-08-30): share of NEW curated nodes (semantic
/// lesson/decision/diagnostico, 14-day window) honouring the minimum graph
/// contract — deterministic key shape AND at least one typed edge. The shape
/// predicate is `tags::key_shape_ok`, the SAME one the store advisory
/// declares (D8: declared text and enforced predicate share one source).
/// `None` = STUB when the window has no curated nodes — unknown, never a
/// false 1.0.
fn graph_contract_share(project_root: &Path) -> Option<f64> {
    use touring_intelligence::rl::memory::tags;
    let db = touring_foundation::TouringConfig::memory_db_canonical(project_root);
    let conn = rusqlite::Connection::open_with_flags(
        &db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok()?;
    // Cross-audit 2026-08-30 (F-1): the contract governs kinds by FACET
    // (clause 3), so the filter reads memory_tags — the governed vocabulary —
    // never entry_type (a legacy field that diverges: ~39 semantic nodes
    // carried a curated facet with a different entry_type when measured).
    // auto-derive maps entry_type→kind facet, so this is a strict superset.
    let keys: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT e.key FROM memory_entries e
             JOIN memory_tags t ON t.entry_key = e.key
             WHERE e.tier = 'semantic'
               AND t.full_tag IN ('kind:lesson','kind:decision','kind:diagnostico')
               AND e.created_at >= datetime('now','-14 days')",
        )
        .ok()?
        .query_map([], |r| r.get(0))
        .ok()?
        .filter_map(std::result::Result::ok)
        .collect();
    if keys.is_empty() {
        return None;
    }
    let mut ok = 0usize;
    for key in &keys {
        if !tags::key_shape_ok(key) {
            continue;
        }
        // A missing memory_links table reads as unlinked, not as an error:
        // the share then honestly reports how far the corpus is from the
        // contract instead of hiding behind a STUB.
        let linked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_links WHERE src = ?1 OR dst = ?1",
                rusqlite::params![key],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if linked > 0 {
            ok += 1;
        }
    }
    Some(ok as f64 / keys.len() as f64)
}

/// P5 (graph contract, 2026-08-30): typed edges created per new memory entry
/// (14-day window) — the ruler that decides when derived suggestions may ever
/// become automatic. `None` = STUB when the window has no new entries.
fn memory_edge_density(project_root: &Path) -> Option<f64> {
    let db = touring_foundation::TouringConfig::memory_db_canonical(project_root);
    let conn = rusqlite::Connection::open_with_flags(
        &db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok()?;
    let entries: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_entries
             WHERE created_at >= datetime('now','-14 days')",
            [],
            |r| r.get(0),
        )
        .ok()?;
    if entries <= 0 {
        return None;
    }
    let edges: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_links
             WHERE created_at >= datetime('now','-14 days')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Some(edges as f64 / entries as f64)
}

fn default_commitments_path() -> PathBuf {
    // Canonical source tree first: the workspace moved from `~/.claude/rust`
    // to `~/projects/touring` (F4′, 24/07/2026), so the old preferred path
    // could never exist and resolution silently depended on the daemon's cwd.
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home).join("projects/touring/docs/kpi/commitments.yaml");
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("docs/kpi/commitments.yaml")
}

fn load_commitments(path: &PathBuf) -> std::io::Result<CommitmentsFile> {
    let raw = std::fs::read_to_string(path)?;
    serde_yaml::from_str::<CommitmentsFile>(&raw)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

fn today_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    iso_date_from_unix(secs)
}

/// Convert unix seconds to `YYYY-MM-DD` (UTC, Gregorian, post-1970).
#[must_use]
/// MED-1 (28/08) — a régua de aderência do `touring run`, lida do journal
/// durável (`~/.claude/touring/run_journal.jsonl`): `runs_ok/runs_total` é o
/// `successfulExecuteCalls/totalExecuteCalls` que o TanStack chama de CME.
/// O processo CLI morre a cada run; o journal sobrevive. Ausência de journal é
/// EXIBIDA (`available: false`), nunca some (E4). `wasted_attempts` é um proxy
/// documentado: uma run da mesma linguagem <120 s após uma falha (o par
/// falha→retry conta 1).
fn code_mode_adherence() -> Value {
    let Some(home) = std::env::var_os("HOME") else {
        return json!({"available": false, "reason": "HOME unset"});
    };
    let path = PathBuf::from(home).join(".claude/touring/run_journal.jsonl");
    match std::fs::read_to_string(&path) {
        Ok(content) => adherence_from_lines(content.lines()),
        Err(_) => json!({"available": false, "reason": "no journal yet"}),
    }
}

/// F6 — aggregate per-canonical-hook stats from the post-tool-use mirror.
/// Returns `(used, total)` where `total = 8` mirrors `HookName::ALL.len()`.
/// Fail-open: missing file → `(0, 8)` (consistent with the gate).
fn code_mode_signal_use() -> Value {
    const TOTAL_HOOKS: u64 = 8;
    let Some(home) = std::env::var_os("HOME") else {
        return json!({"available": false, "reason": "HOME unset"});
    };
    let path = PathBuf::from(home).join(".claude/touring/sdk_signal_mirror.jsonl");
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let mut seen: std::collections::BTreeSet<String> = Default::default();
            let mut calls = 0u64;
            for line in content.lines() {
                let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
                if let Some(name) = v.get("hook_name").and_then(|x| x.as_str()) {
                    seen.insert(name.to_string());
                    calls += 1;
                }
            }
            json!({
                "available": true,
                "used": seen.len() as u64,
                "total": TOTAL_HOOKS,
                "ratio": seen.len() as f64 / TOTAL_HOOKS as f64,
                "total_calls": calls,
            })
        }
        Err(_) => json!({"available": false, "reason": "no mirror yet"}),
    }
}

/// A agregação pura por trás de [`code_mode_adherence`] — testável sem FS.
fn adherence_from_lines<'a>(lines: impl Iterator<Item = &'a str>) -> Value {
    let mut total = 0u64;
    let mut ok = 0u64;
    let mut by_kind: std::collections::BTreeMap<String, u64> = Default::default();
    let mut by_lang: std::collections::BTreeMap<String, u64> = Default::default();
    let mut wasted = 0u64;
    let mut prev_fail: Option<(u64, String)> = None;
    for line in lines {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        total += 1;
        let lang = v
            .get("language")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        *by_lang.entry(lang.clone()).or_default() += 1;
        let ts = v.get("ts").and_then(Value::as_u64).unwrap_or(0);
        let exit = v.get("exit_code").and_then(Value::as_i64).unwrap_or(-1);
        if exit == 0 {
            ok += 1;
        }
        if let Some(fk) = v.get("failure_kind").and_then(Value::as_str) {
            *by_kind.entry(fk.to_string()).or_default() += 1;
        }
        if let Some((pts, plang)) = prev_fail.take()
            && plang == lang
            && ts.saturating_sub(pts) < 120
        {
            wasted += 1;
        }
        prev_fail = (exit != 0).then_some((ts, lang));
    }
    json!({
        "available": true,
        "runs_total": total,
        "runs_ok": ok,
        "success_rate": (total > 0).then(|| ok as f64 / total as f64),
        "wasted_attempts_retry_pairs": wasted,
        "by_failure_kind": by_kind,
        "by_language": by_lang,
        "source": "run_journal.jsonl",
        "note": "wasted = run da mesma linguagem <120s após uma falha (proxy)",
    })
}

/// Convert unix seconds to `YYYY-MM-DD` (UTC, Gregorian, post-1970).
#[must_use]
pub fn iso_date_from_unix(secs: u64) -> String {
    let days = secs / 86_400;
    let (y, m, d) = days_to_ymd(days as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    days += 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = (days - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y_signed = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y_signed + 1 } else { y_signed };
    (y as i32, m, d)
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-commitment checking
// ─────────────────────────────────────────────────────────────────────────────

fn check_one(rt: &mut HookRuntime, c: &Commitment, external_dir: &std::path::Path) -> CommitmentCheck {
    let (kind, actual) = resolve_source(rt, &c.source, &c.id, external_dir);
    let mut status = match (kind, actual) {
        ("daemon" | "derived" | "external", Some(v)) => {
            check_threshold(v, c.threshold, &c.direction)
        }
        ("daemon" | "derived" | "external", None) => "STUB",
        _ => "ERROR",
    };
    if c.advisory && status == "FAIL" {
        status = "ADVISORY";
    }
    CommitmentCheck {
        id: c.id.clone(),
        name: c.name.clone(),
        threshold: c.threshold,
        direction: c.direction.clone(),
        source_kind: kind,
        actual,
        status,
        rationale: c.rationale.clone(),
        advisory: c.advisory,
        stub_reason: external_stub_reason(kind, actual, external_dir, &c.id),
    }
}

fn resolve_source(
    rt: &mut HookRuntime,
    source: &str,
    id: &str,
    external_dir: &std::path::Path,
) -> (&'static str, Option<f64>) {
    if let Some(rest) = source.strip_prefix("daemon:") {
        let mut parts = rest.splitn(2, ':');
        let handler = parts.next().unwrap_or("");
        let pointer = parts.next().unwrap_or("");
        let value = invoke_handler(rt, handler);
        let extracted = value
            .as_ref()
            .and_then(|v| v.pointer(pointer))
            .and_then(json_value_as_f64);
        return ("daemon", extracted);
    }
    if let Some(name) = source.strip_prefix("derived:") {
        return ("derived", resolve_derived(rt, name));
    }
    if source.starts_with("external:") {
        return ("external", resolve_external(external_dir, id));
    }
    ("unknown", None)
}

/// Out-of-band measurements stay valid for a fortnight: `external:` sources
/// are expensive runs (a full llvm-cov is ~10 min) whose numbers move slowly,
/// and a months-old figure must not masquerade as current.
const EXTERNAL_STALE_SECS: u64 = 14 * 24 * 3600;

/// Reads the recorded measurement for an `external:` commitment.
///
/// `touring kpi` never runs external commands itself (a dashboard must not
/// spawn a 10-minute `llvm-cov`) — until 2026-08-28 `external:` sources
/// resolved to `None` unconditionally, so `test.count`/`coverage.line` stayed
/// STUB forever even after being measured. The `source` field documents HOW
/// to measure; whoever runs the measurement drops
/// `<commitments-dir>/external/<id>.json` with a numeric `value` field (extra
/// fields like `measured_at`/`command` are for human audit) and this reader
/// surfaces it while fresh (file mtime within [`EXTERNAL_STALE_SECS`]).
/// Missing, stale or malformed → `None` (STUB): an unrecorded measurement is
/// unknown, never zero.
fn resolve_external(external_dir: &std::path::Path, id: &str) -> Option<f64> {
    resolve_external_detailed(external_dir, id).ok()
}

/// Why an `external:` commitment resolved to no value. The three causes have
/// three different remedies, so the dashboard names which one it is instead
/// of a bare STUB — otherwise "nobody measured" is indistinguishable from
/// "measured long ago", the very silence that kept `test.count` /
/// `coverage.line` STUB until 2026-08-28 (see [`resolve_external`]'s history).
#[derive(Debug, PartialEq)]
enum ExternalStub {
    /// No measurement file on disk — declared in the contract, never fed.
    Missing,
    /// A measurement exists but is older than [`EXTERNAL_STALE_SECS`].
    Stale { days: u64 },
    /// The producer measured and DECLARED no value yet (`value: null` plus a
    /// `detalhe`/`note` string) — e.g. "sample n=6 below the minimum 20". An
    /// honest unknown, not a defect (cross-audit 30/08, achado A3: the first
    /// real producer did exactly this and `Malformed` would slander it).
    Declared { detalhe: String },
    /// The file exists and is fresh but carries no numeric `value` field
    /// and no producer explanation.
    Malformed,
}

impl ExternalStub {
    /// The message teaches the remedy (A5): each cause names its own fix and
    /// the exact drop path, so the operator acts without reading this file.
    fn teach(&self, id: &str) -> String {
        match self {
            Self::Missing => format!(
                "never measured — run the `source` command and drop docs/kpi/external/{id}.json with a numeric `value` field"
            ),
            Self::Stale { days } => format!(
                "stale — measured {days}d ago (window 14d); re-run the `source` command to refresh docs/kpi/external/{id}.json"
            ),
            Self::Declared { detalhe } => format!(
                "declared unmeasured by the producer — {detalhe}"
            ),
            Self::Malformed => format!(
                "malformed — docs/kpi/external/{id}.json exists but has no numeric `value` field"
            ),
        }
    }
}

/// Pure verdict over the gathered facts (file age + raw content) — testable
/// without a filesystem, the same shape as [`adherence_from_lines`].
fn external_verdict(age_secs: u64, raw: Option<&str>) -> Result<f64, ExternalStub> {
    if age_secs > EXTERNAL_STALE_SECS {
        return Err(ExternalStub::Stale {
            days: age_secs / 86_400,
        });
    }
    let raw = raw.ok_or(ExternalStub::Missing)?;
    let parsed = serde_json::from_str::<Value>(raw).ok();
    if let Some(v) = parsed
        .as_ref()
        .and_then(|v| v.pointer("/value"))
        .and_then(json_value_as_f64)
    {
        return Ok(v);
    }
    // No numeric value: an explicit `value: null` carrying a producer
    // explanation is an honest unknown — surface THEIR words, never an
    // accusation (E4/E5: display the absence with its real cause).
    if let Some(v) = parsed.as_ref()
        && v.pointer("/value").is_some_and(Value::is_null)
        && let Some(detalhe) = ["/detalhe", "/note", "/nota"]
            .iter()
            .find_map(|p| v.pointer(p).and_then(Value::as_str))
            .filter(|s| !s.trim().is_empty())
    {
        return Err(ExternalStub::Declared {
            detalhe: detalhe.chars().take(160).collect(),
        });
    }
    Err(ExternalStub::Malformed)
}

fn resolve_external_detailed(
    external_dir: &std::path::Path,
    id: &str,
) -> Result<f64, ExternalStub> {
    let path = external_dir.join(format!("{id}.json"));
    let Ok(meta) = std::fs::metadata(&path) else {
        return Err(ExternalStub::Missing);
    };
    // An unreadable mtime counts as fresh — staleness only fires when the
    // clock could actually be read (the pre-refactor behavior).
    let age_secs = meta
        .modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map_or(0, |d| d.as_secs());
    let raw = std::fs::read_to_string(&path).ok();
    external_verdict(age_secs, raw.as_deref())
}

/// The `stub_reason` wiring for [`check_one`], kept pure so a test reaches it
/// without a `HookRuntime`: only an `external:` source that resolved to no
/// value carries a reason.
fn external_stub_reason(
    kind: &str,
    actual: Option<f64>,
    external_dir: &std::path::Path,
    id: &str,
) -> Option<String> {
    (kind == "external" && actual.is_none())
        .then(|| resolve_external_detailed(external_dir, id))
        .and_then(Result::err)
        .map(|s| s.teach(id))
}

/// Resolves a `derived:<name>` KPI — a value computed from already-collected
/// data with no new instrumentation, for the `touring.coupling.*` family (F1).
/// Returns `None` (→ `STUB`) when the underlying data is unavailable.
fn resolve_derived(rt: &mut HookRuntime, name: &str) -> Option<f64> {
    match name {
        "health_delta_net" => {
            let m = invoke_handler(rt, "cli-gate-metrics")?;
            let imp = m
                .pointer("/health_delta_improvement_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            let reg = m
                .pointer("/health_delta_regression_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            Some(imp - reg)
        }
        "suggestion_uptake" => {
            let m = invoke_handler(rt, "cli-gate-metrics")?;
            let emitted = m
                .pointer("/suggestion_uptake_emitted_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            if emitted <= 0.0 {
                return None;
            }
            let followed = m
                .pointer("/suggestion_uptake_followed_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            Some(followed / emitted)
        }
        "adoption_ratio" => {
            let m = invoke_handler(rt, "cli-gate-metrics")?;
            let touring = m
                .pointer("/adoption_touring_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            let antipattern = m
                .pointer("/adoption_antipattern_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            let total = touring + antipattern;
            if total <= 0.0 {
                return None;
            }
            Some(touring / total)
        }
        "pillar_induction_ratio" => {
            // Task #6 — followed / emitted for the armed pillar-induction layer
            // (master-cli + learning-memory nudges). `None` until the layer emits
            // (default-OFF), so it reports ADVISORY rather than a false 0.
            //
            // R2 (29/08, ordem de Gabriel): a fonte é o arquivo DURÁVEL
            // (`durable_gate_evidence.json`), não os contadores de processo —
            // 3 deploys num dia são 3 apagões da amostra, e um medidor que
            // esquece nunca cruza o piso (a mesma razão que fez o braço da
            // apresentação ler `code_mode_arm.json`).
            let v = crate::cli_suggester::read_durable_evidence(&rt.project_root);
            let emitted = v
                .pointer("/pillar/emitted")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            if emitted <= 0.0 {
                return None;
            }
            let followed = v
                .pointer("/pillar/followed")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            Some(followed / emitted)
        }
        "code_mode_economy_ratio" => code_mode_economy_ratio(rt),
        "code_mode_arm_native" => code_mode_arm_rate(rt, "native"),
        "code_mode_arm_both" => code_mode_arm_rate(rt, "both"),
        "code_mode_arm_code" => code_mode_arm_rate(rt, "code"),
        "inspect_burst_share" => {
            // S3 (2026-08-27) — a leitura da recalibração. O veredito sobre o
            // limiar precisa de DIAS de uso — e uma série que zera a cada
            // restart do daemon nunca acumula dias (R2, 29/08: os counters de
            // processo ficaram para o `gate-metrics` vivo; o KPI lê o arquivo
            // durável que o gate incrementa ao lado deles).
            let v = crate::cli_suggester::read_durable_evidence(&rt.project_root);
            let denied = v
                .pointer("/s3/denied")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            let passed = v
                .pointer("/s3/first_passed")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            inspect_burst_share(denied, passed)
        }
        "code_mode_adoption_ratio" => {
            // W0 S-0.1 (plano code-mode-total, 2026-08-24) — `touring run`
            // executions over ALL Bash actions, both daemon-lifetime
            // accumulators fed by the suggester hook and the run journal relay.
            let m = invoke_handler(rt, "cli-gate-metrics")?;
            let bash = m
                .pointer("/bash_calls_total_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            let runs = m
                .pointer("/code_mode_runs_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            code_mode_adoption(runs, bash)
        }
        "world_model_success" => read_world_model_success(),
        "learning_policy_discrimination" => policy_discrimination(&rt.project_root),
        "learning_replay_share" => {
            // P2 replay (29/08) — que fração do corpus de outcomes
            // recompensados o OnlineRLEngine já consumiu. Numerador do cursor
            // durável (`learning_replay_cursor.json`), denominador contado no
            // memory.db AGORA — a mesma fonte que o replay lê. `None` = STUB
            // enquanto nada foi replayado E o corpus está invisível (db
            // ilegível): amostra ausente é desconhecido, nunca zero.
            let (last_rowid, replayed_total, _) =
                crate::cli::learning::replay_cursor_read(&rt.project_root);
            let pending =
                crate::cli::learning::replay_corpus_pending(&rt.project_root, last_rowid);
            match (replayed_total, pending) {
                (0, None) => None,
                (r, p) => {
                    let total = r as f64 + p.unwrap_or(0).max(0) as f64;
                    if total <= 0.0 { None } else { Some(r as f64 / total) }
                }
            }
        }
        // The `touring.memory.*` family (2026-08-02). Derived by SQL over the
        // project's memory.db — no new instrumentation, same shape as the ADW
        // derivations. Retroactive justification for the family: the ANN corpus
        // had drifted to 79,7 % coverage for weeks with a one-command remedy, and
        // nothing measured it, so nobody could see it.
        "memory_corpus_coverage" => memory_corpus_coverage(&rt.project_root),
        "memory_curated_recall_share" => memory_curated_recall_share(&rt.project_root),
        "memory_graph_contract_share" => graph_contract_share(&rt.project_root),
        "memory_edge_density" => memory_edge_density(&rt.project_root),
        "memory_never_recalled_ratio" => memory_never_recalled_ratio(&rt.project_root),
        // F6.4 (ADW plan 2026-07-19) — the software-factory KPI family. All are
        // file-derived from per-project artifacts the ADW stack already writes;
        // `None` (→ STUB) whenever the project has no such artifacts yet.
        "adw_explore_rounds_to_dry" => adw_explore_rounds_to_dry(&rt.project_root),
        "adw_plan_refine_iters" => adw_plan_refine_iters(&rt.project_root),
        "adw_runs" => adw_runs_count(&rt.project_root),
        "adw_router_accuracy" => adw_router_accuracy(&rt.project_root),
        "adw_zte_bypass_rate" => adw_zte_bypass_rate(&rt.project_root),
        // M0 (estratégia paralelização 29/08/2026) — instrumentar ANTES de
        // esperar adoção: as duas afordâncias de paralelismo eram invisíveis
        // ao instrumento (inventário 29/08: journal sem discriminador do
        // `parallel`; tier só como estimativa estática do explain --cost).
        "code_mode_parallel_runs" => code_mode_parallel_runs(),
        "adw_tiered_agent_share" => adw_tiered_agent_share(&rt.project_root),
        // E3 (flow enforcement 2026-07-23) — gated-flow OUTER compliance for
        // THIS project, fed by loop_outer_gate.py evaluations at every Stop.
        "flow_compliance" => flow_compliance_ratio(&rt.project_root),
        _ => None,
    }
}

/// Mean number of exploration rounds until the CCE ledger converged, over every
/// `.touring-explore/*.ledger.json` in the current project. `None` when no
/// ledger has converged yet (the KPI only speaks about *finished* explorations).
fn adw_explore_rounds_to_dry(root: &std::path::Path) -> Option<f64> {
    let dir = root.join(".touring-explore");
    let mut totals: Vec<f64> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(ledger) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let converged = ledger
            .pointer("/verdict/converged")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if converged
            && let Some(rounds) = ledger
                .pointer("/rounds")
                .and_then(serde_json::Value::as_array)
        {
            totals.push(rounds.len() as f64);
        }
    }
    if totals.is_empty() {
        return None;
    }
    Some(totals.iter().sum::<f64>() / totals.len() as f64)
}

/// Highest iteration count across `*.refine.json` plan-refinement ledgers
/// (searched shallowly: project root and `docs/plans/**`, depth-capped).
fn adw_plan_refine_iters(root: &std::path::Path) -> Option<f64> {
    fn scan(dir: &std::path::Path, depth: usize, best: &mut Option<f64>) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan(&path, depth - 1, best);
            } else if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".refine.json"))
                && let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
            {
                // O produtor real (`plan_refine.py`) grava `{version,
                // iterations: […]}`; só o formato de array cru fazia o KPI
                // ficar STUB com ledger legítimo no disco (produtor≠consumidor,
                // descoberto 28/08/2026 exercitando a fonte de verdade).
                let iters = match &value {
                    serde_json::Value::Array(a) => Some(a.len()),
                    v => v
                        .pointer("/iterations")
                        .and_then(serde_json::Value::as_array)
                        .map(|a| a.len()),
                };
                if let Some(n) = iters {
                    let n = n as f64;
                    if best.is_none_or(|b| n > b) {
                        *best = Some(n);
                    }
                }
            }
        }
    }
    let mut best = None;
    scan(&root.join("docs").join("plans"), 4, &mut best);
    scan(root, 1, &mut best);
    best
}

/// M0 (29/08/2026) — distinct sandbox runs with at least one sub-call issued
/// via `touring.parallel` (the SDKs stamp a `:par` suffix on the origin).
/// Reads the daemon-side `~/.claude/touring/run_subcalls.jsonl`; `None`
/// (→ STUB) until the first parallel sub-call is journaled — before this
/// counter the fan-out affordance's adoption was invisible by construction.
fn code_mode_parallel_runs() -> Option<f64> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::Path::new(&home).join(".claude/touring/run_subcalls.jsonl");
    code_mode_parallel_runs_from(&path)
}

/// Testable core of [`code_mode_parallel_runs`]: the wrapper resolves `$HOME`
/// (process-global, unsafe to mutate in parallel tests), this one takes the
/// journal path.
fn code_mode_parallel_runs_from(path: &std::path::Path) -> Option<f64> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut runs = std::collections::BTreeSet::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(origin) = v.get("origin").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if let Some(resto) = origin.strip_suffix(":par")
            && let Some(run_id) = resto.split(":code:").next()
        {
            runs.insert(run_id.to_string());
        }
    }
    if runs.is_empty() {
        return None;
    }
    Some(runs.len() as f64)
}

/// M0 (29/08/2026) — share of EXECUTED ADW agent-node starts that declared a
/// `tier`, over this project's run journals. The static `calls_by_tier` in
/// `explain --cost` estimates; this measures. `None` until a journal carries
/// an agent `node_started` with the (new) `tier` key — old journals predate
/// the instrumentation and must not read as "0% tiered".
fn adw_tiered_agent_share(root: &std::path::Path) -> Option<f64> {
    let dir = root.join(".touring").join("adw-runs");
    let mut agentes = 0u64;
    let mut com_tier = 0u64;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let journal = entry.path().join("journal.jsonl");
        let Ok(text) = std::fs::read_to_string(&journal) else {
            continue;
        };
        for line in text.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if v.get("event").and_then(serde_json::Value::as_str) != Some("node_started")
                || v.get("type").and_then(serde_json::Value::as_str) != Some("agent")
                || !v.as_object().is_some_and(|o| o.contains_key("tier"))
            {
                continue;
            }
            agentes += 1;
            if v.get("tier")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|t| !t.is_empty())
            {
                com_tier += 1;
            }
        }
    }
    if agentes == 0 {
        return None;
    }
    Some(com_tier as f64 / agentes as f64)
}

/// Number of ADW runs recorded for this project (`.touring/adw-runs/*/journal.jsonl`).
fn adw_runs_count(root: &std::path::Path) -> Option<f64> {
    let dir = root.join(".touring").join("adw-runs");
    let mut count = 0u64;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        if entry.path().join("journal.jsonl").is_file() {
            count += 1;
        }
    }
    Some(count as f64)
}

/// Fraction of factory-routed runs whose outcome completed (proxy for
/// `router_accuracy` until human relabeling exists). `None` before any outcome.
fn adw_router_accuracy(root: &std::path::Path) -> Option<f64> {
    let path = root.join(".touring").join("factory").join("stats.json");
    let stats: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let outcomes = stats.pointer("/outcomes")?.as_array()?;
    if outcomes.is_empty() {
        return None;
    }
    let completed = outcomes
        .iter()
        .filter(|o| o.pointer("/status").and_then(serde_json::Value::as_str) == Some("completed"))
        .count();
    Some(completed as f64 / outcomes.len() as f64)
}

/// ZTE bypasses per finished ADW run — the bypass must stay the audited
/// exception, never the rule. `None` before any run finishes.
fn adw_zte_bypass_rate(root: &std::path::Path) -> Option<f64> {
    let dir = root.join(".touring").join("adw-runs");
    let mut finished = 0u64;
    let mut bypasses = 0u64;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let journal = entry.path().join("journal.jsonl");
        let Ok(text) = std::fs::read_to_string(journal) else {
            continue;
        };
        if text.contains("\"run_finished\"") {
            finished += 1;
        }
        bypasses += text.matches("\"zte_bypass\"").count() as u64;
    }
    if finished == 0 {
        return None;
    }
    Some(bypasses as f64 / finished as f64)
}

/// Fraction of gated-flow OUTER evaluations whose artifact manifest was
/// complete, for THIS project (`~/.claude/loop-engineering/compliance.jsonl`,
/// one JSONL record per Stop-gate evaluation, written by `loop_outer_gate.py`).
/// `None` before any evaluation — the KPI only speaks once a gated flow has
/// actually been enforced here.
fn flow_compliance_ratio(root: &std::path::Path) -> Option<f64> {
    let home = std::env::var("HOME").ok()?;
    let log = PathBuf::from(home).join(".claude/loop-engineering/compliance.jsonl");
    flow_compliance_from_log(&log, root)
}

/// Pure core of [`flow_compliance_ratio`], separated so tests can feed a
/// synthetic log: ratio of `complete: true` records whose `cwd` is `root`.
/// Open the project's `memory.db` read-only, or `None` when it does not exist.
fn memory_db(root: &std::path::Path) -> Option<rusqlite::Connection> {
    let path = touring_foundation::TouringConfig::memory_db_canonical(root);
    if !path.exists() {
        return None;
    }
    rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

/// Share of stored entries that are actually reachable by ANN recall.
///
/// Counted as a JOIN (entries present in the corpus / entries), not
/// `COUNT(embeddings) / COUNT(entries)` — the corpus can hold rows for entries
/// that no longer exist, which would push the ratio above 1.0 and hide a real gap.
fn memory_corpus_coverage(root: &std::path::Path) -> Option<f64> {
    let conn = memory_db(root)?;
    let entries: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_entries", [], |r| r.get(0))
        .ok()?;
    if entries <= 0 {
        return None;
    }
    let indexed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_entries me JOIN embeddings em ON em.id = me.key",
            [],
            |r| r.get(0),
        )
        .ok()?;
    Some(indexed as f64 / entries as f64)
}

/// Share of retrievals landing on curated lessons rather than auto-recorded noise.
///
/// Uses the SAME `outcome:` namespace the recall filter drops, so the metric
/// tracks that fix's effect directly instead of muddying attribution with other
/// automatic namespaces.
fn memory_curated_recall_share(root: &std::path::Path) -> Option<f64> {
    let conn = memory_db(root)?;
    let total: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(access_count), 0) FROM memory_entries",
            [],
            |r| r.get(0),
        )
        .ok()?;
    if total <= 0 {
        return None;
    }
    let curated: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(access_count), 0) FROM memory_entries
             WHERE key NOT LIKE 'outcome:%'",
            [],
            |r| r.get(0),
        )
        .ok()?;
    Some(curated as f64 / total as f64)
}

/// Share of entries no recall has ever returned — dead weight in the store.
fn memory_never_recalled_ratio(root: &std::path::Path) -> Option<f64> {
    let conn = memory_db(root)?;
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_entries", [], |r| r.get(0))
        .ok()?;
    if total <= 0 {
        return None;
    }
    let never: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_entries WHERE COALESCE(access_count, 0) = 0",
            [],
            |r| r.get(0),
        )
        .ok()?;
    Some(never as f64 / total as f64)
}

fn flow_compliance_from_log(log: &std::path::Path, root: &std::path::Path) -> Option<f64> {
    let text = std::fs::read_to_string(log).ok()?;
    let root_str = root.display().to_string();
    let (mut total, mut complete) = (0u64, 0u64);
    for line in text.lines() {
        let Ok(rec) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if rec.pointer("/cwd").and_then(Value::as_str) != Some(root_str.as_str()) {
            continue;
        }
        total += 1;
        if rec.pointer("/complete").and_then(Value::as_bool) == Some(true) {
            complete += 1;
        }
    }
    if total == 0 {
        return None;
    }
    Some(complete as f64 / total as f64)
}

/// Σsuccesses / (Σsuccesses + Σfailures) over `action_world_model.json`
/// (`~/.claude/touring/action_world_model.json`). Returns `None` when the file
/// is absent/unreadable or has no recorded outcomes yet.
fn read_world_model_success() -> Option<f64> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".claude/touring/action_world_model.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let model: Value = serde_json::from_str(&raw).ok()?;
    let entries = model.get("entries").and_then(Value::as_array)?;
    let (mut succ, mut fail) = (0.0_f64, 0.0_f64);
    for e in entries {
        succ += e
            .get("successes")
            .and_then(json_value_as_f64)
            .unwrap_or(0.0);
        fail += e.get("failures").and_then(json_value_as_f64).unwrap_or(0.0);
    }
    let total = succ + fail;
    if total <= 0.0 {
        None
    } else {
        Some(succ / total)
    }
}

/// `<handler>@<scope>` — a `daemon:` source may carry a scope argument
/// declared in the YAML (e.g. `daemon:cli-mutation-test@touring-identity:/kill_rate`),
/// so the target lives in the contract instead of hardcoded here.
fn split_handler_scope(handler: &str) -> (&str, Option<&str>) {
    match handler.split_once('@') {
        Some((name, scope)) if !scope.is_empty() => (name, Some(scope)),
        _ => (handler, None),
    }
}

fn invoke_handler(rt: &mut HookRuntime, handler: &str) -> Option<Value> {
    let (name, scope) = split_handler_scope(handler);
    let raw = match name {
        "cli-wiring-status" => super::super::cli_handlers::cli_wiring_status(rt, &Value::Null),
        "cli-learning-status" => super::super::cli_handlers::cli_learning_status(rt, &Value::Null),
        "cli-gate-metrics" => super::super::cli_handlers::cli_gate_metrics(rt, &Value::Null),
        "cli-gotcha-stats" => super::super::cli_handlers::cli_gotcha_stats(rt, &Value::Null),
        "cli-memory-stats" => super::super::cli_handlers::cli_memory_stats(rt, &Value::Null),
        // Cache-only read: the mutation run itself takes ~20 min and is
        // triggered explicitly (`touring mutation-test --package <p>`); the
        // KPI must never spawn it — a cold cache reads as STUB, by design.
        "cli-mutation-test" => crate::cli_handlers_mutation_test::cli_mutation_test(
            rt,
            &json!({"cache_only": true, "package": scope}),
        ),
        _ => return None,
    };
    serde_json::from_str(&raw).ok()
}

fn json_value_as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_u64().map(|n| n as f64))
        .or_else(|| v.as_i64().map(|n| n as f64))
        .or_else(|| v.as_bool().map(|b| if b { 1.0 } else { 0.0 }))
}

/// Returns `"PASS"` or `"FAIL"` per RFC-R2 directions.
#[must_use]
pub fn check_threshold(actual: f64, threshold: f64, direction: &str) -> &'static str {
    let pass = match direction {
        "gte" => actual >= threshold,
        "lte" => actual <= threshold,
        "eq" => (actual - threshold).abs() < f64::EPSILON,
        _ => false,
    };
    if pass { "PASS" } else { "FAIL" }
}

fn summarize(checks: &[CommitmentCheck]) -> Value {
    let total = checks.len();
    let passed = checks.iter().filter(|e| e.status == "PASS").count();
    let failed = checks.iter().filter(|e| e.status == "FAIL").count();
    let stub = checks.iter().filter(|e| e.status == "STUB").count();
    let advisory = checks.iter().filter(|e| e.status == "ADVISORY").count();
    json!({
        "total": total,
        "passed": passed,
        "failed": failed,
        "stub": stub,
        "advisory": advisory,
        "regressions": 0,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot persistence
// ─────────────────────────────────────────────────────────────────────────────

/// Path to the latest A/B attribution block written by `run_bench.py --compare`
/// (mirrors its `DEFAULT_AB_OUT`: `<workspace>/docs/agentic-bench/.ab-latest.json`).
fn default_ab_path() -> PathBuf {
    // Canonical source tree first (F4′, 24/07/2026): `~/.claude/rust` is the
    // FROZEN tree — preferring it meant reading A/B blocks nothing writes
    // anymore. The frozen path stays as a read fallback for pre-move history.
    if let Ok(home) = std::env::var("HOME") {
        let canonical =
            PathBuf::from(&home).join("projects/touring/docs/agentic-bench/.ab-latest.json");
        if canonical.exists() {
            return canonical;
        }
        PathBuf::from(home).join(".claude/rust/docs/agentic-bench/.ab-latest.json")
    } else {
        PathBuf::from("docs/agentic-bench/.ab-latest.json")
    }
}

/// Reads the latest A/B causal-attribution block (telemetry §10) persisted by
/// `run_bench.py --compare`. Returns `None` when no A/B has run yet → the snapshot
/// carries `"ab": null` (an honest absence, never a fabricated zero).
fn build_ab_block(path: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&raw).ok()
}

fn persist_snapshot(
    payload: &Value,
    date: &str,
    project_root: &std::path::Path,
) -> std::io::Result<PathBuf> {
    let month = date.get(0..7).unwrap_or("0000-00");
    // Canonical source tree (F4′, 24/07/2026): snapshots were still being
    // written under `~/.claude/rust/docs/kpi` — the FROZEN tree — so the
    // dated series silently accumulated where no reader looks anymore.
    let dir = if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join("projects/touring/docs/kpi")
            .join(month)
    } else {
        PathBuf::from("docs/kpi").join(month)
    };
    std::fs::create_dir_all(&dir)?;
    // One file per project per day (investigation 2026-07-01): the daemon's F5
    // flush snapshots EVERY warm project, and per-project sources (orphans,
    // ema) differ wildly between projects — a shared `{date}.json` made
    // consecutive writes silently overwrite each other and the dated series
    // uninterpretable (observed: 368→0→0→12002 across mixed projects).
    // Readers glob `docs/kpi/*/*.json`, so the new name stays discoverable.
    let file = dir.join(format!("{date}--{}.json", project_slug(project_root)));
    let pretty = serde_json::to_string_pretty(payload)?;
    std::fs::write(&file, pretty)?;
    Ok(file)
}

/// Deterministic filename-safe slug for a project root: the full path with
/// separators folded to `-` (collision-free, unlike a basename).
fn project_slug(project_root: &std::path::Path) -> String {
    let slug: String = project_root.display().to_string().replace(['/', '\\'], "-");
    slug.trim_matches('-').to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// F7 — refinement engine (telemetry §12): coupling KPIs → recommended actuators
// ─────────────────────────────────────────────────────────────────────────────

/// The coupling KPI signals the refinement engine reads (telemetry §5, D1–D6).
/// Each is `None` when its data is unavailable (STUB) — the engine recommends
/// nothing for a signal it cannot observe (no fabricated action).
#[derive(Debug, Clone, Default)]
pub struct CouplingSignals {
    /// D3 suggestion-uptake (followed/emitted); low → demote noisy hints.
    pub suggestion_uptake: Option<f64>,
    /// D2 STR bytes/emit; rising → tighten `--brief`/summarizer elision.
    pub str_bytes_per_emit: Option<f64>,
    /// D1 adoption_ratio (the mother metric); low → promote the capability.
    pub adoption_ratio: Option<f64>,
    /// D4 net health movement (improvements − regressions); < 0 → drift alert.
    pub health_delta_net: Option<f64>,
    /// A/B causal gate (F6): `Some(true)` iff treatment beat control (coupling
    /// confirmed). `None` = no A/B run. Actuators auto-apply only when confirmed.
    pub ab_attributable: Option<bool>,
}

/// A recommended refinement actuator (telemetry §12). `actionable` is the A/B
/// gate: an action auto-applies only once the coupling is causally confirmed
/// (`ab_attributable == Some(true)`); otherwise it is surfaced advisory-only —
/// the discipline that separates "induce" from "mutate a system not proven to help".
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RefinementAction {
    /// Actuator kind (stable machine tag).
    pub kind: &'static str,
    /// The KPI that triggered it.
    pub signal: &'static str,
    /// Observed value of the triggering KPI.
    pub observed: f64,
    /// Threshold it breached.
    pub threshold: f64,
    /// Whether the A/B gate authorises auto-application (vs advisory-only).
    pub actionable: bool,
    /// Human rationale for the recommendation.
    pub rationale: String,
}

/// Thresholds mirror the `touring.coupling.*` commitments (telemetry §7).
const UPTAKE_FLOOR: f64 = 0.40;
const STR_BYTES_CEIL: f64 = 800.0;
const ADOPTION_FLOOR: f64 = 0.50;

/// Assembles the live coupling signals (telemetry §5) and runs the F7 engine.
/// The A/B gate is read from the snapshot's `ab` block (`attributable`). This is
/// the I/O glue over the pure [`recommend_refinements`]; exercised end-to-end by
/// `touring kpi --refine`.
fn collect_and_recommend(rt: &mut HookRuntime, ab: &Value) -> Vec<RefinementAction> {
    let str_bytes = invoke_handler(rt, "cli-gate-metrics")
        .as_ref()
        .and_then(|m| m.pointer("/enrichment_mean_bytes_per_emit"))
        .and_then(json_value_as_f64);
    let signals = CouplingSignals {
        suggestion_uptake: resolve_derived(rt, "suggestion_uptake"),
        str_bytes_per_emit: str_bytes,
        adoption_ratio: resolve_derived(rt, "adoption_ratio"),
        health_delta_net: resolve_derived(rt, "health_delta_net"),
        ab_attributable: ab.get("attributable").and_then(Value::as_bool),
    };
    recommend_refinements(&signals)
}

/// Pure refinement engine (telemetry §12): maps coupling KPIs to the actuators
/// that would refine the strategy. A/B-gated — `actionable` is true only when the
/// coupling is causally confirmed; otherwise actions are advisory (recommend,
/// don't auto-apply). Empty vec when every observed signal is healthy.
#[must_use]
pub fn recommend_refinements(s: &CouplingSignals) -> Vec<RefinementAction> {
    let gate = s.ab_attributable == Some(true);
    let mut actions = Vec::new();

    if let Some(uptake) = s.suggestion_uptake
        && uptake < UPTAKE_FLOOR
    {
        actions.push(RefinementAction {
            kind: "demote_hint",
            signal: "suggestion_uptake",
            observed: uptake,
            threshold: UPTAKE_FLOOR,
            actionable: gate,
            rationale: format!(
                "uptake {uptake:.2} < {UPTAKE_FLOOR:.2}: hints are ignored — demote the \
                     noisiest cluster and re-arm with a number (telemetry §12, I5)."
            ),
        });
    }

    if let Some(bytes) = s.str_bytes_per_emit
        && bytes > STR_BYTES_CEIL
    {
        actions.push(RefinementAction {
            kind: "tighten_elision",
            signal: "str_bytes_per_emit",
            observed: bytes,
            threshold: STR_BYTES_CEIL,
            actionable: gate,
            rationale: format!(
                "STR {bytes:.0}B/emit > {STR_BYTES_CEIL:.0}: tighten the --brief/summarizer \
                     elision floor (auto-tune, telemetry §12)."
            ),
        });
    }

    if let Some(adoption) = s.adoption_ratio
        && adoption < ADOPTION_FLOOR
    {
        actions.push(RefinementAction {
            kind: "promote_capability",
            signal: "adoption_ratio",
            observed: adoption,
            threshold: ADOPTION_FLOOR,
            actionable: gate,
            rationale: format!(
                "adoption {adoption:.2} < {ADOPTION_FLOOR:.2}: prior-bash still wins — promote \
                     the capability via a high-signal-rare trigger (telemetry §12)."
            ),
        });
    }

    if let Some(net) = s.health_delta_net
        && net < 0.0
    {
        actions.push(RefinementAction {
            kind: "alert_drift",
            signal: "health_delta_net",
            observed: net,
            threshold: 0.0,
            actionable: gate,
            rationale: format!(
                "health_delta_net {net:.0} < 0: coupling-guided edits regress more than they \
                     improve — raise a drift alert + RL penalty (telemetry §12)."
            ),
        });
    }

    actions
}

/// F7 actuator (telemetry §12, hint demotion) — the *brain* of F7c. Returns the
/// additive bump to the hint firing threshold when the coupling data shows hints
/// are ignored (`suggestion_uptake` below the floor) AND the A/B gate confirms the
/// coupling helps (`ab_attributable == Some(true)`). `0.0` otherwise — graduated by
/// how far below the floor, capped at `+0.30`. `cli_suggester` applies this only when
/// armed (`TOURING_F7_ACTUATOR_ARMED`), so the default is zero live impact.
#[must_use]
pub fn hint_demotion_bump(suggestion_uptake: Option<f64>, ab_attributable: Option<bool>) -> f32 {
    if ab_attributable != Some(true) {
        return 0.0; // A/B gate: never demote a coupling not proven beneficial.
    }
    match suggestion_uptake {
        Some(u) if u < UPTAKE_FLOOR => (((UPTAKE_FLOOR - u) * 0.5) as f32).min(0.30),
        _ => 0.0,
    }
}

/// F7c actuator signal source: the live `(suggestion_uptake, ab_attributable)` pair
/// the `cli_suggester` demotion gate consults when armed. **Read-only** — uptake from
/// the global gate-metrics snapshot (the same counters `derived:suggestion_uptake`
/// reads, so the actuator and the dashboard agree) and the A/B verdict from disk; no
/// `HookRuntime` needed (the hot path holds only `&HookRuntime`). Lives here with the
/// engine so all F7 signal logic is co-located.
pub fn actuator_signals() -> (Option<f64>, Option<bool>) {
    let snap = crate::shared::gate_metrics::GateMetricsSnapshot::capture();
    let emitted = snap.suggestion_uptake_emitted_count;
    let uptake =
        (emitted > 0).then(|| snap.suggestion_uptake_followed_count as f64 / emitted as f64);
    let ab = build_ab_block(&default_ab_path())
        .and_then(|v| v.get("attributable").and_then(Value::as_bool));
    (uptake, ab)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-audit 2026-08-30 (F-1) — o contrato do grafo governa kinds por
    /// FACETA (cláusula 3), então o KPI filtra por memory_tags, nunca por
    /// entry_type (medido: nós semantic com `kind:decision` na faceta e
    /// entry_type legado ficavam invisíveis). Mutação que mata: reverter o
    /// filtro para `entry_type IN (...)`.
    #[test]
    fn graph_contract_share_counts_by_facet_not_entry_type() {
        let dir = std::env::temp_dir().join(format!("kpi-gcs-{}", std::process::id()));
        let db_dir = dir.join(".claude").join("touring");
        std::fs::create_dir_all(&db_dir).expect("tempdir");
        let conn = rusqlite::Connection::open(db_dir.join("memory.db")).expect("open");
        conn.execute_batch(
            "CREATE TABLE memory_entries (key TEXT PRIMARY KEY, tier TEXT,
                 entry_type TEXT, created_at TEXT);
             CREATE TABLE memory_tags (entry_key TEXT, full_tag TEXT);
             CREATE TABLE memory_links (id TEXT PRIMARY KEY, src TEXT, dst TEXT,
                 rel TEXT, created_at TEXT DEFAULT (datetime('now')));
             -- curado PELA FACETA, entry_type legado, COM aresta e key ok
             INSERT INTO memory_entries VALUES
                 ('lesson:facetado:2026-08-30','semantic','text',datetime('now'));
             INSERT INTO memory_tags VALUES ('lesson:facetado:2026-08-30','kind:lesson');
             INSERT INTO memory_links VALUES ('e1','lesson:facetado:2026-08-30','x',
                 'generated-by',datetime('now'));
             -- curado pela faceta, SEM aresta (conta no denominador, não no numerador)
             INSERT INTO memory_entries VALUES
                 ('decisao:orfa:2026-08-30','semantic','text',datetime('now'));
             INSERT INTO memory_tags VALUES ('decisao:orfa:2026-08-30','kind:decision');
             -- não-curado: fora das duas contagens
             INSERT INTO memory_entries VALUES
                 ('outcome:bash:x','episodic','outcome',datetime('now'));",
        )
        .expect("seed");
        drop(conn);
        assert_eq!(
            super::graph_contract_share(&dir),
            Some(0.5),
            "1 de 2 nós curados-por-faceta cumpre chave+aresta"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F2 ADW (28/08/2026) — o KPI lê o formato que o PRODUTOR grava:
    /// `plan_refine.py` escreve `{version, iterations: […]}`, e só o array cru
    /// era aceito — o KPI ficava STUB com ledger legítimo no disco.
    #[test]
    fn plan_refine_iters_reads_the_producers_object_format() {
        let dir = std::env::temp_dir().join(format!("kpi-refine-{}", std::process::id()));
        let plans = dir.join("docs").join("plans").join("bundle");
        std::fs::create_dir_all(&plans).expect("tempdir");
        std::fs::write(
            plans.join("strategy.refine.json"),
            r#"{"version": 1, "iterations": [{"iter": 1}, {"iter": 2}, {"iter": 3}]}"#,
        )
        .expect("write ledger");
        assert_eq!(super::adw_plan_refine_iters(&dir), Some(3.0));
        // o formato de array cru segue aceito
        std::fs::write(plans.join("raw.refine.json"), r#"[1, 2, 3, 4]"#).expect("write raw");
        assert_eq!(super::adw_plan_refine_iters(&dir), Some(4.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// M0 (29/08/2026) — the `:par` stamp is the only adoption signal:
    /// distinct run_ids count once, unstamped origins never count, and a
    /// missing journal is STUB (`None`), never zero.
    #[test]
    fn parallel_runs_counts_distinct_stamped_runs_only() {
        let dir = std::env::temp_dir().join(format!("kpi-par-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tempdir");
        let journal = dir.join("run_subcalls.jsonl");
        std::fs::write(
            &journal,
            concat!(
                r#"{"origin":"run-1:code:1:par","hook":"cli-memory-recall"}"#,
                "\n",
                r#"{"origin":"run-1:code:2:par","hook":"cli-index-find"}"#,
                "\n",
                r#"{"origin":"run-2:code:1:par","hook":"cli-memory-recall"}"#,
                "\n",
                r#"{"origin":"run-3:code:1","hook":"cli-memory-recall"}"#,
                "\n",
            ),
        )
        .expect("write journal");
        assert_eq!(
            super::code_mode_parallel_runs_from(&journal),
            Some(2.0),
            "run-1 counts once, run-3 has no :par stamp"
        );
        assert_eq!(
            super::code_mode_parallel_runs_from(&dir.join("missing.jsonl")),
            None,
            "no journal is STUB, never a measured zero"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// M0 (29/08/2026) — the `tier` KEY separates the eras: a line without it
    /// (pre-instrumentation) stays out of the denominator entirely, while
    /// `tier: null` is an agent that declared nothing and MUST lower the
    /// share — the ruler measures declared specialization, not runner
    /// version (the "adoption measures the channel" trap).
    #[test]
    fn tiered_share_null_lowers_and_missing_key_is_excluded() {
        let dir = std::env::temp_dir().join(format!("kpi-tier-{}", std::process::id()));
        let run = dir.join(".touring").join("adw-runs").join("r1");
        std::fs::create_dir_all(&run).expect("tempdir");
        let journal = run.join("journal.jsonl");
        // only a pre-instrumentation line (no tier key) → STUB, never "0% tiered"
        std::fs::write(
            &journal,
            concat!(r#"{"event":"node_started","type":"agent","node":"old"}"#, "\n"),
        )
        .expect("write journal");
        assert_eq!(
            super::adw_tiered_agent_share(&dir),
            None,
            "journals that predate the instrumentation must read as STUB"
        );
        std::fs::write(
            &journal,
            concat!(
                r#"{"event":"node_started","type":"agent","node":"old"}"#,
                "\n",
                r#"{"event":"node_started","type":"agent","node":"critic","tier":"mid"}"#,
                "\n",
                r#"{"event":"node_started","type":"agent","node":"bare","tier":null}"#,
                "\n",
                r#"{"event":"node_started","type":"code","node":"gate","tier":"mid"}"#,
                "\n",
            ),
        )
        .expect("write journal");
        assert_eq!(
            super::adw_tiered_agent_share(&dir),
            Some(0.5),
            "null tier enters the denominator only; non-agent lines never count"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// S3 — a fração da inspeção capturada como rajada.
    #[test]
    fn inspect_burst_share_is_denied_over_observed() {
        // a previsão da medição de 115 transcripts: ~77,5% em rajada
        let r = super::inspect_burst_share(775.0, 225.0).expect("há observação");
        assert!((r - 0.775).abs() < 1e-9, "{r}");
    }

    /// Ausência de sinal é DESCONHECIDO, nunca zero (Lei L2). Um gate que nunca
    /// falou e um gate que falou e não pegou nada são estados diferentes — o
    /// T3-B morreu justamente por essa confusão.
    #[test]
    fn no_inspection_observed_is_none_not_zero() {
        assert_eq!(super::inspect_burst_share(0.0, 0.0), None);
        assert_eq!(
            super::inspect_burst_share(0.0, 10.0),
            Some(0.0),
            "10 isoladas e nenhuma rajada é um zero MEDIDO"
        );
    }

    #[test]
    fn adoption_ratio_is_runs_over_bash_calls() {
        assert_eq!(code_mode_adoption(4.0, 100.0), Some(0.04));
        // Measured zero: Bash actions exist, no runs — honestly 0.0.
        assert_eq!(code_mode_adoption(0.0, 10.0), Some(0.0));
    }

    #[test]
    fn ratio_absent_reads_as_null_never_zero() {
        // No denominator observed → unknown, never a fabricated 0.0 (Lei L2).
        assert_eq!(code_mode_adoption(0.0, 0.0), None);
        assert_eq!(code_mode_adoption(3.0, 0.0), None);
    }

    #[test]
    fn project_slug_is_deterministic_and_collision_free() {
        let a = project_slug(std::path::Path::new("/home/g/.claude/rust"));
        assert_eq!(a, "home-g-.claude-rust");
        // Distinct roots sharing a basename must not collide (a plain
        // basename slug would map both to "rust").
        let b = project_slug(std::path::Path::new("/tmp/other/rust"));
        assert_ne!(a, b);
        assert!(!a.contains('/'));
    }

    #[test]
    fn build_ab_block_reads_persisted_attribution() {
        let dir = std::env::temp_dir().join(format!("kpi_ab_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(".ab-latest.json");
        std::fs::write(
            &p,
            r#"{"schema":"coupling-ab-v1","arm":"treatment","attributable":true,"verdict":"coupling_helps"}"#,
        )
        .unwrap();
        let block = build_ab_block(&p).expect("persisted A/B block should parse");
        assert_eq!(block["verdict"], "coupling_helps");
        assert_eq!(block["attributable"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_ab_block_absent_is_none() {
        let p = std::env::temp_dir().join("kpi_ab_definitely_absent_zzz.json");
        let _ = std::fs::remove_file(&p);
        assert!(build_ab_block(&p).is_none());
    }

    #[test]
    fn recommend_empty_when_all_signals_healthy() {
        let s = CouplingSignals {
            suggestion_uptake: Some(0.80),
            str_bytes_per_emit: Some(400.0),
            adoption_ratio: Some(0.90),
            health_delta_net: Some(5.0),
            ab_attributable: Some(true),
        };
        assert!(recommend_refinements(&s).is_empty());
    }

    #[test]
    fn recommend_ignores_absent_signals() {
        // All None → nothing to recommend (can't act on unobserved data).
        assert!(recommend_refinements(&CouplingSignals::default()).is_empty());
    }

    #[test]
    fn demotion_bump_zero_without_ab_confirmation() {
        // Low uptake but no causal confirmation → no demotion (the F7 A/B gate).
        assert_eq!(hint_demotion_bump(Some(0.10), None), 0.0);
        assert_eq!(hint_demotion_bump(Some(0.10), Some(false)), 0.0);
    }

    #[test]
    fn demotion_bump_zero_when_uptake_healthy() {
        assert_eq!(hint_demotion_bump(Some(0.80), Some(true)), 0.0);
        assert_eq!(hint_demotion_bump(None, Some(true)), 0.0);
    }

    #[test]
    fn demotion_bump_graduated_by_distance_below_floor() {
        // uptake 0.30 vs floor 0.40 → (0.10 * 0.5) = 0.05 bump.
        assert!((hint_demotion_bump(Some(0.30), Some(true)) - 0.05).abs() < 1e-6);
        // uptake 0.0 → (0.40 * 0.5) = 0.20 bump (under the 0.30 cap).
        assert!((hint_demotion_bump(Some(0.0), Some(true)) - 0.20).abs() < 1e-6);
    }

    #[test]
    fn recommend_demotes_hint_actionable_when_ab_confirmed() {
        let s = CouplingSignals {
            suggestion_uptake: Some(0.10),
            ab_attributable: Some(true),
            ..Default::default()
        };
        let acts = recommend_refinements(&s);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].kind, "demote_hint");
        assert!(
            acts[0].actionable,
            "A/B confirmed → actuator may auto-apply"
        );
    }

    #[test]
    fn recommend_advisory_only_when_ab_absent() {
        // Same low uptake, but no A/B run → recommend, do NOT auto-apply (the F7 gate).
        let s = CouplingSignals {
            suggestion_uptake: Some(0.10),
            ab_attributable: None,
            ..Default::default()
        };
        let acts = recommend_refinements(&s);
        assert_eq!(acts.len(), 1);
        assert!(!acts[0].actionable, "no A/B confirmation → advisory only");
    }

    #[test]
    fn recommend_advisory_when_ab_negative() {
        let s = CouplingSignals {
            adoption_ratio: Some(0.20),
            ab_attributable: Some(false),
            ..Default::default()
        };
        let acts = recommend_refinements(&s);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].kind, "promote_capability");
        assert!(!acts[0].actionable);
    }

    #[test]
    fn recommend_tighten_elision_and_drift_together() {
        let s = CouplingSignals {
            str_bytes_per_emit: Some(1200.0),
            health_delta_net: Some(-3.0),
            ab_attributable: Some(true),
            ..Default::default()
        };
        let kinds: Vec<_> = recommend_refinements(&s).iter().map(|a| a.kind).collect();
        assert!(kinds.contains(&"tighten_elision"));
        assert!(kinds.contains(&"alert_drift"));
    }

    #[test]
    fn check_threshold_gte_pass_and_fail() {
        assert_eq!(check_threshold(0.8, 0.5, "gte"), "PASS");
        assert_eq!(check_threshold(0.5, 0.5, "gte"), "PASS");
        assert_eq!(check_threshold(0.4, 0.5, "gte"), "FAIL");
    }
    #[test]
    fn check_threshold_lte_pass_and_fail() {
        assert_eq!(check_threshold(50.0, 100.0, "lte"), "PASS");
        assert_eq!(check_threshold(100.0, 100.0, "lte"), "PASS");
        assert_eq!(check_threshold(150.0, 100.0, "lte"), "FAIL");
    }
    #[test]
    fn check_threshold_eq_uses_epsilon() {
        assert_eq!(check_threshold(0.0, 0.0, "eq"), "PASS");
        assert_eq!(check_threshold(1.0, 1.0, "eq"), "PASS");
        assert_eq!(check_threshold(1.0, 0.0, "eq"), "FAIL");
    }
    #[test]
    fn check_threshold_unknown_direction_fails() {
        assert_eq!(check_threshold(1.0, 0.0, "lt"), "FAIL");
    }
    #[test]
    fn iso_date_2026_04_25() {
        let date = iso_date_from_unix(1_777_075_200);
        assert_eq!(date, "2026-04-25");
    }
    #[test]
    fn iso_date_unix_epoch() {
        assert_eq!(iso_date_from_unix(0), "1970-01-01");
    }
    #[test]
    fn json_value_as_f64_handles_numerics() {
        assert_eq!(json_value_as_f64(&json!(42)), Some(42.0));
        assert_eq!(json_value_as_f64(&json!(3.14)), Some(3.14));
        assert_eq!(json_value_as_f64(&json!(true)), Some(1.0));
        assert_eq!(json_value_as_f64(&json!(false)), Some(0.0));
        assert_eq!(json_value_as_f64(&json!("not a number")), None);
    }

    // MED-1 (28/08) — a régua de aderência, sobre dados sintéticos:
    // 4 runs (3 ok, 1 falha com failure_kind) e 1 par falha→retry <120s.
    #[test]
    fn med1_adherence_from_lines_agrega_e_conta_retry() {
        let journal = [
            r#"{"exit_code":0,"failure_kind":null,"language":"python","ts":1000}"#,
            r#"{"exit_code":1,"failure_kind":"proc-exit","language":"python","ts":1010}"#,
            // retry da mesma linguagem 30s depois → wasted
            r#"{"exit_code":0,"failure_kind":null,"language":"python","ts":1040}"#,
            // outra linguagem: não é retry do par anterior
            r#"{"exit_code":0,"failure_kind":null,"language":"bash","ts":2000}"#,
        ];
        let v = adherence_from_lines(journal.iter().copied());
        assert_eq!(v["runs_total"], json!(4));
        assert_eq!(v["runs_ok"], json!(3));
        assert_eq!(v["success_rate"], json!(0.75));
        assert_eq!(v["wasted_attempts_retry_pairs"], json!(1));
        assert_eq!(v["by_failure_kind"]["proc-exit"], json!(1));
        assert_eq!(v["by_language"]["python"], json!(3));
        assert_eq!(v["by_language"]["bash"], json!(1));
        // journal vazio: available com zeros, success_rate null (nunca inventa)
        let vazio = adherence_from_lines(std::iter::empty());
        assert_eq!(vazio["runs_total"], json!(0));
        assert_eq!(vazio["success_rate"], Value::Null);
    }
    #[test]
    fn yaml_round_trip_preserves_commitment() {
        let yaml = "version: '1.0'\nschema: kpi-v1\ncommitments:\n  - id: a.b\n    name: T\n    threshold: 1.0\n    direction: gte\n    source: 'daemon:x:/y'\n";
        let parsed: CommitmentsFile = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(parsed.commitments.len(), 1);
        assert_eq!(parsed.commitments[0].id, "a.b");
        assert_eq!(parsed.commitments[0].direction, "gte");
    }
    #[test]
    fn summarize_counts_buckets() {
        let checks = vec![
            mk_check("PASS"),
            mk_check("PASS"),
            mk_check("FAIL"),
            mk_check("STUB"),
        ];
        let s = summarize(&checks);
        assert_eq!(s["total"], 4);
        assert_eq!(s["passed"], 2);
        assert_eq!(s["failed"], 1);
        assert_eq!(s["stub"], 1);
    }
    #[test]
    fn external_source_returns_stub_marker() {
        assert!("external:cargo nextest --list-tests".starts_with("external:"));
    }
    #[test]
    fn default_commitments_path_returns_non_empty() {
        let p = default_commitments_path();
        assert!(!p.as_os_str().is_empty());
    }
    #[test]
    fn iso_date_handles_late_year() {
        assert_eq!(iso_date_from_unix(1_924_905_600), "2030-12-31");
    }
    fn mk_check(status: &'static str) -> CommitmentCheck {
        CommitmentCheck {
            id: "x".into(),
            name: "x".into(),
            threshold: 0.0,
            direction: "gte".into(),
            source_kind: "daemon",
            actual: Some(0.0),
            status,
            rationale: String::new(),
            advisory: false,
            stub_reason: None,
        }
    }
    #[test]
    fn summarize_counts_advisory_separately_from_failed() {
        let checks = vec![mk_check("PASS"), mk_check("ADVISORY"), mk_check("FAIL")];
        let s = summarize(&checks);
        assert_eq!(
            s["failed"], 1,
            "advisory must not inflate the failed bucket"
        );
        assert_eq!(s["advisory"], 1);
        assert_eq!(s["passed"], 1);
    }
    #[test]
    fn flow_compliance_filters_by_project_and_needs_data() {
        let dir = std::env::temp_dir().join(format!("kpi-flow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let log = dir.join("compliance.jsonl");
        let proj = std::path::Path::new("/proj/a");
        // No log yet → None (STUB, never a fabricated 0.0).
        assert_eq!(flow_compliance_from_log(&log, proj), None);
        std::fs::write(
            &log,
            concat!(
                "{\"cwd\":\"/proj/a\",\"flow\":\"strategy-outer\",\"complete\":true}\n",
                "{\"cwd\":\"/proj/a\",\"flow\":\"strategy-outer\",\"complete\":false}\n",
                "{\"cwd\":\"/proj/b\",\"flow\":\"cross-audit\",\"complete\":false}\n",
                "not-json\n",
            ),
        )
        .expect("write log");
        // Only /proj/a records count: 1 complete of 2 → 0.5; /proj/b is ignored.
        assert_eq!(flow_compliance_from_log(&log, proj), Some(0.5));
        assert_eq!(
            flow_compliance_from_log(&log, std::path::Path::new("/proj/c")),
            None,
            "a project with no evaluations stays STUB"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn commitment_advisory_defaults_false_and_parses_true() {
        let base = "version: '1.0'\nschema: kpi-v1\ncommitments:\n  - id: a.b\n    name: T\n    threshold: 1.0\n    direction: gte\n    source: 'daemon:x:/y'\n";
        let f: CommitmentsFile = serde_yaml::from_str(base).expect("parse base");
        assert!(!f.commitments[0].advisory, "advisory defaults to false");
        let adv = "version: '1.0'\nschema: kpi-v1\ncommitments:\n  - id: a.b\n    name: T\n    threshold: 1.0\n    direction: gte\n    source: 'derived:health_delta_net'\n    advisory: true\n";
        let f2: CommitmentsFile = serde_yaml::from_str(adv).expect("parse advisory");
        assert!(f2.commitments[0].advisory);
    }

    #[test]
    fn split_handler_scope_extracts_package() {
        assert_eq!(
            split_handler_scope("cli-gate-metrics"),
            ("cli-gate-metrics", None)
        );
        assert_eq!(
            split_handler_scope("cli-mutation-test@touring-identity"),
            ("cli-mutation-test", Some("touring-identity"))
        );
        assert_eq!(
            split_handler_scope("cli-x@"),
            ("cli-x@", None),
            "an empty scope is not a scope"
        );
    }

    #[test]
    fn resolve_external_reads_fresh_value_and_stubs_otherwise() {
        let dir = std::env::temp_dir().join(format!("kpi-external-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        assert_eq!(resolve_external(&dir, "a.b"), None, "missing file is STUB");
        std::fs::write(dir.join("a.b.json"), r#"{"value": 15786, "measured_at": "x"}"#)
            .expect("write fresh");
        assert_eq!(resolve_external(&dir, "a.b"), Some(15786.0));
        std::fs::write(dir.join("c.d.json"), "not json").expect("write malformed");
        assert_eq!(resolve_external(&dir, "c.d"), None, "malformed is STUB");
        std::fs::write(dir.join("e.f.json"), r#"{"measured_at": "x"}"#).expect("write valueless");
        assert_eq!(
            resolve_external(&dir, "e.f"),
            None,
            "a file without `value` is STUB, never a fabricated zero"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The three STUB causes are distinguishable facts with distinct remedies
    /// (analise-e0 review, 30/08/2026): "nobody measured" must never be
    /// confusable with "measured long ago" or "recorded wrong".
    #[test]
    fn an_external_stub_names_its_cause_and_the_remedy() {
        assert_eq!(external_verdict(0, Some("{\"value\": 3.5}")), Ok(3.5));
        assert_eq!(
            external_verdict(0, None),
            Err(ExternalStub::Missing),
            "unreadable content is an unknown, never zero"
        );
        assert_eq!(
            external_verdict(EXTERNAL_STALE_SECS + 86_400, Some("{\"value\": 1.0}")),
            Err(ExternalStub::Stale { days: 15 }),
            "a stale value must not masquerade as current even when parseable"
        );
        assert_eq!(
            external_verdict(0, Some("{\"measured_at\": \"2026-08-28\"}")),
            Err(ExternalStub::Malformed),
            "no `value` key at all and no explanation — that IS malformed"
        );
        // A3 (cross-audit 30/08): value:null + producer explanation is an
        // HONEST unknown — the dashboard repeats their words, never accuses.
        assert_eq!(
            external_verdict(
                0,
                Some("{\"value\": null, \"detalhe\": \"amostra n=6 abaixo do mínimo 20\"}")
            ),
            Err(ExternalStub::Declared {
                detalhe: "amostra n=6 abaixo do mínimo 20".into()
            })
        );
        assert_eq!(
            external_verdict(0, Some("{\"value\": null}")),
            Err(ExternalStub::Malformed),
            "value:null WITHOUT an explanation stays malformed — silence is not honesty"
        );
        let d = ExternalStub::Declared {
            detalhe: "amostra n=6".into(),
        }
        .teach("x");
        assert!(
            d.contains("declared unmeasured") && d.contains("amostra n=6"),
            "got: {d}"
        );
        // The message carries the remedy and the exact drop path (A5) — the
        // operator acts on the dashboard line alone.
        let m = ExternalStub::Missing.teach("touring.test.count");
        assert!(
            m.contains("never measured")
                && m.contains("docs/kpi/external/touring.test.count.json"),
            "got: {m}"
        );
        let s = ExternalStub::Stale { days: 15 }.teach("x");
        assert!(s.contains("15d ago"), "got: {s}");
        assert!(ExternalStub::Malformed.teach("x").contains("numeric `value`"));
    }

    /// Only an `external:` source that resolved to no value carries a reason —
    /// non-external STUBs and resolved values keep the old payload shape.
    #[test]
    fn only_an_external_stub_carries_a_reason() {
        let dir = std::env::temp_dir().join("touring-kpi-stub-reason-test");
        std::fs::create_dir_all(&dir).ok();
        assert_eq!(
            external_stub_reason("derived", None, &dir, "x"),
            None,
            "a non-external STUB explains nothing new"
        );
        assert_eq!(
            external_stub_reason("external", Some(1.0), &dir, "x"),
            None,
            "a resolved value needs no excuse"
        );
        let reason = external_stub_reason("external", None, &dir, "kpi-stub-nonexistent")
            .expect("a missing measurement must name its cause");
        assert!(reason.contains("never measured"), "got: {reason}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The structural guard the 2026-08-28 audit found missing: every source
    /// the contract declares must have a matching arm in this file, and every
    /// derived arm must be declared by the contract. `cli-mutation-test` sat
    /// in the YAML with no `invoke_handler` arm (permanently STUB), and the
    /// `inspect_burst_share` arm sat here with no commitment (never shown) —
    /// declaration and executor must derive from the same source (D8).
    #[test]
    fn every_declared_source_has_an_arm_and_every_derived_arm_a_commitment() {
        let yaml_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/kpi/commitments.yaml");
        let Ok(yaml) = std::fs::read_to_string(&yaml_path) else {
            eprintln!(
                "skip: {} not present (out-of-repo build)",
                yaml_path.display()
            );
            return;
        };
        let src = include_str!("kpi.rs");
        // A region runs from its top-level `fn` line to the next top-level fn.
        let region = |start: &str| -> String {
            let mut out = String::new();
            let mut inside = false;
            for line in src.lines() {
                if line.starts_with(start) {
                    inside = true;
                } else if inside && line.starts_with("fn ") {
                    break;
                }
                if inside {
                    out.push_str(line);
                    out.push('\n');
                }
            }
            out
        };
        let invoke = region("fn invoke_handler");
        let derived_region = region("fn resolve_derived");
        assert!(
            !invoke.is_empty() && !derived_region.is_empty(),
            "invoke_handler / resolve_derived regions must be locatable"
        );

        let mut daemon_handlers: Vec<String> = Vec::new();
        let mut derived_names: Vec<String> = Vec::new();
        let mut external_ids: Vec<String> = Vec::new();
        let mut last_id = String::new();
        for line in yaml.lines() {
            let t = line.trim();
            if let Some(id) = t.strip_prefix("- id: ") {
                last_id = id.trim().to_string();
                continue;
            }
            let Some(rest) = t.strip_prefix("source: \"") else {
                continue;
            };
            let Some(end) = rest.rfind('"') else { continue };
            let source = &rest[..end];
            if let Some(r) = source.strip_prefix("daemon:") {
                let handler = r.split(':').next().unwrap_or("");
                let (name, _) = split_handler_scope(handler);
                daemon_handlers.push(name.to_string());
            } else if let Some(r) = source.strip_prefix("derived:") {
                derived_names.push(r.to_string());
            } else if source.starts_with("external:") {
                // The operator learns WHERE to drop the measurement from the
                // source line itself; an id×path drift leaves the KPI STUB
                // forever while the operator writes to the wrong file.
                assert!(
                    source.contains(&format!("docs/kpi/external/{last_id}.json")),
                    "external source for `{last_id}` does not name its drop path docs/kpi/external/{last_id}.json — the operator cannot know where to record the measurement"
                );
                external_ids.push(last_id.clone());
            }
        }
        assert!(
            !daemon_handlers.is_empty() && !derived_names.is_empty(),
            "commitments.yaml parsed no sources — the guard is not seeing the contract"
        );
        for h in &daemon_handlers {
            assert!(
                invoke.contains(&format!("\"{h}\"")),
                "daemon source `{h}` declared in commitments.yaml has no invoke_handler arm — it can only ever be STUB"
            );
        }
        for d in &derived_names {
            assert!(
                derived_region.contains(&format!("\"{d}\"")),
                "derived source `{d}` declared in commitments.yaml has no resolve_derived arm — it can only ever be STUB"
            );
        }
        // Inverse direction: an arm nothing declares is dead code no KPI shows.
        for line in derived_region.lines() {
            let t = line.trim_start();
            if !t.starts_with('"') || !t.contains("=>") {
                continue;
            }
            let Some(name) = t.trim_start_matches('"').split('"').next() else {
                continue;
            };
            if !name.is_empty() {
                assert!(
                    derived_names.iter().any(|d| d == name),
                    "resolve_derived arm `{name}` has no commitment in commitments.yaml — an orphan no `touring kpi` output ever shows"
                );
            }
        }

        // `external:` arm (the gap the analise-e0 review named, 30/08/2026):
        // the resolver is generic so there is no per-id arm to demand, but the
        // parse must be SEEN working (an id-format drift would empty this vec
        // silently and turn the path assertions above into no-ops)…
        assert!(
            !external_ids.is_empty(),
            "commitments.yaml declares no parseable external: sources — the guard is not seeing the contract"
        );
        // …and the inverse direction holds like the others: a measurement file
        // nothing declares is an orphan no `touring kpi` output ever shows.
        if let Some(dir) = yaml_path.parent().map(|p| p.join("external"))
            && let Ok(entries) = std::fs::read_dir(&dir)
        {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let Some(id) = name.strip_suffix(".json") else {
                    continue;
                };
                assert!(
                    external_ids.iter().any(|e| e == id),
                    "external measurement `{name}` has no commitment in commitments.yaml — an orphan no `touring kpi` output ever shows"
                );
            }
        }
    }
}
