//! Resolucao da fonte de cada compromisso
//!
//! Extraido de `cli/kpi.rs` em 21/09/2026: o arquivo tinha 2931 linhas e
//! reprovava F1.2 (manutenibilidade) desde antes deste trabalho. O corte e por
//! COESAO — este modulo guarda de onde sai o valor de um compromisso: fonte declarada,
//! lente externa, derivacao e o veredito quando nada responde.

use super::*;
pub(crate) fn resolve_source(
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
pub(crate) const EXTERNAL_STALE_SECS: u64 = 14 * 24 * 3600;

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
pub(crate) fn resolve_external(external_dir: &std::path::Path, id: &str) -> Option<f64> {
    resolve_external_detailed(external_dir, id).ok()
}

/// Why an `external:` commitment resolved to no value. The three causes have
/// three different remedies, so the dashboard names which one it is instead
/// of a bare STUB — otherwise "nobody measured" is indistinguishable from
/// "measured long ago", the very silence that kept `test.count` /
/// `coverage.line` STUB until 2026-08-28 (see [`resolve_external`]'s history).
#[derive(Debug, PartialEq)]
pub(crate) enum ExternalStub {
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
    pub(crate) fn teach(&self, id: &str) -> String {
        match self {
            Self::Missing => format!(
                "never measured — run the `source` command and drop docs/kpi/external/{id}.json with a numeric `value` field"
            ),
            Self::Stale { days } => format!(
                "stale — measured {days}d ago (window 14d); re-run the `source` command to refresh docs/kpi/external/{id}.json"
            ),
            Self::Declared { detalhe } => {
                format!("declared unmeasured by the producer — {detalhe}")
            }
            Self::Malformed => format!(
                "malformed — docs/kpi/external/{id}.json exists but has no numeric `value` field"
            ),
        }
    }
}

/// Pure verdict over the gathered facts (file age + raw content) — testable
/// without a filesystem, the same shape as [`adherence_from_lines`].
pub(crate) fn external_verdict(age_secs: u64, raw: Option<&str>) -> Result<f64, ExternalStub> {
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

pub(crate) fn resolve_external_detailed(
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
pub(crate) fn external_stub_reason(
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
pub(crate) fn resolve_derived(rt: &mut HookRuntime, name: &str) -> Option<f64> {
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
            let pending = crate::cli::learning::replay_corpus_pending(&rt.project_root, last_rowid);
            match (replayed_total, pending) {
                (0, None) => None,
                (r, p) => {
                    let total = r as f64 + p.unwrap_or(0).max(0) as f64;
                    if total <= 0.0 {
                        None
                    } else {
                        Some(r as f64 / total)
                    }
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
        // N1 (2026-09-23): started-conditional + 14d window + per-flow split —
        // post-Opção-C (03/09) an armed-and-abandoned flow is policy-allowed,
        // not a violation, so it leaves the denominator and shows up in
        // `flow_arm_yield` instead of dragging compliance.
        "flow_compliance" => flow_compliance_ratio(&rt.project_root),
        "flow_compliance_work_outer" => flow_compliance_flow(&rt.project_root, "work-outer"),
        "flow_compliance_strategy_outer" => {
            flow_compliance_flow(&rt.project_root, "strategy-outer")
        }
        "flow_compliance_cross_audit" => flow_compliance_flow(&rt.project_root, "cross-audit"),
        "flow_arm_yield" => flow_arm_yield_ratio(&rt.project_root),
        _ => None,
    }
}

