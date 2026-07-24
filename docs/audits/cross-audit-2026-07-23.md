---
type: CrossAuditReport
title: "Cross-audit — Flow Enforcement E1-E4 (armador + gate de manifest + ADW strategy-loop + KPI compliance)"
description: "Auditoria de purpose-fidelity das 7 fases sobre tudo que foi implementado em 23/07/2026; 4 findings encontrados E corrigidos no próprio audit; evidência executada em cada claim"
tags: [cross-audit, flow-enforcement, loop-engineering, purpose-fidelity]
timestamp: 2026-07-23
plan: /docs/plans/touring-productization-pln2/00-INDEX.md
---

# Cross-Audit 2026-07-23 — Flow Enforcement E1-E4

## 1 · VERDICT

**PASS — com 5 findings reais, TODOS corrigidos e com regressão em teste permanente: 4 encontrados nas fases 1-4 e o 5º (F5) capturado pela própria prova final da fase 6 — o audit auditou a si mesmo.** O mecanismo cumpre o propósito documentado ("fluxos gated se auto-garantem por artefato, nunca por narrativa") e foi provado E2E **nesta própria sessão**: este relatório é o artefato que destrava o Stop guard armado na FASE 0. Um único item permanece honestamente `UNVERIFIED` (abaixo).

## 2 · SCORECARD

| Eixo | Resultado | Evidência executada |
|---|---|---|
| 50-dim (pós-fix) | arm 0.9498 · gate 0.9470 · snapshot 0.9430 · stop_guard 0.9403 · marker 0.9497 — **Platinum ×5** | `touring-quality score` por arquivo |
| kpi.rs | 0.9287 (≥ Gold; Silver por WARNs pré-existentes do arquivo — sem gaming) | idem |
| 6 P0 BLOCK | **6/6 PASS** em arm, gate e kpi.rs | `touring-quality check --gate F2.1/F2.4/F2.5/F2.6/F4.3/F4.5` |
| Testes | **23/23** test_flow_guard.py (novo, permanente) · **182/182** glob scripts Touring · **28/28** kpi (Rust) | pytest / cargo test |
| Débito | **0 marcadores** em hooks/ e adw-library/ ("the tree is clean") | `scan_debt.py` ×2 |
| Harmonia | todo símbolo novo com **2+ consumers** reais; settings.json JSON válido, comando correto | grep Cadeia 7 + jq |
| Sistema | e2e **0.8748 PASS** (80/84; 4 advisory pré-existentes já na fila ADW `task_1784818024931392697`) | `touring e2e -j` (cwd rust) |

## 3 · FINDINGS (all-breadth — 5 corrigidos + 2 observações)

**F1 · ALTA · CONFIRMADO POR EXECUÇÃO → CORRIGIDO.** O regex do armador (`/?` opcional, qualquer posição) armava o marker em prosa comum: `"o skill loop-engineering ficou ótimo"` → ARMOU; `"qual é o /goal disso tudo?"` → ARMOU. Fix: só formas genuínas de invocação — slash command ancorado no início do prompt (`^\s*/nome\b`) ou tag `<command-name>`. Regressão: 5 casos positivos + 5 negativos em `test_flow_guard.py` (todos verdes).

**F2 · MÉDIA → CORRIGIDO + POTENCIALIZADO.** `loop_snapshot.py` (PreCompact) com marker OUTER gravaria a key `loop-state:OUTER` — colidível entre projetos e sem valor de retomada. Fix: branch `snapshot_outer()` grava `flow-state:<flow>:<hash-do-cwd>` carregando exatamente o que falta no manifest (`missing` + `next_action` do gate) — a compactação agora preserva o estado ÚTIL da fase OUTER. Regressão em teste (monkeypatch prova a key).

**F3 · MÉDIA → CORRIGIDO.** A bateria de aceitação E1 (16 asserts) era ad-hoc em bash — sem arquivo permanente, regressão voltaria silenciosa (o exato anti-padrão que este trabalho combate). Fix: `test_flow_guard.py` (23 testes) cobrindo detecção, arming, gate, Stop guard block→allow, cap, mtime-floor, fail-opens e as regressões F1/F2/F4.

**F4 · BAIXA → CORRIGIDO.** `compliance.jsonl` crescia sem bound. Fix: `_trim_log()` (>2000 linhas → mantém 1000) — o KPI é medidor de comportamento recente, não arquivo morto. Regressão em teste.

**F5 · ALTA · ACHADO PELA PRÓPRIA PROVA FINAL → CORRIGIDO.** O edge-test "payload sem cwd" da FASE 2 revelou (na verificação de fechamento) que `arm()` com cwd ausente caía em `CLAUDE_PROJECT_DIR`/`getcwd()` — e **sobrescreveu o marker do projeto REAL** (flow cross-audit → strategy-outer) a partir de um payload de teste. Forense: marker com `flow: strategy-outer` + bundle preservado + `CLAUDE_PROJECT_DIR=~/.claude/rust`. Fix: payload sem cwd = malformado → no-op fail-safe (nunca adivinhar o projeto pelo ambiente do processo). Regressão `test_arm_without_cwd_is_noop_never_env_fallback` (24º teste). Nota de design mantida: um arm legítimo (com cwd) sobre marker outer de outro flow ATUALIZA o flow — o último fluxo invocado rege o contrato.

**O1 (observação).** kpi.rs fica Silver por WARN dims pré-existentes do arquivo inteiro (não regressão deste trabalho; floor Gold atendido). **O2 (observação).** O path DAG do Stop guard (branch antigo) foi re-provado intacto: marker ativo com task inexistente → allow + sidecar `.archived.json`.

## 4 · FUSED RISK

Nenhum P0. Risco residual único — **UNVERIFIED honesto**: o disparo do armador por um prompt REAL em produção não pôde ser observado porque hooks de settings.json só carregam em sessão nova (gotcha PROVADO na FASE 0: a invocação deste `/TACO-cross-audit` não armou o marker). Toda a cadeia foi provada por subprocess com payloads idênticos aos reais; a confirmação em produção acontece na primeira invocação de fluxo da próxima sessão.

## 5 · ROOT-CAUSE (dos 5 findings)

Padrão comum: **contrato implícito sem caso negativo testado**. F1: a bateria original testou só verdadeiros positivos do regex (recall) e nunca um falso positivo (precisão). F2 e F5: a mesma classe do "âncora de contexto errada" (root-cause dominante já catalogado — sharding por cwd, KPI no cwd do daemon, e2e cwd-sensitive): F2 por key sem namespace de projeto, F5 por fallback ao ambiente do processo quando o payload não traz cwd — e o padrão reapareceu uma 3ª vez NESTE audit quando o e2e mediu o HOME por reset de cwd e deu 0.635 falso. F3: prova-uma-vez ≠ prova-permanente. F4: crescimento sem bound é pending disfarçado.

## 6 · PROVENANCE (comandos executados)

`touring doctor -j` 6/6 · `loop_marker.py show` (gotcha) · `loop_marker.py write` + `loop_stop_guard.py` (block 1/5 → este relatório → allow) · grep consumers ×5 · `Read loop_snapshot.py` · bateria de edges (H1a/H1b ARMOU = F1; 4 fail-opens exit 0) · `scan_debt.py` ×2 (0) · `touring-quality score` ×8 + `check` P0 ×18 · `pytest test_flow_guard.py` 23/23 · `pytest test_*.py` 182/182 · `cargo test -p touring-cli --lib kpi` 28/28 · path DAG órfão → archived · `touring e2e -j` 0.8748 PASS (após corrigir a âncora de cwd) · `touring kpi -j` (flow.compliance_ratio vivo).

## 7 · ACTIONS

**Fechadas neste audit**: F1-F4 corrigidos + testados; suíte permanente criada; relatório-artefato fecha o gate (dogfooding do próprio mecanismo).
**Abertas (fila, sem pendência de código)**: (a) confirmar o armador em produção na próxima sessão (primeira invocação real de fluxo); (b) 4 advisory do e2e já roteados na fila ADW audit `task_1784818024931392697` (candle_bge CC=17, fastembed antipatterns, orphan-meter/integração — pré-existentes, fora desta árvore).

---
_Auditoria executada sob o próprio contrato auditado (marker `cross-audit` armado na FASE 0; este arquivo é o artefato de saída que o Stop guard exige). Institucional: `touring memory store cross-audit-flow-enforcement-2026-07-23`._
