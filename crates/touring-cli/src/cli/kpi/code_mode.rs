//! Reguas do code mode
//!
//! Extraido de `cli/kpi.rs` em 21/09/2026: o arquivo tinha 2931 linhas e
//! reprovava F1.2 (manutenibilidade) desde antes deste trabalho. O corte e por
//! COESAO — este modulo guarda as medidas do code mode: economia, adocao, rajada de
//! inspecao, aderencia, reuso da escada e uso do sinal do SDK.

use super::*;
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
pub(crate) fn code_mode_economy_ratio(rt: &HookRuntime) -> Option<f64> {
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
pub(crate) fn code_mode_arm_rate(rt: &HookRuntime, arm: &str) -> Option<f64> {
    let path = rt.project_root.join(".claude/touring/code_mode_arm.json");
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
pub(crate) fn code_mode_adoption(runs: f64, bash_calls: f64) -> Option<f64> {
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
pub(crate) fn inspect_burst_share(denied: f64, first_passed: f64) -> Option<f64> {
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
pub(crate) fn policy_discrimination(project_root: &Path) -> Option<f64> {
    let db = touring_foundation::TouringConfig::graph_db_canonical(project_root);
    let conn =
        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
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
pub(crate) fn graph_contract_share(project_root: &Path) -> Option<f64> {
    use touring_intelligence::rl::memory::tags;
    let db = touring_foundation::TouringConfig::memory_db_canonical(project_root);
    let conn =
        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
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
pub(crate) fn memory_edge_density(project_root: &Path) -> Option<f64> {
    let db = touring_foundation::TouringConfig::memory_db_canonical(project_root);
    let conn =
        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
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

pub(crate) fn default_commitments_path() -> PathBuf {
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

pub(crate) fn load_commitments(path: &PathBuf) -> std::io::Result<CommitmentsFile> {
    let raw = std::fs::read_to_string(path)?;
    serde_yaml::from_str::<CommitmentsFile>(&raw)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

pub(crate) fn today_iso() -> String {
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
pub(crate) fn code_mode_adherence() -> Value {
    let Some(home) = std::env::var_os("HOME") else {
        return json!({"available": false, "reason": "HOME unset"});
    };
    let path = PathBuf::from(home).join(".claude/touring/run_journal.jsonl");
    match std::fs::read_to_string(&path) {
        Ok(content) => adherence_from_lines(content.lines()),
        Err(_) => json!({"available": false, "reason": "no journal yet"}),
    }
}

/// C4 — piso da razão de reuso; abaixo dele o problema é de DESCOBERTA
/// (busca por intenção), não de persistência (canvas 02/09, §9c).
pub(crate) const CODE_MODE_REUSE_FLOOR: f64 = 0.20;

/// C4 (2026-09-02) — reuse ruler over the durable journal (B2 fields), read
/// together with the trust ladder.
///
/// The journal alone cannot answer the question: its `harvest` field is set
/// only by an EXPLICIT `--harvest <slug>`, which happened 0 times in 16.205
/// lines, while the executor had silently enrolled 369 bodies in the ladder
/// (measured 19/09/2026). A ruler blind to the rail it is measuring reports a
/// zero that is not there.
pub(crate) fn code_mode_reuse(project_root: &Path) -> Value {
    let Some(home) = std::env::var_os("HOME") else {
        return json!({"available": false, "reason": "HOME unset"});
    };
    let path = touring_code::journal::default_journal_path(&PathBuf::from(home));
    let ladder = ladder_totals_of(project_root);
    match touring_code::journal::read_journal(&path) {
        Ok(agg) => reuse_from_aggregate(&agg, ladder),
        Err(_) => json!({"available": false, "reason": "no journal yet"}),
    }
}

/// The ladder's three numbers, or zeros when it cannot be read.
///
/// Fail-soft on purpose: a missing memory db means "nothing reused yet", never
/// a KPI that refuses to report.
pub(crate) fn ladder_totals_of(
    project_root: &Path,
) -> touring_intelligence::rl::memory::snippet_stats::LadderTotals {
    use touring_intelligence::rl::memory::snippet_stats;
    let db = touring_foundation::TouringConfig::memory_db_canonical(project_root);
    if !db.exists() {
        return snippet_stats::LadderTotals::default();
    }
    rusqlite::Connection::open_with_flags(
        &db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
    .and_then(|conn| snippet_stats::ladder_totals(&conn).ok())
    .unwrap_or_default()
}

/// C4 — the pure aggregation behind [`code_mode_reuse`], testable without FS.
///
/// `v2_runs` = runs that declared an origin (`file` | `inline`); v1 lines are
/// counted nowhere (E4: shown as `v1_runs`, never folded into the ratio).
///
/// `reused` = runs from a PERSISTENT script (a file outside the harness
/// scratchpad) + runs that RE-RAN a body already on the trust ladder. A
/// scratchpad script is one-off by construction — per-session tmpfs — so it
/// never counts, and neither does ENROLLING a body: the first run of something
/// is not a reuse of it. That distinction is the correction of 19/09/2026;
/// before it the numerator added `harvested_runs`, which counts explicit
/// `--harvest` slugs (0 of 16.205 lines) and would have counted a first-time
/// persist as reuse had anyone ever typed it. `ladder_enrolled` stays visible
/// so the gap between "the library fills" and "the library is read" is legible
/// instead of hidden (E4).
pub(crate) fn reuse_from_aggregate(
    agg: &touring_code::journal::JournalAggregate,
    ladder: touring_intelligence::rl::memory::snippet_stats::LadderTotals,
) -> Value {
    let v2_runs = agg.file_runs + agg.inline_runs;
    let persistent_file_runs = agg.file_runs.saturating_sub(agg.scratch_file_runs);
    let reused = persistent_file_runs + ladder.reused;
    let ratio = if v2_runs == 0 {
        0.0
    } else {
        reused as f64 / v2_runs as f64
    };
    // N6 (2026-09-23): hint_coverage — in what share of runs did the library
    // OFFER a similar block at run time. It is the denominator the reuse
    // question was missing: `reuse_ratio` low with `hint_coverage` high means
    // the offer exists and is being ignored (discovery); low with low means
    // the library genuinely has nothing to offer (absence). The `hint` field
    // is journaled only by binaries built after 2026-09-23, so a zero here
    // reads "not yet measured" until the window fills — never "no offers".
    let hint_coverage = if v2_runs == 0 {
        0.0
    } else {
        agg.hinted_runs as f64 / v2_runs as f64
    };
    json!({
        "available": v2_runs > 0,
        "v1_runs": agg.total_entries.saturating_sub(v2_runs),
        "v2_runs": v2_runs,
        "file_runs": agg.file_runs,
        "scratch_file_runs": agg.scratch_file_runs,
        "persistent_file_runs": persistent_file_runs,
        "inline_runs": agg.inline_runs,
        "harvested_runs": agg.harvested_runs,
        // The ladder, reported apart from the ratio: how many distinct bodies
        // the executor kept, and how many runs actually re-ran one. A large
        // `ladder_enrolled` beside a near-zero `ladder_reused` is the shape of
        // a library that fills and is never read — which is DISCOVERY, not
        // persistence, and is what the floor is really failing for.
        "ladder_enrolled": ladder.entries,
        "ladder_reused": ladder.reused,
        "orchestrate_runs": agg.orchestrate_runs,
        "brief_runs": agg.brief_runs,
        "hinted_runs": agg.hinted_runs,
        "hint_coverage": (hint_coverage * 1000.0).round() / 1000.0,
        "total_tmp_bytes": agg.total_tmp_bytes,
        "reuse_ratio": (ratio * 1000.0).round() / 1000.0,
        "floor": CODE_MODE_REUSE_FLOOR,
        "status": if v2_runs == 0 { "STUB" } else if ratio >= CODE_MODE_REUSE_FLOOR { "PASS" } else { "FAIL" },
    })
}

#[cfg(test)]
mod reuse_tests {
    use super::*;
    use touring_code::journal::JournalAggregate;

    use touring_intelligence::rl::memory::snippet_stats::LadderTotals;

    /// C4: two persistent-file runs + one ladder RE-run over 10 v2 runs = 0.3
    /// (PASS); 5 scratch-file runs count as one-offs; v1 lines are shown apart
    /// and never enter the ratio.
    #[test]
    fn reuse_ratio_counts_persistent_scripts_and_ladder_rereuns() {
        let agg = JournalAggregate {
            total_entries: 14,
            file_runs: 7,
            scratch_file_runs: 5,
            inline_runs: 3,
            hinted_runs: 5,
            ..Default::default()
        };
        let ladder = LadderTotals {
            entries: 4,
            executions: 5,
            reused: 1,
        };
        let v = reuse_from_aggregate(&agg, ladder);
        assert_eq!(v["v2_runs"], 10);
        assert_eq!(v["v1_runs"], 4);
        assert_eq!(v["persistent_file_runs"], 2);
        assert_eq!(v["ladder_reused"], 1);
        assert_eq!(v["reuse_ratio"], 0.3);
        assert_eq!(v["status"], "PASS");
        assert_eq!(v["available"], true);
        // N6: 5 of 10 v2 runs had a similar block on the ladder → coverage 0.5.
        assert_eq!(v["hinted_runs"], 5);
        assert_eq!(v["hint_coverage"], 0.5);
    }

    /// Enrolling is not reusing. A ladder full of bodies that each ran ONCE
    /// adds nothing to the numerator — the shape measured on 19/09/2026 (369
    /// enrolled, 364 of them run once) must read as FAIL, not as success.
    #[test]
    fn enrolling_a_body_is_not_reusing_it() {
        let agg = JournalAggregate {
            total_entries: 100,
            file_runs: 10,
            scratch_file_runs: 10,
            inline_runs: 90,
            ..Default::default()
        };
        let so_far_unread = LadderTotals {
            entries: 369,
            executions: 369,
            reused: 0,
        };
        let v = reuse_from_aggregate(&agg, so_far_unread);
        assert_eq!(v["ladder_enrolled"], 369, "the enrolment stays visible");
        assert_eq!(v["ladder_reused"], 0);
        assert_eq!(v["reuse_ratio"], 0.0);
        assert_eq!(
            v["status"], "FAIL",
            "a library that fills and is never read must not report PASS"
        );
    }

    #[test]
    fn reuse_ratio_is_a_stub_without_v2_runs_and_fails_below_the_floor() {
        let empty = reuse_from_aggregate(&JournalAggregate::default(), LadderTotals::default());
        assert_eq!(empty["status"], "STUB");
        assert_eq!(empty["available"], false);
        let low = reuse_from_aggregate(
            &JournalAggregate {
                total_entries: 20,
                file_runs: 20,
                scratch_file_runs: 19,
                ..Default::default()
            },
            LadderTotals::default(),
        );
        assert_eq!(low["reuse_ratio"], 0.05);
        assert_eq!(low["status"], "FAIL");
    }
}

/// F6 — aggregate per-canonical-hook stats from the post-tool-use mirror.
/// Returns `(used, total)` where `total = 8` mirrors `HookName::ALL.len()`.
/// Fail-open: missing file → `(0, 8)` (consistent with the gate).
/// D1 (2026-09-02) — o sinal vivo comparado com um relatório PERSISTIDO.
///
/// `code_mode_signal_use` lê o espelho do processo corrente e responde "quantos
/// hooks estão em uso agora". Um baseline em disco responde a pergunta que só o
/// tempo faz: a adoção caiu, a latência subiu? É o consumidor que
/// [`touring_code::sdk::load_signal_report`] documentava e não tinha.
///
/// Falha SEMPRE de forma legível: um caminho ausente ou um JSON inválido viram
/// um campo `error` no payload, nunca um KPI mudo.
pub(crate) fn signal_baseline(path: &str) -> Value {
    match touring_code::sdk::load_signal_report(std::path::Path::new(path)) {
        Ok(report) => {
            let canonical: std::collections::BTreeSet<&'static str> =
                touring_code::sdk::HookName::ALL
                    .iter()
                    .map(|h| h.as_str())
                    .collect();
            let used = report
                .hooks
                .keys()
                .filter(|k| canonical.contains(k.as_str()))
                .count() as u64;
            // O pior p99 entre os hooks é a leitura honesta de "quanto custa o
            // caminho mais lento"; uma média entre hooks de volumes diferentes
            // esconderia exatamente o que se quer vigiar.
            let worst_p99 = report
                .hooks
                .values()
                .map(|h| h.duration_ms_p99)
                .max()
                .unwrap_or(0);
            let worst_p50 = report
                .hooks
                .values()
                .map(|h| h.duration_ms_p50)
                .max()
                .unwrap_or(0);
            json!({
                "source": path,
                "generated_at_unix": report.generated_at_unix,
                "total_runs": report.total_runs,
                "used": used,
                "total": touring_code::sdk::HookName::ALL.len() as u64,
                "worst_duration_ms_p50": worst_p50,
                "worst_duration_ms_p99": worst_p99,
            })
        }
        Err(e) => json!({"source": path, "error": e.to_string()}),
    }
}

/// O delta entre o sinal vivo e o baseline, quando ambos são legíveis.
///
/// Só campos comparáveis entram: `used` e o pior p99. Um delta negativo em
/// `used` é regressão de adoção; positivo em `worst_duration_ms_p99` é
/// regressão de custo.
pub(crate) fn signal_delta(live: &Value, baseline: &Value) -> Value {
    let gi = |v: &Value, k: &str| v.get(k).and_then(Value::as_i64);
    match (gi(live, "used"), gi(baseline, "used")) {
        (Some(l), Some(b)) => json!({
            "used": l - b,
            "worst_duration_ms_p99": gi(live, "duration_ms_p99").unwrap_or(0)
                - gi(baseline, "worst_duration_ms_p99").unwrap_or(0),
        }),
        _ => Value::Null,
    }
}

pub(crate) fn code_mode_signal_use() -> Value {
    let Some(home) = std::env::var_os("HOME") else {
        return json!({"available": false, "reason": "HOME unset"});
    };
    // F4 P4 (2026-09-01) — same source the writers use (`default_mirror_path`),
    // so reader and sinks cannot drift apart on the path.
    let path = touring_code::sdk_signal_mirror::default_mirror_path(&PathBuf::from(home));
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let mut out = signal_use_from_lines(content.lines());
            // REGRA #0 (2026-09-02) — `duration_p50`/`duration_p99` do agregado
            // do mirror não tinham UM consumidor. O consumidor natural é este
            // KPI, que já lia o mesmo arquivo e descartava a duração de cada
            // entrada: um contador diz se os hooks são usados, o percentil diz
            // se valem o que custam.
            if let Ok(agg) = touring_code::sdk_signal_mirror::read(&path) {
                out["duration_ms_p50"] = json!(agg.duration_p50());
                out["duration_ms_p99"] = json!(agg.duration_p99());
            }
            out
        }
        Err(_) => json!({"available": false, "reason": "no mirror yet"}),
    }
}

/// F0 wave signal-layer-tier-ab (01/09) — a agregação pura por trás de
/// [`code_mode_signal_use`], testável sem FS. `used` cruza com os 8 canônicos
/// de `HookName::ALL`; nomes fora do cânone (alias `cli-*` do daemon) contam
/// em `non_canonical_calls` — visíveis, nunca somados ao ratio (E4: a
/// ausência/anomalia é exibida, não escondida). Medido 01/09: sem o filtro o
/// ratio leu 1.0 com só 3 hooks canônicos no mirror.
pub(crate) fn signal_use_from_lines<'a>(lines: impl Iterator<Item = &'a str>) -> Value {
    const TOTAL_HOOKS: u64 = 8;
    let canonical: std::collections::BTreeSet<&'static str> = touring_code::sdk::HookName::ALL
        .iter()
        .map(|h| h.as_str())
        .collect();
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut calls = 0u64;
    let mut non_canonical = 0u64;
    for line in lines {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(name) = v.get("hook_name").and_then(|x| x.as_str()) {
            calls += 1;
            if canonical.contains(name) {
                seen.insert(name.to_string());
            } else {
                non_canonical += 1;
            }
        }
    }
    json!({
        "available": true,
        "used": seen.len() as u64,
        "total": TOTAL_HOOKS,
        "ratio": seen.len() as f64 / TOTAL_HOOKS as f64,
        "total_calls": calls,
        "non_canonical_calls": non_canonical,
    })
}

/// F9 (2026-09-01) — régua do complemento de hooks.
///
/// Lado daemon: `hook_dispatch_by_name` (F0.3d) diz quantas vezes cada hook
/// foi despachado desde `hook_dispatch_since_epoch`. Lado sinal: o mirror
/// (`sdk_signal_mirror.jsonl`) diz quantas entregas canônicas chegaram no
/// MESMO intervalo. A razão `post_bash_delivery_ratio` é o número que a sonda
/// F0.3 não tinha: se post-bash quase não é despachado enquanto o Claude Code
/// emite PostToolUse, o thin client não chega ao daemon; se é despachado e o
/// mirror não cresce, o feeder/classificador é o suspeito. `cli_kpi` roda
/// dentro do daemon, então os contadores lidos são os do processo vivo.
pub(crate) fn hooks_complement() -> Value {
    let by_name = touring_foundation::gate_metrics::hook_dispatch_by_name();
    let since = touring_foundation::gate_metrics::hook_dispatch_since_epoch();
    let mirror = std::env::var_os("HOME")
        .map(|home| touring_code::sdk_signal_mirror::default_mirror_path(&PathBuf::from(home)))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    hooks_complement_from(&by_name, mirror.lines(), since)
}

/// A agregação pura por trás de [`hooks_complement`] — testável sem FS nem
/// daemon. Linhas do mirror anteriores a `since_epoch` ficam fora (o mirror é
/// cumulativo, os contadores do daemon não); nomes fora de `HookName::ALL`
/// contam à parte (E4: visíveis, nunca somados). Sem despacho ainda, a razão
/// não existe — e diz isso em vez de dividir por zero.
pub(crate) fn hooks_complement_from<'a>(
    by_name: &std::collections::BTreeMap<String, u64>,
    mirror_lines: impl Iterator<Item = &'a str>,
    since_epoch: Option<u64>,
) -> Value {
    let Some(since) = since_epoch else {
        return json!({
            "available": false,
            "reason": "no hook dispatched yet in this daemon",
        });
    };
    let canonical: std::collections::BTreeSet<&'static str> = touring_code::sdk::HookName::ALL
        .iter()
        .map(|h| h.as_str())
        .collect();
    let mut deliveries = 0u64;
    let mut non_canonical = 0u64;
    // F9-origem (02/09): cada linha canônica é creditada a quem a escreveu
    // (`origin`: `post_bash` | `sdk`; ausente = `unknown`, linhas legadas).
    let mut by_origin: std::collections::BTreeMap<&'static str, u64> =
        [("post_bash", 0u64), ("sdk", 0u64), ("unknown", 0u64)]
            .into_iter()
            .collect();
    for line in mirror_lines {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let ts = v.get("ts").and_then(Value::as_u64).unwrap_or(0);
        if ts < since {
            continue;
        }
        match v.get("hook_name").and_then(Value::as_str) {
            Some(name) if canonical.contains(name) => {
                deliveries += 1;
                let origin = match v.get("origin").and_then(Value::as_str) {
                    Some("post_bash") => "post_bash",
                    Some("sdk") => "sdk",
                    _ => "unknown",
                };
                *by_origin.entry(origin).or_insert(0) += 1;
            }
            Some(_) => non_canonical += 1,
            None => {}
        }
    }
    let post_bash = by_name.get("post-bash").copied().unwrap_or(0);
    let post_bash_deliveries = by_origin.get("post_bash").copied().unwrap_or(0);
    // A razão só admite entregas ORIGINADAS no post-bash: uma entrega do SDK
    // in-sandbox não prova que o PostToolUse chegou ao daemon.
    let ratio = if post_bash > 0 {
        json!(post_bash_deliveries as f64 / post_bash as f64)
    } else {
        Value::Null
    };
    json!({
        "available": true,
        "since_epoch": since,
        "dispatched": by_name,
        "post_bash_dispatched": post_bash,
        "mirror_deliveries_since": deliveries,
        "mirror_deliveries_by_origin": by_origin,
        "post_bash_origin_deliveries_since": post_bash_deliveries,
        "mirror_non_canonical_since": non_canonical,
        "post_bash_delivery_ratio": ratio,
    })
}

/// A agregação pura por trás de [`code_mode_adherence`] — testável sem FS.
pub(crate) fn adherence_from_lines<'a>(lines: impl Iterator<Item = &'a str>) -> Value {
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
        // A classe passa pela taxonomia canônica (`FailureKind`) antes de virar
        // balde: string crua fazia uma grafia desconhecida criar categoria nova.
        // A classe passa pela taxonomia canônica (`FailureKind`) antes de virar
        // balde: string crua fazia uma grafia desconhecida criar categoria nova.
        // A classe passa pela taxonomia canônica (`FailureKind`) antes de virar
        // balde: string crua fazia uma grafia desconhecida criar categoria nova.
        if let Some(fk) = v
            .get("failure_kind")
            .and_then(Value::as_str)
            .map(|s| touring_code::journal::FailureKind::from_str_opt(Some(s)).as_str())
        {
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

pub(crate) fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
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

