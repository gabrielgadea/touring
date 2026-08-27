---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-26-code-mode-afordancia-deep
tags: [loop, log]
timestamp: 2026-08-26T12:44:48.345189-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-26T15:41:50.441942-03:00 — S1 done

Classificador alem do 1o token: resolved_tokens/skip_wrapper/wrapper_spec unicos, consumidos por 5 sitios (scan_class_of, is_scan_command, parse_grep_command, is_memory_search, classify_bash P1). Wrappers env/time/nice/sudo/command/builtin/timeout/stdbuf/ionice/taskset/chrt transparentes; exclusoes preservadas (escrita, involucro puro, -n do nice != -n do sed). 4 testes novos; 440 passed 0 failed; clippy -D warnings limpo; guard D8 12 passed (via pytest — python3 direto coleta 0).

## 2026-08-26T15:58:39.952357-03:00 — N3a done

G9 gate escrita-cega-inline: predicado is_inline_blind_edit ancorado em posicao de comando (echo "rode sed -i" e prosa; find -exec/xargs documentados como lacuna). Deny carrega rotas derivadas (Edit no alvo real + mesmo comando no sandbox com aspas escapadas + ast grep --rewrite). GateId::G9=6 em foundation (array 6->7, label g9_sed_inline, all() aditivo). Followed/Bypassed/Denied nos 4 eventos. 3 testes novos (deny+rotas, variantes/poupas, bypass). BONUS: 2 testes de config/paths falhavam por /tmp/.touring (debris do strategy-loop de probes 13:28) — quarentenado, 11/11 verde. cli 443 + foundation 484 passed, 0 failed; clippy limpo.

## 2026-08-26T16:10:55.828508-03:00 — N5 done

KPI injecao nativa medido nas 2 pontas. MINERADOR (scripts/n5_injection_kpi.py + 5 testes): baseline de 31 transcripts — overall 52.5% seguida (313f/283r), serie diaria 2.2%→44.7%→77.5%→33.3%, code_route explodindo (79→395). Relatorio OKF no bundle (n5-injection-kpi.md). ESTRUTURAL: 3 contadores (native_injection_{followed,resisted,code_route}) em gate_metrics + snapshot, record_native_injection no hook ANTES dos gates (escolha, nao execucao); classificador puro native_injection_class espelha o python 1:1 (paridade guardada pelos mesmos casos nos 2 lados). Ressalva documentada: 'followed' inclui grep legitimo em pipe — e proxy de pressao. cli 444 + foundation 485 passed; clippy limpo.

## 2026-08-26T16:17:43.447935-03:00 — S3 done

VERIFICADO: G-turno ja entrega o remedio derivado — fuse_burst_program (comando inteiro ou fora, orcamento 3000 declarado, aspas escapadas) + deny T3-B/G1 carrega o programa real da rajada (vivido ao vivo nesta sessao: meu deny G1 trouxe os 4 comandos verbatim). Cobertura existente: 5 testes fusao_do_remedio + turn_gate_first_wins E2E. DELTA S3: teste E2E do caso historicamente quebrado (aspas simples atravessando o caminho ate o deny JSON) — pegou 1 defeito na MINHA assercao (a forma escapada contem o padrao cru como substring; a negativa correta e a forma quebrada). 445 passed, 0 failed.

## 2026-08-26T16:29:06.716768-03:00 — S5a done

Advisory CEG fora do stderr: gate_run retorna Option<CegRunAdvisory> (warn->debug forense); emit_output insere campo ceg_advisory no JSON (full) e no summary (brief, via to_value — shape aditivo). PROVA E2E: ./target/debug/touring run --lang bash --code 'echo hi' → stdout JSON com ceg_advisory {composite 0.675, reason X6, note}, stderr com ZERO ocorrencias do advisory (resta 1 linha INFO enforcement — aviso de capacidade, fora do escopo nomeado). Teste do produtor (gate_run bash echo hi → Ok(Some)). server 1527 passed; clippy limpo.

## 2026-08-26T16:46:48.564558-03:00 — S4 done

G10 exec-burst + R9: exec_class_of (python/pytest via effective_tokens; -c e mutacao marcada fora por construcao; wrappers/cd transparentes via S1) + ledger 600s/16cap + deny na 10a com r9_exec_program (array JSON=literal Python valido; subprocess sequencial mesma ordem; digest com cauda so nas falhas; integra no spill). GateId::G10=7 (array 7->8). Deny zera o ledger (um por lote, nunca fadiga); touring run zera a contagem (condicao '0 run') e fecha Followed. 5 testes (classe, programa R9, decima-nega, reset, bypass). LIVE PROOF: programa R9 com 3 comandos reais (2 ok+1 falha) digeriu RESUMO: 2/3 ok. BONUS S1-complemento: prefixos cd e operadores de sequencia (a lacuna que a estrategia nomeou e eu deixei passar) + paridade no espelho python. cli 451 + foundation 485; clippy limpo.

## 2026-08-26T16:58:14.469498-03:00 — S6 done

Portfolio no remedio do G10: exec_burst_intent derivado da rajada real (classe + COMPONENTES de trabalho — dir names + stems sem extensao/digitos; caminho inteiro diluia o BM25: 6 termos -> required_matches 3 -> 2 matches -> portfolio calado com o artefato certo na prateleira, achado vivo pelo teste) + portfolio_remedy_for_burst (cache mtime existente; entry_point instanciado; placeholder '<' -> None honesto; fail-open total). Deny G10 entrega R9 + prior art quando ha. 4 testes (intent, fail-open, positivo scratch via TOURING_PORTFOLIO_DIR, E2E no deny). cli 455 passed; clippy limpo.

## 2026-08-26T17:07:47.297530-03:00 — S5bc done

KPI gate-fatigue nas 2 pontas. ESTRUTURAL: GateFatigue computado no capture() do snapshot (ratio=bypassed/(denied+bypassed) por gate e global; None sem volume — Lei L2; fp_candidate=ratio>0.20 com volume>=10 — o KPI que a revisao S5c consome). Teste baseline-delta+autoconsistencia. MINERADOR: N5 conta TOURING_GATE_OK=1 por sessao/dia (24 usos no corpus). Baseline do eixo injecao RECALCULADO com o classificador S1-completo (cd/segmentos): 24/08 119->251 followed — o numero antigo subcontava; documentado no relatorio. INCIDENTE de edicao no meio: minha insercao roubou derive+doc do GateMetricsSnapshot (anchor colado) — pego pelo deny(missing_docs); corrigido e guardado pela suite. foundation 486 passed; clippy limpo; pytest 7/7.

## 2026-08-26 18:50 — loop pausado em HUMAN GATE (não abandonado)

7/8 subtasks done; convergência exit 1 exclusivamente por S2 (piloto analise).
S2 exige deploy (bump + update-touring + toolchain + restart de 2 daemons)
com 2 sessões CC vivas — classe "ação outward/irreversível" → decisão do
Gabriel (opções A/B/C no RETOMAR-AQUI.md). Marker arquivado para a pausa
humana não virar 30 continuations de falso positivo; o estado completo está
persistido (DAG task_1787767900017576294, memória retomar:…:2026-08-26,
RETOMAR-AQUI.md, este bundle). Retomada: decisão do piloto → deploy → medir
adoção >60% + fatigue live → convergência final.

## 2026-08-26T17:26:06.232433-03:00 — S2 done

PILOTO ATIVO. Deploy: propagate-release 30.4.15 (gates workspace+clippy+mirror verdes, update-touring, toolchain freeze, default, analise+konverter atualizados e verificados lock=30.4.15). Config: [code_mode] mode=code no analise toml. PROVA COMPORTAMENTAL 8/8 no binario do piloto (regra 10 — nunca rotulo): grep→deny CODE MODE; time grep→deny (S1 wrappers); cd /x && cat→deny (S1 cd); sed -i→deny G9; sed -n isolado passa; ceg_advisory no JSON + stderr limpa (S5a); gate_fatigue + native_injection_* no snapshot do daemon do analise. ABERTO POR CONSTRUCAO: gate adocao elegivel >60% se mede nas proximas sessoes do analise — plano de medicao no bundle (s2-piloto-medicao.md): baseline adoption 0/10/42, injecao 52.5-63.7%, fatigue x24.
