//! The chain: one tool event in, the strictest applicable verdict out.

use serde_json::Value;
use std::path::Path;

use super::*;
use crate::workflow::exit_code_through_pipe;

/// S5 — o gate do par write→run. `None` = sem decisão (o fluxo segue).
/// O corpo, com os overrides injetados (ver `GateConfig`).
pub(super) fn write_run_pair_gate_with(
    st: &GateState,
    project_root: &Path,
    session: &str,
    cmd: &str,
    ovr: &GateConfig,
) -> Option<Verdict> {
    use crate::shared::gate_metrics::{GateEvent, GateId, record_gate_event};
    // 1 par por comando, mesmo com múltiplos runs no tool_use: o que se conta
    // é o PASSO do loop execute-observe, não o número de interpretações.
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
    // `--lang` é obrigatório no CLI: sem ele a rota emitida NÃO EXECUTA
    // (medido pela peer analise-a2, 30/08 — clap rejeita e o deny vira erro
    // opaco, o antipadrão E4 cometido pelo próprio enforcement). is_script_path
    // só aceita .py/.sh, então o else é python.
    let lang = if path.ends_with(".sh") {
        "bash"
    } else {
        "python"
    };
    // G10ROTA (30.4.97): o script JÁ está em disco — o executor julga o CONTEÚDO dele. Um script
    // que escreve não roda no Sandboxed (X6 `file-write`), e a rota `--file` nomeada falharia
    // exatamente como a do exec-burst; a pergunta é do executor (D8), como lá.
    let rota = g10_script_route(project_root, lang, &path);
    if !rota.runs() {
        return Some(Verdict::deny(Gate::G10, format!(
            "[G10 write→run] {n}º script escrito-e-executado na janela de \
             {WRITE_RUN_WINDOW_SECS}s (`{path}`).{} {}",
            rota.explanation(project_root),
            bypass_next_step(cmd)
        )));
    }
    let comando = format!(
        "{}touring run {}--lang {lang} --file {path} --timeout-ms 60000",
        rota.env_prefix(),
        rota.run_flags()
    );
    let porque = rota.explanation(project_root);
    Some(Verdict::deny(Gate::G10, format!(
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
    )))
}


/// A decisão do G11 sobre um bypass que ACABOU de ser cobrado.
///
/// `None` libera. `Some(regra)` nega, e a regra nomeada entra na razão do deny —
/// um gate mudo degrada o retry (a lição dos gates falantes, A5).
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


/// Cobra um bypass e devolve as contas já atualizadas.
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


/// A rota sancionada foi usada: o crédito volta inteiro.
pub(super) fn credit_sanctioned_route(st: &GateState, project_root: &Path, actor: &str) {
    st.bypass_ledger.invalidate(&bypass_key(project_root, actor));
}


/// Um comando que não é bypass quebra a sequência ESTRITA, e só ela.
///
/// A conta `since_route` sobrevive de propósito: caso contrário, bastaria
/// intercalar um `echo` entre os bypasses para zerar o orçamento, que é
/// exatamente a série espaçada que a R2 existe para impedir.
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


/// Carimba esta leitura e, se o arquivo MUDOU desde a leitura que o ledger lembra, zera a
/// contagem do G7: reler um arquivo alterado por fora da ferramenta Edit (python, linter, outra
/// sessão) não é re-inspeção — é o refresh que o harness exige antes do próximo Edit. Só uma
/// mudança MEDIDA conta (carimbo anterior e atual existem e diferem); arquivo inalterado ou não
/// mensurável segue contando, e a 3ª leitura dele continua negada.
pub(super) fn g7_refresh_if_changed(st: &GateState, project_root: &Path, key: u64, fp: &str) -> Option<FileStamp> {
    let stamp = file_stamp(project_root, fp);
    if matches!((stamp, st.g7_stamps.get(&key)), (Some(agora), Some(antes)) if agora != antes) {
        st.g7_seen.invalidate(&key);
    }
    if let Some(s) = stamp {
        st.g7_stamps.insert(key, s);
    }
    stamp
}


/// W2 S-2.3 — decide deny vs advisory a partir da precisão VIVA. Pura, para o
/// teste não depender dos contadores globais de processo.
pub(super) fn g1_should_deny(precision: Option<(f64, u64)>) -> bool {
    !matches!(precision,
        Some((p, volume)) if volume >= G1_DEMOTE_MIN_EVENTS && p < G1_DEMOTE_FLOOR)
}


/// O corpo, com os overrides injetados (ver `GateConfig`).
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
    // S-2.3: a primeira chamada pós-deny fecha o continuation-check. Seguir o
    // REMÉDIO (`touring run`) conta como acerto E como followed: converter a
    // rajada é o melhor desfecho do gate — contá-lo como "other" demoveria um
    // gate funcionando (visto vivo no primeiríssimo deny do G1, 2026-08-25).
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
    let epoca_atual = st.mutation_epoch
        .get(&project_root.display().to_string())
        .unwrap_or(0);
    let (count, mut cmds, epoca_anterior) =
        st.burst_ledger
            .get(&key)
            .unwrap_or((0, Vec::new(), epoca_atual));
    let count = count.saturating_add(1);
    // O comando entra INTEIRO (nunca cortado). O ledger é a memória da rajada;
    // no programa do disparo os já executados viram comentário (S212) e só o
    // comando que disparou roda — inclusive quando ele não coube aqui.
    if cmds.len() < 8 && cmd.chars().count() <= G1_BODY_BUDGET {
        cmds.push(cmd.to_string());
    }
    st.burst_ledger.insert(key, (count, cmds.clone(), epoca_atual));
    // Calibração por SIMILARIDADE: o G1 esperava `G1_DENY_AT` chamadas da mesma
    // classe. Uma variação pequena da chamada anterior já é a mesma inspeção
    // repetida — não é preciso esperar a quarta para saber que a rajada é uma
    // só. Dispara na SEGUNDA quando ≥ 50% dos tokens se repetem.
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
    // Um disparo por lote (o mesmo contrato do G10: «zera para a próxima rajada — um
    // deny por lote, nunca fadiga»): TODO disparo consome a rajada — advisory, reescrita
    // ou deny. O que o disparo levou foi consumido e a próxima inspeção começa outra; sem
    // isto cada inspeção seguinte refundia o ledger inteiro (até 8 comandos). `cmds`
    // abaixo é a cópia local, então o programa do disparo não depende do ledger.
    st.burst_ledger.invalidate(&key);
    // S212 no G1 (09/10/2026, medido ao vivo no 30.4.95): o ledger termina no comando
    // que disparou, e tudo antes dele JÁ rodou nativo. Fundir o ledger inteiro
    // reexecutava esses comandos no sandbox a cada inspeção seguinte; e um comando
    // acima do orçamento nem entrava no ledger, então a reescrita rodava os antigos e
    // PERDIA o atual. O invariante é o do G10: o que já rodou viaja como comentário, e
    // o comando que disparou é o único passo — sempre presente, nunca cortado.
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
        // Autodemovido por dado vivo (S-2.3): a tese continua falsificável.
        record_gate_event(GateId::G1, GateEvent::Emitted);
        return Some(Verdict::advise(
            Gate::G1,
            g1_advisory_text(count, class, &programa),
        ));
    }
    // V0e1: o Denied e o pending_g1 desceram para o caminho do deny clássico
    // (abaixo do rewrite) — o rewrite não é um turno cego e não entra no
    // continuation-check.
    // A razão DECLARADA tem de ser a do gatilho que disparou. Com a calibração
    // por similaridade, um deny na 2ª chamada exibindo "90% das rajadas ≥4"
    // justificaria o bloqueio por uma estatística que não se aplica a ele — o
    // anti-padrão D8 (o anúncio promete o que o executor não fez) dentro do
    // próprio gate.
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
    // V0e1 (09/10/2026 — aprovado por Gabriel; medido pela 6b: G1 = 46% das
    // negações e menos da metade segue a rota): o gate EXECUTA a rota em vez de
    // negar — a rajada acumulada vira o `updatedInput` do `touring run` e o
    // executor a roda no sandbox (spike S-8.6: updatedInput honrado). Quem o
    // sandbox não alcança nem chega aqui (o check S24 acima devolve None → a
    // chamada original roda nativa: o fallback é estrutural, nunca um zero
    // silencioso). Kill switch humano: TOURING_G1_REWRITE_DISABLED=1.
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
    // um deny encerra o turno cego: a próxima chamada é a RESPOSTA do modelo
    // ao deny (o continuation-check A/B vive dela chegando ao burst_gate).
    st.pending_g1.insert(session.to_string(), class.to_string());
    Some(Verdict::deny(Gate::G1, format!(
        "[G1 rajada-de-inspeção] {motivo}. Rode-o como programa — os já executados da \
         rajada vêm como comentário e NUNCA são repetidos:\n  {programa}{hint}\nBypass \
         por-comando: prefixe {GATE_BYPASS_TOKEN} (reseta a janela e é contado)."
    )))
}


/// A cadeia de gates com o veredito MAIS SEVERO (30.4.97, ordem G5/G9): nenhum advisory
/// curto-circuita a cadeia. Os gates que só avisam depositam o texto em `avisos` e a avaliação
/// segue; o deny aplicável (qualquer posição na cadeia) vence, e o aviso vai junto no texto dele.
/// Corre ANTES do anti-spam do suggester: um deny repetido jamais pode ser engolido por ele.
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
    // Toda mutação avança a época do projeto (G6 nunca dispara através dela).
    if matches!(tool_name, "Edit" | "Write" | "NotebookEdit") {
        // G9 (N3a): a conversão do deny é o Edit com gates — a próxima
        // Edit/Write/NotebookEdit da sessão fecha o continuation-check.
        if st.pending_g9.remove(session).is_some() {
            record_gate_event(GateId::G9, GateEvent::Followed);
        }
        let epoch = st.mutation_epoch.get(&project).unwrap_or(0);
        st.mutation_epoch.insert(project.clone(), epoch + 1);
        // G5 (telemetria): a rajada de edits cresce; o fim dela é a primeira
        // ação Bash (onde o advisory único pode falar).
        let streak = st.g5_edit_streak.get(session).unwrap_or(0);
        st.g5_edit_streak.insert(session.to_string(), streak.saturating_add(1));
        // W3 S-3.1 — G3: Edit sem Read recente do arquivo, POR SESSÃO.
        // Write fica FORA do gate: criação não tem o que ler e overwrite já
        // exige Read no harness — o G3 negou um Write de criação vivo em
        // 24/08 (strategy doc). O conteúdo escrito é conhecido pela sessão,
        // então o Write REGISTRA o arquivo como lido (Edit seguinte passa).
        if let Some(fp) = tool_input.get("file_path").and_then(Value::as_str) {
            if tool_name == "Write" {
                st.g3_read_files.insert(g3_read_key(project_root, session, fp), ());
            } else if st.g3_read_files
                .get(&g3_read_key(project_root, session, fp))
                .is_some()
            {
                st.g3_streak.insert(session.to_string(), 0);
            } else if !ovr.code_gates_disabled {
                let n = st.g3_streak.get(session).unwrap_or(0).saturating_add(1);
                st.g3_streak.insert(session.to_string(), n);
                // Aperto 29/08 (ordem de Gabriel): 1 advisory e o 2º seguido
                // nega — editar sem ler é a origem dos edit_string_not_found;
                // um aviso basta.
                if n >= 2 {
                    record_gate_event(GateId::G3, GateEvent::Denied);
                    return Some(Verdict::deny(Gate::G3, g3_deny_reason(
                        project_root,
                        fp,
                        n,
                        tool_input,
                    )));
                }
                record_gate_event(GateId::G3, GateEvent::Emitted);
                return Some(Verdict::advise(Gate::G3, g3_advisory_text(
                    project_root,
                    fp,
                    n,
                    tool_input,
                )));
            }
        }
        // W6 S-6.3 — contrafactual em comentário adicionado (advisory, NUNCA
        // deny: prosa em comentário tem falso positivo real).
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
            return Some(Verdict::advise(Gate::E3,
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
        // M3 — o ledger (sessão, arquivo) do G3 já distingue arquivo novo de
        // releitura; um miss aqui É um arquivo distinto novo na janela.
        let g3k = g3_read_key(project_root, session, fp);
        let arquivo_novo = st.g3_read_files.get(&g3k).is_none();
        st.g3_read_files.insert(g3k, ());
        st.g3_streak.insert(session.to_string(), 0);
        if arquivo_novo {
            let distintos = st.m3_distinct_reads
                .get(session)
                .unwrap_or(0)
                .saturating_add(1);
            st.m3_distinct_reads.insert(session.to_string(), distintos);
            // Exatamente no limiar (== e não >=): 1 advisory por janela.
            if distintos == M3_DELEGATION_AT && !ovr.code_gates_disabled {
                crate::shared::gate_metrics::record_m3_delegation_advised();
                return Some(Verdict::advise(Gate::M3, format!(
                    "[M3 delegação] {M3_DELEGATION_AT}º arquivo DISTINTO lido nesta janela — \
                     exploração desta largura paga o isolamento de contexto: delegue a 1-3 \
                     subagentes read-only (Explore/general-purpose), cada um com objetivo + \
                     formato de saída + fronteiras, e receba só as conclusões (50 arquivos \
                     lidos voltam como 3 achados). Leitura larga paraleliza; ESCRITA fica \
                     serial no contexto principal — decisões paralelas conflitam. Cadeia \
                     dependente (passo 2 precisa do output integral do passo 1) também fica \
                     no contexto único. Advisory único por sessão/1h."
                )));
            }
        }
        // G4 (telemetria, nunca deny): leitura sem localizar antes.
        if st.g4_last_locate.get(session).is_none() {
            crate::shared::gate_metrics::record_g4_observed();
        }
        // W3 S-3.2 — G7: re-inspeção do MESMO alvo.
        let key = g7_key(project_root, session, fp);
        // 30.4.97 (G7G3G9): reler um arquivo que MUDOU desde a leitura que o ledger lembra não é
        // re-inspeção — é a releitura que o harness exige («File has been modified since read»)
        // antes do próximo Edit. Contar essa leitura fechava o círculo: python escreve o arquivo
        // (o G9 empurra para lá), o Edit fica obsoleto, e o Read que o destrava era o 3º.
        let stamp = g7_refresh_if_changed(st, project_root, key, fp);
        let n = st.g7_seen.get(&key).unwrap_or(0).saturating_add(1);
        st.g7_seen.insert(key, n);
        if !ovr.code_gates_disabled {
            if n >= 3 {
                record_gate_event(GateId::G7, GateEvent::Denied);
                let rota = r1_route_for(fp, ovr.r1_skeleton().as_deref());
                let edicao = g7_edit_route_note(fp, stamp.is_some());
                return Some(Verdict::deny(Gate::G7, format!(
                    "[G7 re-inspeção] {n}ª leitura de {fp} na janela — reler é o sinal \
                     mais direto de programa-faltando (campeões medidos: adw.py 39×, \
                     cli_suggester.rs 32×). Varra de uma vez (R1): {rota}.{edicao} \
                     Um `touring run` citando {fp} reseta o gate."
                )));
            }
            if n == 2 {
                record_gate_event(GateId::G7, GateEvent::Emitted);
                let rota = r1_route_for(fp, ovr.r1_skeleton().as_deref());
                return Some(Verdict::advise(Gate::G7, format!(
                    "[G7 re-inspeção] 2ª leitura de {fp} na janela — programa-faltando? \
                     R1 instanciado: {rota} (a 3ª leitura nega)."
                )));
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
    // G7 reset: um `touring run` citando um caminho zera a contagem daquele alvo.
    if cmd.contains("touring run") {
        for frag in path_fragments(cmd) {
            st.g7_seen.invalidate(&g7_key(project_root, session, frag));
        }
    }
    // G5 (telemetria): a primeira ação Bash encerra a rajada de edits; se ela
    // tinha >=3 e este comando não é validação, UM advisory (nunca durante).
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
            // Advisory: NÃO curto-circuita a cadeia (30.4.97) — um deny aplicável adiante
            // (G9 `sed -i` cego, G2, G6…) vence e leva este texto junto.
            avisos.push(Advisory::new(Gate::G5, format!(
                "[G5 telemetria] rajada de {g5} edits terminou sem build/teste — P9 \
                 medido em 17%. Valide agora (cargo check/test no crate tocado); a \
                 regressão silenciosa nasce aqui."
            )));
        }
    }
    // followed (S-1.4): houve deny de G2 nesta sessão e o comando agora traz
    // pipefail — a conversão canônica foi adotada.
    if st.pending_g2.remove(session).is_some() && cmd.contains("pipefail") {
        record_gate_event(GateId::G2, GateEvent::Followed);
    }
    // G11 — a rota sancionada devolve o crédito ANTES de qualquer cobrança, para
    // que um turno que alterna programa e bypass nunca acumule dívida.
    if is_sanctioned_route(cmd) {
        credit_sanctioned_route(st, project_root, session);
    } else if !has_bypass_token(cmd) {
        break_strict_run(st, project_root, session);
    }
    if has_bypass_token(cmd) {
        // O orçamento é cobrado ANTES da isenção: o token continua legítimo, mas
        // a SÉRIE passa a custar. O deny do G11 não é bypassável de propósito —
        // um bypass que se auto-libera não é orçamento; a saída é o kill switch
        // humano no env do daemon, que exige decisão fora do turno.
        let budget = charge_bypass(st, project_root, session);
        if !ovr.code_gates_disabled
            && let Some(rule) = bypass_verdict(budget)
        {
            record_gate_event(GateId::G1, GateEvent::Emitted);
            return Some(Verdict::deny(Gate::G11, format!(
                "[G11 orçamento de bypass] {rule}: {} seguido(s), {} desde a última rota, janela {}s. \
                 O token é por-comando, não uma rota. Use a rota sancionada — funda a \
                 família numa varredura única: `touring run --lang bash --file <script>` \
                 — e o crédito volta inteiro. Sem saída por prefixo: o kill switch é \
                 humano (`TOURING_CODE_GATES_DISABLED=1` no env do daemon + restart).",
                budget.strict_run, budget.since_route, G11_WINDOW_SECS
            )));
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
        // G1: o bypass consciente reseta a janela da classe (S-2.2) — a
        // exploração legítima recomeça do zero, e o evento fica contado.
        if let Some(class) = scan_class_of(cmd) {
            let key = burst_key(project_root, session, class);
            if matches!(st.burst_ledger.get(&key), Some((n, _, _)) if n >= G1_DENY_AT - 1) {
                record_gate_event(GateId::G1, GateEvent::Bypassed);
            }
            st.burst_ledger.invalidate(&key);
        }
        return None;
    }
    // G2 — heredoc é DADO (o corpo não executa como pipeline DESTE shell);
    // pular evita negar a escrita de um teste que apenas CONTÉM o padrão.
    if !cmd.contains("<<") && exit_code_through_pipe(cmd) {
        if ovr.code_gates_disabled {
            record_gate_event(GateId::G2, GateEvent::Bypassed);
            return None;
        }
        // S-8.6 (spike SUPORTADO, provado vivo 2026-08-25: o rewrite A4
        // `cat`→highlight trocou um comando real desta sessão): o Claude Code
        // honra `updatedInput` de hook de settings.json. O G2 então CORRIGE em
        // vez de negar — zero fricção, correção garantida pelo executor (o
        // teto da afordância). `Followed` é registrado junto com `Emitted`
        // porque a adoção é do executor, não uma esperança.
        if !ovr.g2_rewrite_disabled {
            record_gate_event(GateId::G2, GateEvent::Emitted);
            record_gate_event(GateId::G2, GateEvent::Followed);
            return Some(Verdict::Rewrite {
                gate: Gate::G2,
                reason: "[G2 exit-code-through-pipe → REESCRITO] `$?` após pipe \
                             leria o status do último estágio; `set -o pipefail; ` \
                             prefixado (spike S-8.6: updatedInput honrado).".to_string(),
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
        return Some(Verdict::deny(Gate::G2, format!(
            "[G2 exit-code-through-pipe] `$?` depois de um pipe lê o status do ÚLTIMO \
             estágio (tail/jq), não do programa medido — 68 casos na simulação de 55 \
             sessões; 3 cometidos pela própria sessão que desenhou este gate. \
             Reexecute exatamente:\n  {remedy}\nOu remova o pipe e leia o exit \
             direto. Bypass por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed)."
        )));
    }
    // G9 (N3a, 2026-08-26) — escrita cega inline negada com a rota derivada:
    // o advisory Pattern 5 falava e o comando executava igual (advisory ≠
    // gate — adoção medida de um não transfere para o outro). Heredoc é
    // dado (o corpo não executa neste shell). ANTES do G8: um laço com
    // `sed -i` é negado aqui, não reescrito para o sandbox.
    if !cmd.contains("<<") && is_inline_blind_edit(cmd) {
        if ovr.code_gates_disabled {
            record_gate_event(GateId::G9, GateEvent::Bypassed);
            return None;
        }
        record_gate_event(GateId::G9, GateEvent::Denied);
        st.pending_g9.insert(session.to_string(), ());
        let target = inline_edit_target(cmd).unwrap_or_else(|| "<o arquivo>".to_string());
        let cmd_remedio = if cmd.len() <= 1500 {
            cmd.replace('\'', r"'\''") // aspas escapadas — a rota cola no shell (lição G8)
        } else {
            "<o mesmo comando, verbatim>".to_string()
        };
        return Some(Verdict::deny(Gate::G9, format!(
            "[G9 escrita-cega-inline] `sed -i`/`awk -i inplace`/`perl -pi` editam às \
             cegas — zero dos 17 gates do Edit (VGP, blast_radius, TDG, snapshot \
             atômico, gotcha). Rotas:\n  \
             1 arquivo:   Edit tool em `{target}` (old_string → new_string, gates completos)\n  \
             mesmo comando, sandbox: touring run --lang bash --code '{cmd_remedio}'\n  \
             estrutural (N arquivos): touring ast grep {target} '<pattern>' --rewrite '<replacement>' \
             (ast-grep — metavars $VAR, NÃO regex de sed)\n  \
             Bypass por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed)."
        )));
    }
    // G8 — laço de INSPEÇÃO reescrito para o sandbox (2026-08-25).
    //
    // Medido no transcript desta própria sessão (1433 tool calls): das 453
    // oportunidades de code mode, as outras classes ou já estão cobertas ou
    // não compensam. Apertar o limiar da rajada é fricção sem ganho (a mediana
    // de rajada é 1); converter grep para o índice serviria a ~3% (56% dos
    // greps são regex estrutural, que o índice não responde). Os laços, não:
    // 44 casos reais em 47 detectados (94% de precisão), e o programa está
    // ESCRITO no comando — foi decidido inteiro e partido em N round-trips.
    //
    // O remédio é o do G2, que se provou vivo: REESCREVER, não negar. Só
    // laços de inspeção pura entram: o sandbox aplica capabilities por
    // comando — `run-1787620774532-346193` registrou "X6 denied the subprocess
    // capability 'echo' under profile 'Sandboxed'" —, então um laço que mata
    // processo, faz git ou compila fica de fora por construção.
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
                             CodeAct). Kill switch humano: TOURING_G8_REWRITE_DISABLED=1.".to_string(),
                context: None,
                updated_input: rewritten_input(tool_input, rewritten),
            });
        }
    }
    // S5 (29/08) — o par write→run: `cat >`/`tee` de um script registra o
    // alvo; a execução do MESMO path na janela conta um par. Nasceu do turno
    // de 60 Bash do `analise` (28 pares, todos invisíveis: a escrita é T4
    // sancionada e a execução avulsa não acumulava) — o G-turno só falou no
    // Stop, com o custo já pago. Aqui o colapso acontece DURANTE o turno.
    if let Some(path) = script_write_target(cmd) {
        st.written_scripts_ledger.insert(write_run_key(project_root, session, &path), ());
    }
    if let Some(deny) = write_run_pair_gate_with(st, project_root, session, cmd, ovr) {
        return Some(deny);
    }
    // G10 (S4, 2026-08-26) — rajada de execução homogênea (R4): N seriadas do
    // mesmo interpretador com 0 `touring run` na janela. O laço desenrolado
    // vira 1 programa (R9). Um `touring run` entre elas zera a contagem — a
    // condição "0 run" é a prova de que a rota já foi oferecida e ignorada.
    // DEPOIS do G8: um laço escrito é reescrito lá; aqui é a rajada serial.
    if let Some(class) =
        exec_class_of(cmd).filter(|_| unreachable_in_sandbox(project_root, cmd).is_none())
    {
        if class == "python-inline" {
            // telemetria S5: cada inline visto conta — a calibração segue
            // medindo mesmo com a classe agora dentro da rajada.
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
            // zera para a próxima rajada — um deny por lote, nunca fadiga
            st.exec_burst_ledger.invalidate(&key);
            // G10ROTA (30.4.97): qual rota do `touring run` EXECUTA este corpo? A pergunta é do
            // executor (D8) — o deny que nomeia um comando que o X6 recusa ensina a falhar.
            let rota = g10_inline_route(project_root, class, cmd);
            if !rota.runs() {
                return Some(Verdict::deny(Gate::G10, g10_unrunnable_deny(
                    n,
                    class,
                    &rota,
                    project_root,
                    cmd,
                )));
            }
            // python-inline tem remédio 1:1 (o corpo do PRÓPRIO comando); as
            // demais classes fundem a rajada acumulada via R9.
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
            // S6 — a rajada real consulta o portfólio: quando há um programa
            // que já funcionou para este trabalho, o deny entrega os dois
            // (o esqueleto agregado + o prior art instanciado).
            let prior = match &ovr.portfolio_dir {
                Some(dir) => touring_foundation::portfolio::store::load_from(dir)
                    .ok()
                    .and_then(|idx| portfolio_remedy_for_burst_on(class, &cmds, &idx)),
                None => portfolio_remedy_for_burst(class, &cmds),
            }
            .unwrap_or_default();
            // N6 — e a escada entrega o bloco parecido, quando existe.
            let hint = snippet_hint_for_program(&programa, project_root);
            let lead = g10_lead(&rota, project_root);
            return Some(Verdict::deny(Gate::G10, format!(
                "[G10 exec-burst] {n}ª chamada seriada do executor `{class}` (0 `touring run` \
                 na janela de {}s){lead}\n  \
                 {programa}\nSó o comando negado roda; os já executados da rajada vêm como \
                 comentário e NUNCA são repetidos (denies subprocess-only são advisory em \
                 `--lang bash` — o sandbox contém o filesystem); veredito na saída, íntegra \
                 no spill. Os próximos comandos do lote entram no MESMO programa.{prior}{hint} \
                 Bypass por-comando: prefixe {GATE_BYPASS_TOKEN} (contado como bypassed).",
                EXEC_BURST_WINDOW_SECS
            )));
        }
    } else if cmd.contains("touring run") || cmd.contains("touring exec") {
        if st.pending_g10.remove(session).is_some() {
            record_gate_event(GateId::G10, GateEvent::Followed);
        }
        for class in ["python", "pytest", "python-inline"] {
            st.exec_burst_ledger.invalidate(&exec_burst_key(project_root, session, class));
        }
        // S5 — a rota seguida zera o par write→run junto (mesma razão do S3
        // abaixo: sem isto o próximo script legítimo herdaria uma contagem de
        // um loop que o modelo JÁ converteu em programa).
        st.write_run_pair_ledger.invalidate(&write_run_key(
            project_root,
            session,
            "\u{0}write-run-pares",
        ));
        // S3 — a rota foi seguida: a janela de inspeção zera junto. Sem isto o
        // ledger seguiria contando uma rajada que o modelo JÁ converteu em
        // programa, e o próximo `grep` legítimo levaria um deny herdado de uma
        // rajada que não existe mais.
        for class in CODE_MODE_COLLAPSED_CLASSES {
            st.inspect_burst_ledger.invalidate(&inspect_burst_key(project_root, session, class));
            // G1 — o contrato da sessão ("um `touring run` na janela zera a contagem")
            // valia só para o S3: medido 09/10, uma rajada de 18:49 ainda era refundida
            // às 18:55 depois de um `touring run`.
            st.burst_ledger.invalidate(&burst_key(project_root, session, class));
        }
        st.inspect_burst_ledger.invalidate(&inspect_burst_key(
            project_root,
            session,
            PY_INLINE_INSPECT_CLASS,
        ));
    }
    // S3 (27/08/2026) — o colapso do modo `code` deixou de ser POR CLASSE e
    // passou a ser POR RAJADA. A 1ª inspeção de uma classe na janela executa
    // intacta; a 2ª em diante volta como UM programa com as duas fundidas.
    //
    // O que mudou e por quê: o braço P2c negava na PRIMEIRA chamada de
    // `grep`/`cat`/`find`. Medindo 115 transcripts (`s3_burst_distribution.py`),
    // 22,5% dessas chamadas são isoladas — para elas o deny cobrava um
    // round-trip inteiro e devolvia a MESMA leitura, que é a definição de
    // taxar o caso comum. Os outros 77,5% estão em rajadas ≥2, e esses o
    // predicado continua pegando, agora com o programa fundido em vez do
    // comando único reescrito.
    //
    // O predicado também é o que finalmente ENTREGA a promessa que o T3-B fazia
    // e nunca cumpriu (`t3_turn_fused = 0`): "a 1ª executa intacta, as K−1
    // voltam como 1 programa". O T3-B pendurava isso no fechamento de turno, um
    // evento que este modelo de execução não produz; aqui é um contador com
    // TTL, que ocorre. Mutação/build seguem passando (scan_class_of só
    // reconhece inspeção), e um `touring run` na janela zera o ledger — a
    // contagem prova que a rota foi oferecida e não usada.
    let apresentacao = resolve_presentation(project_root, cmd, &ovr.code_mode_env);
    // Aperto 29/08 (ordem de Gabriel): python-inline READ-ONLY é inspeção e
    // cai na MESMA rajada — usado como leitura, o interpretador ganhava 4
    // passes onde `cat` ganha 1 (o G10 só nega na 5ª/600s). A classificação é
    // pelo CORPO (deny-list conservadora): qualquer indício de escrita/rede/
    // subprocesso mantém o comando no G10 — falha na direção frouxa, nunca
    // roteia um escritor ao deny de inspeção.
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
        // D (09/10/2026, ordem do Gabriel — §8 da V0e5): a rajada AVISA, nunca
        // nega. Medido: 1.272 negações do S3 em 24 h, o maior gate de fricção. O
        // lote NÃO zera no aviso — as seguintes da mesma rajada contam em
        // silêncio (um aviso por rajada, nunca fadiga); só um `touring run` ou a
        // janela sem inspeção o rearmam (o TTL renova a cada inserção).
        st.inspect_burst_ledger.insert(key, (n, cmds.clone()));
        if n < INSPECT_BURST_ADVISE_AT {
            // A isolada passa em SILÊNCIO: um nudge aqui devolveria pela porta
            // dos fundos a fricção que o predicado acabou de tirar da frente.
            crate::shared::gate_metrics::record_g1_inspect_first_passed();
            bump_durable_evidence(project_root, "s3", "first_passed");
        } else if n > INSPECT_BURST_ADVISE_AT {
            bump_durable_evidence(project_root, "s3", "in_burst");
        } else {
            // python-inline funde os CORPOS (o remédio 1:1 por comando já
            // existia; a rajada junta-os na ordem); as classes shell do S3 seguem
            // no R9 via fuse_burst_program. O G1 NÃO usa mais o fuse_burst_program:
            // ele manda os já executados como comentário (S212) e só o comando que
            // disparou como passo.
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
            // N6 — a escada entrega o bloco parecido, quando existe: a rajada
            // avisada vira descoberta, não só rota.
            let hint = snippet_hint_for_program(&programa, project_root);
            // Advisory: segue a cadeia até o G6 (30.4.97) — a repetição byte-idêntica que o G6
            // nega vence o aviso. O G1 NÃO volta a decidir esta chamada (`s3_avisou`): na
            // 2ª inspeção a rajada só AVISA, por ordem do Gabriel (09/10/2026, D).
            avisos.push(Advisory::new(Gate::S3, format!(
                "[CODE MODE · rajada] aviso — {n}ª inspeção `{class}` em {}s, executada \
                 nativa. Se a próxima também for inspeção, rode o lote de uma vez:\n  \
                 touring run --lang {lang} --code '{programa}'\n{nota_omissao}{hint}",
                INSPECT_BURST_WINDOW_SECS
            )));
            s3_avisou = true;
        }
    }
    // `native` cala a indução inteira: o escopo declarou que não quer ser
    // empurrado, e um nudge que ele não pediu é o custo sem a contrapartida.
    if apresentacao == CodeModePresentation::Native {
        return None;
    }
    // G6 — repetição byte-idêntica dentro da janela TTL, sem mutação no meio.
    // ANTES do T3-B: retry cego byte-idêntico é o sinal mais específico e o G6
    // o consome inteiro (advisory → deny); o que passa daqui é inspeção NOVA.
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
                    // O G6 CONSOME a repetição idêntica inteira (advisory → deny): o G1 adiante
                    // trata inspeção NOVA, então este advisory encerra a cadeia — por desenho.
                    return Some(Verdict::advise(Gate::G6, g6_advisory_text(cmd)));
                }
                record_gate_event(GateId::G6, GateEvent::Denied);
                // um deny encerra o turno cego: a próxima chamada é a RESPOSTA
                // do modelo ao deny, não continuação de um batch paralelo.
                return Some(Verdict::deny(Gate::G6, g6_deny_reason(cmd)));
            }
            // época mudou: a árvore mutou desde a primeira vista — fresh.
            Some((_, _)) => st.g6_seen.insert(h, (0, epoch)),
        }
    }

    // S10 (2026-08-27) — o T3-B foi REMOVIDO daqui. Medido ao vivo: 3 chamadas
    // Bash de classes distintas no mesmo turno deram `t3_turn_first_passed = 3`
    // e `t3_turn_fused = 0` — cada uma era "a primeira", porque o PostToolUse
    // fecha o turno entre elas. O gate mantinha estado por sessão e NUNCA
    // decidia. O predicado de rajada do S3 entrega o que ele prometia, com uma
    // janela de TEMPO (300s) em vez de um turno que sempre fecha.
    // G1 — rajada de inspeções atômicas da MESMA classe (W2 teeth): decidida
    // depois do G2 (o defeito de leitura vem antes do hábito) e antes do G6.
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

