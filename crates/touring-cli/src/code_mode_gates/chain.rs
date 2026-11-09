//! The chain: one tool event in, the strictest applicable verdict out.

use serde_json::Value;
use std::path::Path;

use super::*;
use crate::workflow::exit_code_through_pipe;

/// S5 — the write→run pair gate. `None` = no decision (the flow continues).
/// The body, with the overrides injected (see `GateConfig`).
pub(super) fn write_run_pair_gate_with(
    st: &GateState,
    project_root: &Path,
    session: &str,
    cmd: &str,
    ovr: &GateConfig,
) -> Option<Verdict> {
    use crate::shared::gate_metrics::{GateEvent, GateId, record_gate_event};
    // 1 pair per command, even with multiple runs in the tool_use: what is counted
    // is the STEP of the execute-observe loop, not the number of interpretations.
    let path = script_run_targets(cmd).into_iter().find(|p| {
        st.written_scripts_ledger
            .get(&write_run_key(project_root, session, p))
            .is_some()
    })?;
    let pkey = write_run_key(project_root, session, "\u{0}write-run-pares");
    let (n, mut paths) = st.write_run_pair_ledger.get(&pkey).unwrap_or_default();
    let n = n + 1;
    if paths.len() < 8 {
        paths.push(path.clone());
    }
    if n < WRITE_RUN_DENY_AT || ovr.code_gates_disabled {
        st.write_run_pair_ledger.insert(pkey, (n, paths));
        return None;
    }
    st.write_run_pair_ledger.invalidate(&pkey);
    record_gate_event(GateId::G10, GateEvent::Denied);
    crate::shared::gate_metrics::record_g10_write_run_pair_denied();
    st.pending_g10.insert(session.to_string(), ());
    // `--lang` is mandatory in the CLI: without it the emitted route DOES NOT RUN
    // (measured by the peer analise-a2, 30/08 — clap rejects it and the deny becomes an
    // opaque error, the E4 anti-pattern committed by the enforcement itself). is_script_path
    // only accepts .py/.sh, so the else is python.
    let lang = if path.ends_with(".sh") {
        "bash"
    } else {
        "python"
    };
    // G10ROTA (30.4.97): the script is ALREADY on disk — the executor judges its CONTENT. A script
    // that writes does not run under Sandboxed (X6 `file-write`), and the named `--file` route would fail
    // exactly like that of the exec-burst; the question belongs to the executor (D8), as there.
    let rota = g10_script_route(project_root, lang, &path);
    if !rota.runs() {
        return Some(Verdict::deny(
            Gate::G10,
            format!(
                "[G10 write→run] {n}º script escrito-e-executado na janela de \
             {WRITE_RUN_WINDOW_SECS}s (`{path}`).{} {}",
                rota.explanation(project_root),
                bypass_next_step(cmd)
            ),
        ));
    }
    let comando = format!(
        "{}touring run {}--lang {lang} --file {path} --timeout-ms 60000",
        rota.env_prefix(),
        rota.run_flags()
    );
    let porque = rota.explanation(project_root);
    Some(Verdict::deny(
        Gate::G10,
        format!(
            "[G10 write→run] {n}º script escrito-e-executado na janela de \
         {WRITE_RUN_WINDOW_SECS}s (`{path}`) — o loop execute-observe manual \
         paga 2 round-trips por passo; o script JÁ está em disco, rode o MESMO \
         arquivo {onde} sem reescrever nada:{porque}\n  \
         {comando}\n\
         O sandbox devolve só o que o script imprime (digest no contexto, \
         íntegra no spill), e um `touring run` zera a janela. Bypass \
         por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed).",
            onde = if matches!(rota, g10_route::Route::Sandboxed) {
                "no sandbox"
            } else {
                "na rota que o executor aceita"
            }
        ),
    ))
}

/// The G11 decision on a bypass that has JUST been charged.
///
/// `None` lets it through. `Some(rule)` denies, and the named rule goes into the deny reason —
/// a mute gate degrades the retry (the lesson of the talking gates, A5).
#[must_use]
pub(crate) fn bypass_verdict(budget: BypassBudget) -> Option<&'static str> {
    if budget.strict_run > G11_MAX_STRICT_RUN {
        return Some("dois bypasses seguidos");
    }
    if budget.since_route > G11_MAX_SINCE_ROUTE {
        return Some("terceiro bypass sem usar a rota");
    }
    None
}

/// Charges a bypass and returns the already-updated accounts.
pub(super) fn charge_bypass(st: &GateState, project_root: &Path, actor: &str) -> BypassBudget {
    let key = bypass_key(project_root, actor);
    let prev = st.bypass_ledger.get(&key).unwrap_or_default();
    let next = BypassBudget {
        strict_run: prev.strict_run.saturating_add(1),
        since_route: prev.since_route.saturating_add(1),
    };
    st.bypass_ledger.insert(key, next);
    next
}

/// The sanctioned route was used: the credit comes back in full.
pub(super) fn credit_sanctioned_route(st: &GateState, project_root: &Path, actor: &str) {
    st.bypass_ledger
        .invalidate(&bypass_key(project_root, actor));
}

/// A command that is not a bypass breaks the STRICT sequence, and only it.
///
/// The `since_route` account survives on purpose: otherwise, it would suffice to
/// interleave an `echo` between the bypasses to zero the budget, which is
/// exactly the spaced series that R2 exists to prevent.
pub(super) fn break_strict_run(st: &GateState, project_root: &Path, actor: &str) {
    let key = bypass_key(project_root, actor);
    if let Some(prev) = st.bypass_ledger.get(&key)
        && prev.strict_run > 0
    {
        st.bypass_ledger.insert(
            key,
            BypassBudget {
                strict_run: 0,
                since_route: prev.since_route,
            },
        );
    }
}

/// Stamps this read and, if the file CHANGED since the read the ledger remembers, zeroes the
/// G7 count: re-reading a file changed outside the Edit tool (python, linter, another
/// session) is not re-inspection — it is the refresh the harness requires before the next Edit. Only a
/// MEASURED change counts (previous and current stamps exist and differ); an unchanged or
/// non-measurable file keeps counting, and its 3rd read is still denied.
pub(super) fn g7_refresh_if_changed(
    st: &GateState,
    project_root: &Path,
    key: u64,
    fp: &str,
) -> Option<FileStamp> {
    let stamp = file_stamp(project_root, fp);
    if matches!((stamp, st.g7_stamps.get(&key)), (Some(agora), Some(antes)) if agora != antes) {
        st.g7_seen.invalidate(&key);
    }
    if let Some(s) = stamp {
        st.g7_stamps.insert(key, s);
    }
    stamp
}

/// W2 S-2.3 — decides deny vs advisory from LIVE precision. Pure, so that the
/// test does not depend on the global process counters.
pub(super) fn g1_should_deny(precision: Option<(f64, u64)>) -> bool {
    !matches!(precision,
        Some((p, volume)) if volume >= G1_DEMOTE_MIN_EVENTS && p < G1_DEMOTE_FLOOR)
}

/// The body, with the overrides injected (see `GateConfig`).
pub(super) fn burst_gate_with(
    st: &GateState,
    project_root: &Path,
    session: &str,
    cmd: &str,
    tool_input: &Value,
    ovr: &GateConfig,
) -> Option<Verdict> {
    use crate::shared::gate_metrics::{
        GateEvent, GateId, g1_live_precision, record_g1_continuation, record_gate_event,
    };
    let class = scan_class_of(cmd);
    // S-2.3: the first call after a deny closes the continuation-check. Following the
    // REMEDY (`touring run`) counts as a hit AND as followed: converting the
    // burst is the best outcome of the gate — counting it as "other" would demote a
    // working gate (seen live on the very first G1 deny, 2026-08-25).
    if let Some(denied_class) = st.pending_g1.remove(session) {
        let seguiu_remedio = cmd.contains("touring run");
        if seguiu_remedio {
            record_gate_event(GateId::G1, GateEvent::Followed);
        }
        record_g1_continuation(seguiu_remedio || class == Some(denied_class.as_str()));
    }
    let class = class?;
    // S24: a command naming a path the sandbox cannot reach is not fusible into
    // `touring run` — counting it would deny toward a route that fails.
    if !unreachable_in_sandbox(project_root, cmd).is_none() {
        return None;
    }
    let key = burst_key(project_root, session, class);
    let epoca_atual = st
        .mutation_epoch
        .get(&project_root.display().to_string())
        .unwrap_or(0);
    let (count, mut cmds, epoca_anterior) =
        st.burst_ledger
            .get(&key)
            .unwrap_or((0, Vec::new(), epoca_atual));
    let count = count.saturating_add(1);
    // The command enters WHOLE (never cut). The ledger is the memory of the burst;
    // in the trigger's program the already-executed ones become a comment (S212) and only the
    // command that fired runs — even when it did not fit here.
    if cmds.len() < 8 && cmd.chars().count() <= G1_BODY_BUDGET {
        cmds.push(cmd.to_string());
    }
    st.burst_ledger
        .insert(key, (count, cmds.clone(), epoca_atual));
    // Calibration by SIMILARITY: G1 used to wait for `G1_DENY_AT` calls of the same
    // class. A small variation of the previous call is already the same inspection
    // repeated — there is no need to wait for the fourth to know the burst is
    // one only. It fires on the SECOND when ≥ 50% of the tokens repeat.
    let similaridade = if cmds.len() >= 2 && epoca_anterior == epoca_atual {
        command_similarity(&cmds[cmds.len() - 2], cmd)
    } else {
        0.0
    };
    let quase_igual = similaridade >= G1_SIMILARITY_AT;
    if count < G1_DENY_AT && !quase_igual {
        return None;
    }
    if ovr.code_gates_disabled {
        record_gate_event(GateId::G1, GateEvent::Bypassed);
        return None;
    }
    // One trigger per batch (the same contract as G10: «resets for the next burst — one
    // deny per batch, never fatigue»): EVERY trigger consumes the burst — advisory, rewrite
    // or deny. What the trigger carried was consumed and the next inspection starts another; without
    // this each following inspection re-fused the whole ledger (up to 8 commands). `cmds`
    // below is the local copy, so the trigger's program does not depend on the ledger.
    st.burst_ledger.invalidate(&key);
    // S212 in G1 (09/10/2026, measured live on 30.4.95): the ledger ends at the command
    // that fired, and everything before it ALREADY ran natively. Fusing the whole ledger
    // re-executed those commands in the sandbox on every following inspection; and a command
    // above the budget did not even enter the ledger, so the rewrite ran the old ones and
    // LOST the current one. The invariant is G10's: what already ran travels as a comment, and
    // the command that fired is the only step — always present, never cut.
    let executados: Vec<String> = match cmds.split_last() {
        Some((ultimo, antes)) if ultimo == cmd => antes.to_vec(),
        _ => cmds.clone(),
    };
    let mut corpo = comentarios_ja_executados(&executados);
    corpo.push_str(cmd);
    let wall_ms = rewrite_wall_ms(tool_input);
    let programa = format!(
        "touring run --lang bash --timeout-ms {wall_ms} --code '{}'",
        corpo.replace('\'', "'\\''")
    );
    if !g1_should_deny(ovr.g1_precision.unwrap_or_else(g1_live_precision)) {
        // Self-demoted by live data (S-2.3): the thesis remains falsifiable.
        record_gate_event(GateId::G1, GateEvent::Emitted);
        return Some(Verdict::advise(
            Gate::G1,
            g1_advisory_text(count, class, &programa),
        ));
    }
    // V0e1: Denied and pending_g1 moved down to the classic deny path
    // (below the rewrite) — the rewrite is not a blind turn and does not enter the
    // continuation-check.
    // The DECLARED reason has to be that of the trigger that fired. With the calibration
    // by similarity, a deny on the 2nd call showing "90% das rajadas ≥4"
    // would justify the block with a statistic that does not apply to it — the
    // D8 anti-pattern (the announcement promises what the executor did not do) inside the
    // gate itself.
    let motivo = if quase_igual && count < G1_DENY_AT {
        format!(
            "repete {:.0}% dos tokens da inspeção anterior da mesma classe, sem \
             mutação no meio — é a mesma varredura variada, não uma pergunta nova",
            similaridade * 100.0
        )
    } else {
        format!(
            "{count}ª inspeção da classe `{class}` desde o último disparo (janela de \
             {CODE_MODE_WINDOW_SECS}s) — 90% das rajadas ≥{G1_DENY_AT} continuavam \
             iguais (51 disparos/55 sessões)"
        )
    };
    let hint = snippet_hint_for_program(&programa, project_root);
    // V0e1 (09/10/2026 — approved by Gabriel; measured by 6b: G1 = 46% of
    // denials and fewer than half follow the route): the gate EXECUTES the route instead of
    // denying — the accumulated burst becomes the `updatedInput` of `touring run` and the
    // executor runs it in the sandbox (spike S-8.6: updatedInput honored). What the
    // sandbox cannot reach does not even get here (the S24 check above returns None → the
    // original call runs natively: the fallback is structural, never a silent
    // zero). Human kill switch: TOURING_G1_REWRITE_DISABLED=1.
    if !ovr.g1_rewrite_disabled {
        record_gate_event(GateId::G1, GateEvent::Emitted);
        record_gate_event(GateId::G1, GateEvent::Followed);
        return Some(g1_rewrite(
            &motivo,
            &hint,
            executados.len(),
            tool_input,
            programa,
        ));
    }
    record_gate_event(GateId::G1, GateEvent::Denied);
    // a deny ends the turn blind: the next call is the model's ANSWER
    // to the deny (the A/B continuation-check lives on its arriving at burst_gate).
    st.pending_g1.insert(session.to_string(), class.to_string());
    Some(Verdict::deny(
        Gate::G1,
        format!(
            "[G1 rajada-de-inspeção] {motivo}. Rode-o como programa — os já executados da \
         rajada vêm como comentário e NUNCA são repetidos:\n  {programa}{hint}\nBypass \
         por-comando: prefixe {GATE_BYPASS_TOKEN} (reseta a janela e é contado)."
        ),
    ))
}

/// The gate chain with the MOST SEVERE verdict (30.4.97, G5/G9 order): no advisory
/// short-circuits the chain. Gates that only warn deposit the text in `avisos` and the evaluation
/// continues; the applicable deny (any position in the chain) wins, and the advisory goes along in its text.
/// Runs BEFORE the suggester's anti-spam: a repeated deny must never be swallowed by it.
pub(super) fn run_chain(
    st: &GateState,
    event: &ToolEvent<'_>,
    ovr: &GateConfig,
    avisos: &mut Vec<Advisory>,
) -> Option<Verdict> {
    let ToolEvent {
        project_root,
        session,
        tool_name,
        tool_input,
    } = *event;
    use crate::shared::gate_metrics::{GateEvent, GateId, record_gate_event};
    let project = project_root.display().to_string();
    // Every mutation advances the project epoch (G6 never fires across it).
    if matches!(tool_name, "Edit" | "Write" | "NotebookEdit") {
        // G9 (N3a): the deny's conversion is the Edit with gates — the next
        // Edit/Write/NotebookEdit of the session closes the continuation-check.
        if st.pending_g9.remove(session).is_some() {
            record_gate_event(GateId::G9, GateEvent::Followed);
        }
        let epoch = st.mutation_epoch.get(&project).unwrap_or(0);
        st.mutation_epoch.insert(project.clone(), epoch + 1);
        // G5 (telemetry): the edit burst grows; its end is the first
        // Bash action (where the single advisory can speak).
        let streak = st.g5_edit_streak.get(session).unwrap_or(0);
        st.g5_edit_streak
            .insert(session.to_string(), streak.saturating_add(1));
        // W3 S-3.1 — G3: Edit without a recent Read of the file, PER SESSION.
        // Write stays OUTSIDE the gate: creation has nothing to read and overwrite already
        // requires a Read in the harness — G3 denied a live creation Write on
        // 24/08 (strategy doc). The written content is known to the session,
        // so the Write REGISTERS the file as read (the following Edit passes).
        if let Some(fp) = tool_input.get("file_path").and_then(Value::as_str) {
            if tool_name == "Write" {
                st.g3_read_files
                    .insert(g3_read_key(project_root, session, fp), ());
            } else if st
                .g3_read_files
                .get(&g3_read_key(project_root, session, fp))
                .is_some()
            {
                st.g3_streak.insert(session.to_string(), 0);
            } else if !ovr.code_gates_disabled {
                let n = st.g3_streak.get(session).unwrap_or(0).saturating_add(1);
                st.g3_streak.insert(session.to_string(), n);
                // Tightening 29/08 (Gabriel's order): 1 advisory and the 2nd in a row
                // denies — editing without reading is the origin of edit_string_not_found;
                // one warning is enough.
                if n >= 2 {
                    record_gate_event(GateId::G3, GateEvent::Denied);
                    return Some(Verdict::deny(
                        Gate::G3,
                        g3_deny_reason(project_root, fp, n, tool_input),
                    ));
                }
                record_gate_event(GateId::G3, GateEvent::Emitted);
                return Some(Verdict::advise(
                    Gate::G3,
                    g3_advisory_text(project_root, fp, n, tool_input),
                ));
            }
        }
        // W6 S-6.3 — counterfactual in an added comment (advisory, NEVER
        // deny: prose in a comment has a real false-positive rate).
        let adicionado = tool_input
            .get("new_string")
            .or_else(|| tool_input.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("");
        // #6 (04/11/2026): the counter keeps observing under the kill switch; the note does
        // not speak — `TOURING_CODE_GATES_DISABLED=1` silences every gate, advisories included.
        if counterfactual_comment(adicionado) {
            crate::shared::gate_metrics::record_e3_counterfactual();
            if ovr.code_gates_disabled {
                return None;
            }
            return Some(Verdict::advise(
                Gate::E3,
                "[E3 contrafactual-sem-endereço] comentário adicionado afirma o que \
                 ACONTECERIA sem citar um run_id — a classe do arm_marker de 24/08 \
                 (afirmação falsa nunca exercitada). Exercite (probe FACT= / touring \
                 run) e cite o run, ou remova o modal."
                    .to_string(),
            ));
        }
        return None;
    }
    if tool_name == "Read" {
        let fp = tool_input.get("file_path").and_then(Value::as_str)?;
        // M3 — the G3 ledger (session, file) already distinguishes a new file from a
        // re-read; a miss here IS a new distinct file in the window.
        let g3k = g3_read_key(project_root, session, fp);
        let arquivo_novo = st.g3_read_files.get(&g3k).is_none();
        st.g3_read_files.insert(g3k, ());
        st.g3_streak.insert(session.to_string(), 0);
        if arquivo_novo {
            let distintos = st
                .m3_distinct_reads
                .get(session)
                .unwrap_or(0)
                .saturating_add(1);
            st.m3_distinct_reads.insert(session.to_string(), distintos);
            // Exactly at the threshold (== and not >=): 1 advisory per window.
            if distintos == M3_DELEGATION_AT && !ovr.code_gates_disabled {
                crate::shared::gate_metrics::record_m3_delegation_advised();
                return Some(Verdict::advise(
                    Gate::M3,
                    format!(
                        "[M3 delegação] {M3_DELEGATION_AT}º arquivo DISTINTO lido nesta janela — \
                     exploração desta largura paga o isolamento de contexto: delegue a 1-3 \
                     subagentes read-only (Explore/general-purpose), cada um com objetivo + \
                     formato de saída + fronteiras, e receba só as conclusões (50 arquivos \
                     lidos voltam como 3 achados). Leitura larga paraleliza; ESCRITA fica \
                     serial no contexto principal — decisões paralelas conflitam. Cadeia \
                     dependente (passo 2 precisa do output integral do passo 1) também fica \
                     no contexto único. Advisory único por sessão/1h."
                    ),
                ));
            }
        }
        // G4 (telemetry, never deny): reading without locating first.
        if st.g4_last_locate.get(session).is_none() {
            crate::shared::gate_metrics::record_g4_observed();
        }
        // W3 S-3.2 — G7: re-inspection of the SAME target.
        let key = g7_key(project_root, session, fp);
        // 30.4.97 (G7G3G9): re-reading a file that CHANGED since the read the ledger remembers is not
        // re-inspection — it is the re-read the harness requires («File has been modified since read»)
        // before the next Edit. Counting that read closed the circle: python writes the file
        // (G9 pushes it there), the Edit goes stale, and the Read that unlocks it was the 3rd.
        let stamp = g7_refresh_if_changed(st, project_root, key, fp);
        let n = st.g7_seen.get(&key).unwrap_or(0).saturating_add(1);
        st.g7_seen.insert(key, n);
        if !ovr.code_gates_disabled {
            if n >= 3 {
                record_gate_event(GateId::G7, GateEvent::Denied);
                let rota = r1_route_for(fp, ovr.r1_skeleton().as_deref());
                let edicao = g7_edit_route_note(fp, stamp.is_some());
                return Some(Verdict::deny(
                    Gate::G7,
                    format!(
                        "[G7 re-inspeção] {n}ª leitura de {fp} na janela — reler é o sinal \
                     mais direto de programa-faltando (campeões medidos: adw.py 39×, \
                     cli_suggester.rs 32×). Varra de uma vez (R1): {rota}.{edicao} \
                     Um `touring run` citando {fp} reseta o gate."
                    ),
                ));
            }
            if n == 2 {
                record_gate_event(GateId::G7, GateEvent::Emitted);
                let rota = r1_route_for(fp, ovr.r1_skeleton().as_deref());
                return Some(Verdict::advise(
                    Gate::G7,
                    format!(
                        "[G7 re-inspeção] 2ª leitura de {fp} na janela — programa-faltando? \
                     R1 instanciado: {rota} (a 3ª leitura nega)."
                    ),
                ));
            }
        }
        return None;
    }
    if matches!(tool_name, "Grep" | "Glob") {
        st.g4_last_locate.insert(session.to_string(), ());
        return None;
    }
    if tool_name != "Bash" {
        return None;
    }
    let cmd = tool_input.get("command").and_then(Value::as_str)?;
    // G7 reset: a `touring run` citing a path zeroes the count of that target.
    if cmd.contains("touring run") {
        for frag in path_fragments(cmd) {
            st.g7_seen.invalidate(&g7_key(project_root, session, frag));
        }
    }
    // G5 (telemetry): the first Bash action ends the edit burst; if it
    // had >=3 and this command is not validation, ONE advisory (never during).
    let g5 = st.g5_edit_streak.get(session).unwrap_or(0);
    if g5 > 0 {
        st.g5_edit_streak.insert(session.to_string(), 0);
        const VALIDA: &[&str] = &[
            "cargo check",
            "cargo test",
            "cargo clippy",
            "pytest",
            "touring e2e",
            "npm test",
            "adw lint",
            "adw test",
        ];
        let g5_unvalidated = g5 >= 3 && !VALIDA.iter().any(|m| cmd.contains(m));
        if g5_unvalidated {
            crate::shared::gate_metrics::record_g5_observed();
        }
        // #6 (04/11/2026): observed under the kill switch, never spoken.
        if g5_unvalidated && !ovr.code_gates_disabled {
            // Advisory: does NOT short-circuit the chain (30.4.97) — an applicable deny further on
            // (G9 blind `sed -i`, G2, G6…) wins and carries this text along.
            avisos.push(Advisory::new(
                Gate::G5,
                format!(
                    "[G5 telemetria] rajada de {g5} edits terminou sem build/teste — P9 \
                 medido em 17%. Valide agora (cargo check/test no crate tocado); a \
                 regressão silenciosa nasce aqui."
                ),
            ));
        }
    }
    // followed (S-1.4): there was a G2 deny in this session and the command now carries
    // pipefail — the canonical conversion was adopted.
    if st.pending_g2.remove(session).is_some() && cmd.contains("pipefail") {
        record_gate_event(GateId::G2, GateEvent::Followed);
    }
    // G11 — the sanctioned route returns the credit BEFORE any charge, so
    // that a turn alternating program and bypass never accumulates debt.
    if is_sanctioned_route(cmd) {
        credit_sanctioned_route(st, project_root, session);
    } else if !has_bypass_token(cmd) {
        break_strict_run(st, project_root, session);
    }
    if has_bypass_token(cmd) {
        // The budget is charged BEFORE the exemption: the token remains legitimate, but
        // the SERIES starts to cost. The G11 deny is not bypassable on purpose —
        // a bypass that releases itself is not a budget; the way out is the human
        // kill switch in the daemon env, which requires a decision outside the turn.
        let budget = charge_bypass(st, project_root, session);
        if !ovr.code_gates_disabled
            && let Some(rule) = bypass_verdict(budget)
        {
            record_gate_event(GateId::G1, GateEvent::Emitted);
            return Some(Verdict::deny(
                Gate::G11,
                format!(
                    "[G11 orçamento de bypass] {rule}: {} seguido(s), {} desde a última rota, janela {}s. \
                 O token é por-comando, não uma rota. Use a rota sancionada — funda a \
                 família numa varredura única: `touring run --lang bash --file <script>` \
                 — e o crédito volta inteiro. Sem saída por prefixo: o kill switch é \
                 humano (`TOURING_CODE_GATES_DISABLED=1` no env do daemon + restart).",
                    budget.strict_run, budget.since_route, G11_WINDOW_SECS
                ),
            ));
        }
        if exit_code_through_pipe(cmd) {
            record_gate_event(GateId::G2, GateEvent::Bypassed);
        }
        if is_inline_blind_edit(cmd) {
            record_gate_event(GateId::G9, GateEvent::Bypassed);
        }
        if exec_class_of(cmd).is_some() {
            record_gate_event(GateId::G10, GateEvent::Bypassed);
        }
        // G1: the conscious bypass resets the class window (S-2.2) — legitimate
        // exploration restarts from zero, and the event is counted.
        if let Some(class) = scan_class_of(cmd) {
            let key = burst_key(project_root, session, class);
            if matches!(st.burst_ledger.get(&key), Some((n, _, _)) if n >= G1_DENY_AT - 1) {
                record_gate_event(GateId::G1, GateEvent::Bypassed);
            }
            st.burst_ledger.invalidate(&key);
        }
        return None;
    }
    // G2 — a heredoc is DATA (the body does not execute as a pipeline of THIS shell);
    // skipping avoids denying the write of a test that merely CONTAINS the pattern.
    if exit_code_through_pipe(&touring_hooks_shared::command_policy::without_heredoc_data(
        cmd,
    )) {
        if ovr.code_gates_disabled {
            record_gate_event(GateId::G2, GateEvent::Bypassed);
            return None;
        }
        // S-8.6 (spike SUPPORTED, proven live 2026-08-25: the A4 rewrite
        // `cat`→highlight swapped a real command of this session): Claude Code
        // honors `updatedInput` from a settings.json hook. G2 then FIXES instead of
        // denying — zero friction, correction guaranteed by the executor (the
        // affordance ceiling). `Followed` is recorded together with `Emitted`
        // because the adoption is the executor's, not a hope.
        if !ovr.g2_rewrite_disabled {
            record_gate_event(GateId::G2, GateEvent::Emitted);
            record_gate_event(GateId::G2, GateEvent::Followed);
            return Some(Verdict::Rewrite {
                gate: Gate::G2,
                reason: "[G2 exit-code-through-pipe → REESCRITO] `$?` após pipe \
                             leria o status do último estágio; `set -o pipefail; ` \
                             prefixado (spike S-8.6: updatedInput honrado)."
                    .to_string(),
                context: None,
                updated_input: rewritten_input(tool_input, format!("set -o pipefail; {cmd}")),
            });
        }
        record_gate_event(GateId::G2, GateEvent::Denied);
        st.pending_g2.insert(session.to_string(), ());
        let remedy = if cmd.len() <= 1500 {
            format!("set -o pipefail; {cmd}")
        } else {
            "set -o pipefail; <o mesmo comando, verbatim>".to_string()
        };
        return Some(Verdict::deny(
            Gate::G2,
            format!(
                "[G2 exit-code-through-pipe] `$?` depois de um pipe lê o status do ÚLTIMO \
             estágio (tail/jq), não do programa medido — 68 casos na simulação de 55 \
             sessões; 3 cometidos pela própria sessão que desenhou este gate. \
             Reexecute exatamente:\n  {remedy}\nOu remova o pipe e leia o exit \
             direto. Bypass por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed)."
            ),
        ));
    }
    // G9 (N3a, 2026-08-26) — blind inline write denied with the derived route:
    // the Pattern 5 advisory spoke and the command executed all the same (advisory ≠
    // gate — the measured adoption of one does not transfer to the other). A heredoc is
    // data (the body does not execute in this shell). BEFORE G8: a loop with
    // `sed -i` is denied here, not rewritten into the sandbox.
    if is_inline_blind_edit(&touring_hooks_shared::command_policy::without_heredoc_data(
        cmd,
    )) {
        if ovr.code_gates_disabled {
            record_gate_event(GateId::G9, GateEvent::Bypassed);
            return None;
        }
        record_gate_event(GateId::G9, GateEvent::Denied);
        st.pending_g9.insert(session.to_string(), ());
        let target = inline_edit_target(cmd).unwrap_or_else(|| "<o arquivo>".to_string());
        let cmd_remedio = if cmd.len() <= 1500 {
            cmd.replace('\'', r"'\''") // escaped quotes — the route pastes into the shell (G8 lesson)
        } else {
            "<o mesmo comando, verbatim>".to_string()
        };
        return Some(Verdict::deny(
            Gate::G9,
            format!(
                "[G9 escrita-cega-inline] `sed -i`/`awk -i inplace`/`perl -pi` editam às \
             cegas — zero dos 17 gates do Edit (VGP, blast_radius, TDG, snapshot \
             atômico, gotcha). Rotas:\n  \
             1 arquivo:   Edit tool em `{target}` (old_string → new_string, gates completos)\n  \
             mesmo comando, sandbox: touring run --lang bash --code '{cmd_remedio}'\n  \
             estrutural (N arquivos): touring ast grep {target} '<pattern>' --rewrite '<replacement>' \
             (ast-grep — metavars $VAR, NÃO regex de sed)\n  \
             Bypass por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed)."
            ),
        ));
    }
    // G8 — INSPECTION loop rewritten into the sandbox (2026-08-25).
    //
    // Measured in the transcript of this very session (1433 tool calls): of the 453
    // code mode opportunities, the other classes are either already covered or
    // not worth it. Tightening the burst threshold is friction with no gain (the burst
    // median is 1); converting grep to the index would serve ~3% (56% of the
    // greps are structural regex, which the index does not answer). Loops, however:
    // 44 real cases in 47 detected (94% precision), and the program is
    // WRITTEN in the command — it was decided whole and split into N round-trips.
    //
    // The remedy is G2's, which proved itself live: REWRITE, not deny. Only
    // pure-inspection loops enter: the sandbox applies capabilities per
    // command — `run-1787620774532-346193` recorded "X6 denied the subprocess
    // capability 'echo' under profile 'Sandboxed'" —, so a loop that kills
    // processes, does git or compiles is out by construction.
    if let Some(rewritten) =
        loop_rewrite_candidate(cmd).filter(|_| unreachable_in_sandbox(project_root, cmd).is_none())
    {
        if ovr.code_gates_disabled {
            record_gate_event(GateId::G8, GateEvent::Bypassed);
        } else {
            record_gate_event(GateId::G8, GateEvent::Emitted);
            record_gate_event(GateId::G8, GateEvent::Followed);
            let rewritten = with_wall_budget(rewritten, rewrite_wall_ms(tool_input));
            return Some(Verdict::Rewrite {
                gate: Gate::G8,
                reason: "[G8 laço-de-inspeção → REESCRITO] o laço já É o programa; \
                             uma varredura no sandbox devolve só o agregado (Anthropic \
                             CodeAct). Kill switch humano: TOURING_G8_REWRITE_DISABLED=1."
                    .to_string(),
                context: None,
                updated_input: rewritten_input(tool_input, rewritten),
            });
        }
    }
    // S5 (29/08) — the write→run pair: `cat >`/`tee` of a script registers the
    // target; the execution of the SAME path in the window counts a pair. It was born from the 60-Bash
    // turn of `analise` (28 pairs, all invisible: the write is sanctioned T4
    // and the standalone execution did not accumulate) — the G-turn only spoke at
    // Stop, with the cost already paid. Here the collapse happens DURING the turn.
    if let Some(path) = script_write_target(cmd) {
        st.written_scripts_ledger
            .insert(write_run_key(project_root, session, &path), ());
    }
    if let Some(deny) = write_run_pair_gate_with(st, project_root, session, cmd, ovr) {
        return Some(deny);
    }
    // G10 (S4, 2026-08-26) — homogeneous execution burst (R4): N serial calls of the
    // same interpreter with 0 `touring run` in the window. The unrolled loop
    // becomes 1 program (R9). A `touring run` between them zeroes the count — the
    // "0 run" condition is the proof that the route was already offered and ignored.
    // AFTER G8: a written loop is rewritten there; here it is the serial burst.
    if let Some(class) =
        exec_class_of(cmd).filter(|_| unreachable_in_sandbox(project_root, cmd).is_none())
    {
        if class == "python-inline" {
            // S5 telemetry: each inline seen counts — the calibration keeps
            // measuring even with the class now inside the burst.
            crate::shared::gate_metrics::record_exec_heredoc_inline_seen();
        }
        let key = exec_burst_key(project_root, session, class);
        let (n, mut cmds) = st.exec_burst_ledger.get(&key).unwrap_or_default();
        let n = n + 1;
        if cmds.len() < 16 {
            cmds.push(cmd.to_string());
        }
        if n < EXEC_BURST_DENY_AT {
            st.exec_burst_ledger.insert(key, (n, cmds));
        } else if ovr.code_gates_disabled {
            record_gate_event(GateId::G10, GateEvent::Bypassed);
            return None;
        } else {
            record_gate_event(GateId::G10, GateEvent::Denied);
            st.pending_g10.insert(session.to_string(), ());
            // resets for the next burst — one deny per batch, never fatigue
            st.exec_burst_ledger.invalidate(&key);
            // G10ROTA (30.4.97): which route of `touring run` EXECUTES this body? The question belongs to the
            // executor (D8) — a deny that names a command X6 refuses teaches failing.
            let rota = g10_inline_route(project_root, class, cmd);
            if !rota.runs() {
                return Some(Verdict::deny(
                    Gate::G10,
                    g10_unrunnable_deny(n, class, &rota, project_root, cmd),
                ));
            }
            // python-inline has a 1:1 remedy (the body of the command ITSELF); the
            // other classes fuse the accumulated burst via R9.
            let programa = if class == "python-inline" {
                python_inline_remedy_via(cmd, &rota)
            } else {
                // S212: the ledger ends with the denied command; everything before it
                // already ran and must not be replayed.
                let executed: Vec<String> = match cmds.split_last() {
                    Some((last, before)) if last == cmd => before.to_vec(),
                    _ => cmds.clone(),
                };
                r9_exec_program(&executed, cmd)
            };
            // S6 — the real burst consults the portfolio: when there is a program
            // that already worked for this work, the deny delivers both
            // (the aggregated skeleton + the instantiated prior art).
            let prior = match &ovr.portfolio_dir {
                Some(dir) => touring_foundation::portfolio::store::load_from(dir)
                    .ok()
                    .and_then(|idx| portfolio_remedy_for_burst_on(class, &cmds, &idx)),
                None => portfolio_remedy_for_burst(class, &cmds),
            }
            .unwrap_or_default();
            // N6 — and the ladder delivers the similar block, when it exists.
            let hint = snippet_hint_for_program(&programa, project_root);
            let lead = g10_lead(&rota, project_root);
            return Some(Verdict::deny(
                Gate::G10,
                format!(
                    "[G10 exec-burst] {n}ª chamada seriada do executor `{class}` (0 `touring run` \
                 na janela de {}s){lead}\n  \
                 {programa}\nSó o comando negado roda; os já executados da rajada vêm como \
                 comentário e NUNCA são repetidos (denies subprocess-only são advisory em \
                 `--lang bash` — o sandbox contém o filesystem); veredito na saída, íntegra \
                 no spill. Os próximos comandos do lote entram no MESMO programa.{prior}{hint} \
                 Bypass por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed).",
                    EXEC_BURST_WINDOW_SECS
                ),
            ));
        }
    } else if cmd.contains("touring run") || cmd.contains("touring exec") {
        if st.pending_g10.remove(session).is_some() {
            record_gate_event(GateId::G10, GateEvent::Followed);
        }
        for class in ["python", "pytest", "python-inline"] {
            st.exec_burst_ledger
                .invalidate(&exec_burst_key(project_root, session, class));
        }
        // S5 — the route followed zeroes the write→run pair as well (same reason as S3
        // below: without this the next legitimate script would inherit a count from
        // a loop that the model ALREADY converted into a program).
        st.write_run_pair_ledger.invalidate(&write_run_key(
            project_root,
            session,
            "\u{0}write-run-pares",
        ));
        // S3 — the route was followed: the inspection window zeroes along with it. Without this the
        // ledger would keep counting a burst that the model ALREADY converted into a
        // program, and the next legitimate `grep` would take a deny inherited from a
        // burst that no longer exists.
        for class in CODE_MODE_COLLAPSED_CLASSES {
            st.inspect_burst_ledger
                .invalidate(&inspect_burst_key(project_root, session, class));
            // G1 — the session contract ("a `touring run` in the window zeroes the count")
            // held only for S3: measured 09/10, an 18:49 burst was still re-fused
            // at 18:55 after a `touring run`.
            st.burst_ledger
                .invalidate(&burst_key(project_root, session, class));
        }
        st.inspect_burst_ledger.invalidate(&inspect_burst_key(
            project_root,
            session,
            PY_INLINE_INSPECT_CLASS,
        ));
    }
    // S3 (27/08/2026) — the `code` mode collapse stopped being PER CLASS and
    // became PER BURST. The 1st inspection of a class in the window executes
    // untouched; the 2nd onwards comes back as ONE program with both fused.
    //
    // What changed and why: the P2c arm denied on the FIRST call of
    // `grep`/`cat`/`find`. Measuring 115 transcripts (`s3_burst_distribution.py`),
    // 22.5% of those calls are isolated — for them the deny charged a whole
    // round-trip and returned the SAME reading, which is the definition of
    // taxing the common case. The other 77.5% are in bursts ≥2, and those the
    // predicate still catches, now with the fused program instead of the rewritten
    // single command.
    //
    // The predicate is also what finally DELIVERS the promise that T3-B made
    // and never kept (`t3_turn_fused = 0`): "the 1st executes untouched, the K−1
    // come back as 1 program". T3-B hung that on turn closing, an
    // event that this execution model does not produce; here it is a counter with
    // TTL, which does occur. Mutation/build keep passing (scan_class_of only
    // recognizes inspection), and a `touring run` in the window zeroes the ledger — the
    // count proves that the route was offered and not used.
    let apresentacao = resolve_presentation(project_root, cmd, &ovr.code_mode_env);
    // Tightening 29/08 (Gabriel's order): READ-ONLY python-inline is inspection and
    // falls into the SAME burst — used as a reading, the interpreter got 4
    // passes where `cat` gets 1 (G10 only denies on the 5th/600s). The classification is
    // by the BODY (conservative deny-list): any sign of write/network/
    // subprocess keeps the command in G10 — fails in the loose direction, never
    // routes a writer to the inspection deny.
    let inspect_class = scan_class_of(cmd)
        .filter(|c| CODE_MODE_COLLAPSED_CLASSES.contains(c))
        .or_else(|| python_inline_readonly_class(cmd));
    let mut s3_avisou = false;
    if apresentacao == CodeModePresentation::Code
        && let Some(class) = inspect_class
        && !ovr.code_gates_disabled
        && unreachable_in_sandbox(project_root, cmd).is_none()
    {
        let key = inspect_burst_key(project_root, session, class);
        let (n, mut cmds) = st.inspect_burst_ledger.get(&key).unwrap_or_default();
        let n = n + 1;
        if cmds.len() < 16 {
            cmds.push(cmd.to_string());
        }
        // D (09/10/2026, Gabriel's order — §8 of V0e5): the burst WARNS, never
        // denies. Measured: 1,272 S3 denials in 24 h, the biggest friction gate. The
        // batch does NOT zero on the warning — the following ones of the same burst count in
        // silence (one warning per burst, never fatigue); only a `touring run` or the
        // window without inspection rearms it (the TTL renews on every insertion).
        st.inspect_burst_ledger.insert(key, (n, cmds.clone()));
        if n < INSPECT_BURST_ADVISE_AT {
            // The isolated one passes in SILENCE: a nudge here would bring back through the back
            // door the friction that the predicate had just removed.
            crate::shared::gate_metrics::record_g1_inspect_first_passed();
            bump_durable_evidence(project_root, "s3", "first_passed");
        } else if n > INSPECT_BURST_ADVISE_AT {
            bump_durable_evidence(project_root, "s3", "in_burst");
        } else {
            // python-inline fuses the BODIES (the 1:1 per-command remedy already
            // existed; the burst joins them in order); the S3 shell classes keep
            // going to R9 via fuse_burst_program. G1 NO LONGER uses fuse_burst_program:
            // it sends the already-executed ones as a comment (S212) and only the command that
            // fired as a step.
            let (programa, omitidos, lang) = if class == PY_INLINE_INSPECT_CLASS {
                let (p, o) = python_inline_burst_program(&cmds);
                (p, o, "python")
            } else {
                let (p, o) = fuse_burst_program(&cmds);
                (p, o, "bash")
            };
            record_route_offer(project_root, session, apresentacao);
            record_gate_event(GateId::G1, GateEvent::Emitted);
            crate::shared::gate_metrics::record_g1_inspect_burst_advised();
            bump_durable_evidence(project_root, "s3", "advised");
            let nota_omissao = if omitidos > 0 {
                format!(
                    " ({omitidos} comando(s) não coube(ram) no orçamento e foram OMITIDOS \
                     — inteiro ou fora, jamais truncado: rode-os à parte)"
                )
            } else {
                String::new()
            };
            // N6 — the ladder delivers the similar block, when it exists: the
            // warned burst becomes discovery, not only a route.
            let hint = snippet_hint_for_program(&programa, project_root);
            // Advisory: continues down the chain to G6 (30.4.97) — the byte-identical repeat that G6
            // denies wins over the warning. G1 does NOT decide this call again (`s3_avisou`): on the
            // 2nd inspection the burst only WARNS, by Gabriel's order (09/10/2026, D).
            avisos.push(Advisory::new(
                Gate::S3,
                format!(
                    "[CODE MODE · rajada] aviso — {n}ª inspeção `{class}` em {}s, executada \
                 nativa. Se a próxima também for inspeção, rode o lote de uma vez:\n  \
                 touring run --lang {lang} --code '{programa}'\n{nota_omissao}{hint}",
                    INSPECT_BURST_WINDOW_SECS
                ),
            ));
            s3_avisou = true;
        }
    }
    // `native` silences the whole induction: the scope declared it does not want to be
    // pushed, and a nudge it did not ask for is the cost without the counterpart.
    if apresentacao == CodeModePresentation::Native {
        return None;
    }
    // G6 — byte-identical repeat within the TTL window, with no mutation in between.
    // BEFORE T3-B: a byte-identical blind retry is the most specific signal and G6
    // consumes it whole (advisory → deny); what passes from here is NEW inspection.
    if !g6_allowlisted(cmd) {
        let h = g6_key(project_root, session, tool_name, tool_input);
        let epoch = st.mutation_epoch.get(&project).unwrap_or(0);
        match st.g6_seen.get(&h) {
            None => st.g6_seen.insert(h, (0, epoch)),
            Some((repeats, seen_epoch)) if seen_epoch == epoch => {
                if ovr.code_gates_disabled {
                    record_gate_event(GateId::G6, GateEvent::Bypassed);
                    return None;
                }
                st.g6_seen.insert(h, (repeats + 1, epoch));
                if repeats == 0 {
                    record_gate_event(GateId::G6, GateEvent::Emitted);
                    // G6 CONSUMES the whole identical repeat (advisory → deny): the G1 further on
                    // handles NEW inspection, so this advisory ends the chain — by design.
                    return Some(Verdict::advise(Gate::G6, g6_advisory_text(cmd)));
                }
                record_gate_event(GateId::G6, GateEvent::Denied);
                // a deny ends the turn blind: the next call is the model's ANSWER
                // to the deny, not the continuation of a parallel batch.
                return Some(Verdict::deny(Gate::G6, g6_deny_reason(cmd)));
            }
            // epoch changed: the tree mutated since the first sight — fresh.
            Some((_, _)) => st.g6_seen.insert(h, (0, epoch)),
        }
    }

    // S10 (2026-08-27) — T3-B was REMOVED from here. Measured live: 3 Bash
    // calls of distinct classes in the same turn gave `t3_turn_first_passed = 3`
    // and `t3_turn_fused = 0` — each one was "the first", because PostToolUse
    // closes the turn between them. The gate kept per-session state and NEVER
    // decided. The S3 burst predicate delivers what it promised, with a
    // TIME window (300s) instead of a turn that always closes.
    // G1 — burst of atomic inspections of the SAME class (W2 teeth): decided
    // after G2 (the reading defect comes before the habit) and before G6.
    if s3_avisou {
        return None;
    }
    if let Some(resp) = burst_gate_with(st, project_root, session, cmd, tool_input, ovr) {
        return Some(resp);
    }
    None
}

// ── Task #6 — pillar induction (the active layer of the compounding structure) ──
//
// rule + skill + CLAUDE.md-pointer are the passive layers (knowledge); the
// empirical lesson (cont.¹⁰) is that passive knowledge does NOT induce — the
// builder had every master command documented and still reached for atomic tools.
// So this layer actively nudges the two pillars the upstream classifiers miss:
//   • `MasterCli`   — the proven adoption gap: an atomic `touring index/ast/wiring`
//                     call where a fused master command (scout/read/map/blast/
//                     investigate/guard) serves better.
//   • `LearningMemory` — raw-shell search of docs/history where `touring memory
//                     recall` may already hold the answer (Reflexo #3).
// `CodeMode` and `Intelligence` already fire upstream (C8 `detect_code_mode`, the
// read-rust classifier), so `classify_pillar` stays silent for them — no double
// nudge. Graduated DEFAULT-OFF, mirroring the F7c actuator: unset env ⇒ no
// classification, no emission, zero live impact. The loop closes via the
// `pillar_induction_{emitted,followed}` counters → `pillar_induction_ratio` KPI →
// F7. Per the roadmap thesis (affordance, not persuasion), if uptake stays low
// while armed, that telemetry is the evidence that pushes toward affordance
// (productization) — the experiment, not just the nudge. Gabriel arms it post-A/B.
