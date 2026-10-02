---
type: Log
title: "Log — chronological history of this loop run"
description: "Append-only history; PreCompact resume notes and phase closes land here."
plan_id: 2026-10-01-jev-harness-r2
okf_version: 0.1
tags: [loop, log]
timestamp: 2026-10-01T19:53:17.860117-03:00
---

# Log

Part of the [bundle](/index.md).
# log — jev-harness-r2

## 2026-10-01 20:10 BRT — OUTER → plano → DAG (rodada 2 completa até o gate)

- OUTER: diagnostic 19:53 · CCE convergido (2 rodadas secas) · NotebookLM 5 queries · docs relidos · T1+T2 da 02 integradas.
- Descoberta central: suporte 22,5% = sub-confident calibration drift (a doc NOMEIA e prescreve T<1); starvation >0,95 = critério ARMADO (braço OFF por opt-in), não violado.
- Estratégia aprovada por Gabriel (W1-W10) → plan.md Pln2: validador 0 erros/0 warnings, confidence_ratio 1.0, gap_detector 0 gaps.
- DAG task_1790895761205640116: 10 subtasks; S-4 depende de S-3; S-1 marcado hazy (protótipo do bundle); 02 tem S-4 + metade de S-3 (decision.rs).
- Próximo: execução W1-W3 antes de 08/10 (w45 com os números novos).

# log — jev-harness-r2

## 2026-10-01 20:10 BRT — OUTER → plano → DAG (rodada 2 completa até o gate)

- OUTER: diagnostic 19:53 · CCE convergido (2 rodadas secas) · NotebookLM 5 queries · docs relidos · T1+T2 da 02 integradas.
- Descoberta central: suporte 22,5% = sub-confident calibration drift (a doc NOMEIA e prescreve T<1); starvation >0,95 = critério ARMADO (braço OFF por opt-in), não violado.
- Estratégia aprovada por Gabriel (W1-W10) → plan.md Pln2: validador 0 erros/0 warnings, confidence_ratio 1.0, gap_detector 0 gaps.
- DAG task_1790895761205640116: 10 subtasks; S-4 depende de S-3; S-1 marcado hazy (protótipo do bundle); 02 tem S-4 + metade de S-3 (decision.rs).
- Próximo: execução W1-W3 antes de 08/10 (w45 com os números novos).

## 2026-10-01T20:13:32.925192-03:00 — S-1 done

W44 temperature scaling: FITTED=None no piso 0,90 (precisao val 0,84-0,86 em todo T; acc global 0,625 — invariante sob T). Curva medida: T=0,8 dobra suporte (0,267->0,458) @ prec 0,855. O 0,926 da estrategia era precisao@suportado (reproduzido 0,917@T=1,2). Config INTOCADA; decisao do piso escalada a Gabriel (0,90 fica | 0,85 -> T*=0,8 | 0,7 meio-termo). 4 testes verdes; instrumento auditado (target soft vs label duro).

## 2026-10-01T20:17:22.406961-03:00 — S-10 done

Uncertain como outcome de 1a classe (self-consistency cookbook): UNCERTAIN_BAND (0.4,0.6) + outcome_of() em pre_review_triage.py; fast path so com todas 'no'; outcomes no payload; docstring das 3 bandas em g2_calibration. Prova viva: HEAD concurrency 0.48 -> 'uncertain' (antes binario forçado). 8 testes verdes.

## 2026-10-01T20:30:26.583022-03:00 — S-2 done

Shadow-matrix VIVA em producao: laya_shadow journala o que o aluno TERIA dito (flag OFF inclusive), decisao inalterada. parse_student_answer_raw (sem tau) + journal_student_shadow + student_classify sempre-pergunta; 460/460 testes crate; prova viva: concordancia (ANALYSIS=ANALYSIS) e discordancia (ANALYSIS x GENERAL) capturadas no mesmo ts_ms. Bonus achado na prova: cold-start 685ms > 200ms timeout do hook perdia shadows — warmup predict no boot do server, provado na 1a chamada.

## 2026-10-01T20:48:29.230395-03:00 — S-3 done

latency_ms VIVO: decision journal E shadow lines carregam o campo (prova viva: 36ms nas duas, mesmo ts). classify_with_details_timed wrapper (assinatura original intacta), gate single-sourced em parse_student_answer. Race de envs na suite (3 tocadores process-wide) morta com JOURNAL_ENV_LOCK compartilhado — 460/460 x3, clippy limpo.

## 2026-10-01T23:01:10.583520-03:00 — S-5 done

w46_teacher_cascade: dry-run REAL com 8 prompts de producao — provenance 6 jev / 1 jev+professor_agree / 1 professor (a cascata de custo disparou como desenhado). LGPD guard por construcao (analise nunca lido) + teste. Default-arg-bind pego (suite chamou o professor REAL — resolucao em call time). 7 testes vendados em 0.08s. Arquivos r2 escritos; split congelado intacto; o full-400 fica para a wave C (input do retreino).

## 2026-10-01T23:04:56.453048-03:00 — S-6 done

Composite scoring VIVO: 7 Nouls + 3 Scores na MESMA chamada (a doc: perguntas extra são quase grátis), composite em código com pesos em touring.toml, prova 'retuning nunca re-chama o modelo' (1 call, 2 vereditos). Prova viva HEAD: security 0,0 / pattern 1,93 / quality 1,02 -> composite 0,3915. Teste de contrato 7->10 atualizado por desenho.

## 2026-10-01T23:12:38.225901-03:00 — S-7 done

Score-gate MEDIDO + wired: replay real 33q — score_gate tau=1.0 injeta 31,1% com MESMA eficacia do contexto cheio (top1 4, mrr 0,1212 — zero perda com 69% menos injecao); jev-rerank segue top1 5. Producao: _score_gate_with_jev no session_startup (TOURING_JEV_RECALL_MODE=score_gate, fail-open devolve todas), funcional OK (filtro+fail-open). Missing 20/39 fica no replay (lado sessao nao pode saber o que falta — honesto). 6/6 testes.

## 2026-10-01T23:58:58.067313-03:00 — S-8 done

Semantic bash gate VIVO: comando novel zdbt -> score 1,52/2 (aluno local 48ms) como advisory; conhecidos nunca chamam. 461/461 runtime + 129/129 handlers. A Saga do Deploy (3a vez no dia): o hook delega eventos ao DAEMON (codigo velho) quando ele esta no ar — a prova exigiu TOURING_NO_DAEMON=1. Em producao o gate so vive quando o daemon for atualizado (janela de propagacao do Gabriel). flag TOURING_CEG_SEMANTIC_GATE default-OFF.

## 2026-10-02T00:10:57.356598-03:00 — S-4 done

Starvation gate IMPLEMENTADO (opção b — a janela da 02 passou; ela revisa): starvation_gate_from em decision.rs — flag ON + fallback>0.95 em 7d = unmet + next_action (critério documentado de rollback); flag OFF = not_armed (o número é desenho); universo só decisões reais (laya_shadow NUNCA entra — contá-la fingiria suporte). 5 testes unit + o teste de montagem sobre os journals REAIS provando o campo na família. KPI daemon: acende quando o daemon atualizar (mesma janela de propagação do S-8).

## 2026-10-02T00:12:32.258661-03:00 — S-9 done

Invariante de taxonomia: check_flat_label_limit (teto 20 documentado, 77 flat = 0,425) com fail-loud no import nomeando a rota hierarquica beam-search. 2 testes (limite atual 8 OK; 20 passa, 21 falha nomeando a rota).


## 2026-10-02 00:15 BRT — EXECUÇÃO COMPLETA: 10/10 subtasks + juiz CONVERGED (standard) · rust-full com 1 falha sob investigação

- DAG task_1790895761205640116: 10/10 done. Cada S com phases/S-N.md + knowledge/S-N.json.
- Entregas: S-1 w44 (FITTED=None honesto; curva; expectativa da estratégia corrigida) · S-2 shadow-matrix VIVA em produção (+warmup 685ms) · S-3 latency_ms nas duas linhas · S-4 starvation_gate (revisão cega da 02 integrada; o defeito duplo da keyword_fallback_rate_from corrigido por ela — shadow ligava flag_active; 0,9735→0,9932 real) · S-5 corpus cascade LGPD-safe (dry-run 8 prompts: 6 jev/1 agree/1 professor) · S-6 composite scoring (pesos touring.toml; retuning nunca re-chama) · S-7 score-gate (31,1% share, mesma eficácia; TOURING_JEV_RECALL_MODE=score_gate) · S-8 semantic bash gate (zdbt → 1,52/2 advisory; TOURING_NO_DAEMON para a prova — o hook delega ao daemon) · S-9 invariante ≤20 · S-10 uncertain outcome.
- Juiz (standard): CONVERGED exit 0 — judge_intact ✓ dag 10/10 ✓ Platinum 0.926 ✓ 0 P0 ✓ escopo inteiro ✓ orphans 761≤761 ✓ cargo ✓ · cross_audit N/A.
- Juiz --rust-full: cargo test workspace rc=101 — 1 falha sob investigação (REGRA #21: corrige-se 100% do que aparecer).
- e2e: 0.8575 pass (baseline 0.856 — sem regressão).
- Testes do r2: 50 pytest + 461 hook-runtime + 621 cli + 129 handlers, tudo verde. P0 dims: 10 arquivos × 6/6.
- 3ª ocorrência da lei "quem EXECUTA não é quem compila" (bridge, shim/toolchain, hook→daemon). Em produção, S-4/S-8 acendem na janela de propagação do daemon (Gabriel).
