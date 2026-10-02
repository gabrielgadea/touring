---
type: AuditReport
title: Cross-audit — bundle Jev/harness-potencialização (execução completa)
description: Auditoria cruzada com prova executada de TODOS os 22 entregáveis do bundle 2026-09-30-jev-harness-potencializacao. 2 bugs reais encontrados e corrigidos (build_answer TypeError em noul/score; 500 opaco→422 nomeado), F2.4 UNVERIFIED→Pass, F3.1 0/0.6/0.462→1.0 nos 4 scripts, 39 testes de script verdes + 2.624 testes cargo verdes. Prova de produção do insight_key em curso (watcher armado).
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [cross-audit, jev, laya, purpose-fidelity, e2e-proof]
timestamp: 2026-10-01T19:10:00-03:00
okf_version: "0.1"
---

# Cross-audit 2026-10-01 — bundle Jev/harness potencialização

Escopo: **tudo** que o bundle `2026-09-30-jev-harness-potencializacao` entregou
(sessão 5956fbbc + touring-02). Método: 7 fases, prova executada, pausa humana
antes do FIX (aprovado por Gabriel: "prossegue (fix 1–5)"), veredito por evidência.
Blind critic externo: touring-02 (achados F2.4/F3.1/checkpoints — confirmados
independentemente aqui).

## VERDICT

**PASS com 2 bugs reais encontrados e corrigidos no próprio audit** — exatamente
o que um cross-audit existe para fazer. Todas as provas abaixo foram EXECUTADAS
nesta sessão; o que não pôde ser executado está marcado IN PROGRESS ou UNVERIFIED.

## SCORECARD (50-dim, gates reais executados)

| Gate | Antes | Depois | Evidência |
|---|---|---|---|
| F2.4 (segredos) `jev_client.py` | 0,5 Warn (UNVERIFIED) | **1,0 Pass** | pragma sancionado + justificativa no cabeçalho (a chave é lida de arquivo, nunca literal, nunca logada) |
| F3.1 (testes) `g2_calibration.py` | 0,0 | **1,0 Pass** | `scripts/test_g2_calibration.py` (4 testes vendados) |
| F3.1 `laya_student_server.py` | 0,0 | **1,0 Pass** | `scripts/test_laya_student_server.py` (7 testes) |
| F3.1 `s54_replay.py` | 0,0→0,6 | **1,0 Pass** | `scripts/test_s54_replay.py` (5 testes incl. `main()` E2E vendado) |
| F3.1 `g2_baselines.py` | 0,462 | **1,0 Pass** | +8 testes (load_jsonl, backend_current_intent×2, backend_professor, backend_laya, predict_choice, predict_noul, subsample, summarize) |
| Suíte scripts | 6 testes | **39 pass, 0 fail (0,11s)** | pytest executado 2× (RED 5 → GREEN 39) |
| Suíte cargo (3 crates tocados) | — | **2.624 pass, 0 fail** (616+459+1549) | `cargo test -p touring-intelligence -p touring-hook-runtime -p touring-cli --lib` EXIT=0 |
| cargo check / clippy -D warnings | — | EXIT=0 limpo | blind critic (02), conferido |
| e2e | — | 0.856 pass, 84 testes | blind critic (02) |
| Orphans dos símbolos novos | — | **NENHUM** | `touring wiring orphans -j`: insight_key, append_json_line, student_classify, journal_intent_decision todos wired |

## FINDINGS — os bugs que SÓ o audit pega (teste de unidade não pegou)

### F1 [REAL, produção] `build_answer` TypeError em TODA resposta noul/score
`laya_student_server.py:43`: o lambda `get` aceitava 1 arg, mas era chamado com 2
(`get("values", {})`). Qualquer resposta sem `choice` top-level → TypeError → 500.
**Invisível em produção** porque o único tráfego vivo é a pergunta Choice de intent
(com `choice` presente, o `or` curto-circuita) — o servidor declara o wire
/v1/systemone (3 primitivas) e 2 delas caíam. Encontrado por teste novo da
primitiva noul (o edge case que o *propósito* implica, não o que o autor pensou).
**Fix (raiz)**: lambda com default + `(get("values") or {})`. Prova: RED→GREEN.

### F2 [REAL] 500 opaco em body malformado → 422 que nomeia o campo
`questions` como lista caía em AttributeError→500 sem nomear o campo. Fix:
`parse_systemone_body()` (nova — validação do wire com erro que ENSINA, A5) +
`traceback.print_exc()` no branch 500 (fail-LOUD: antes o traceback nem logava).
Prova: 5 testes do contrato (body válido / JSON quebrado / questions lista / spec
não-dict / questions ausente).

### F3 [teste errado, não código] soma de support = 0,9999
`curve_for_choice` arredonda support por faixa (`round(x,4)`) — desenho. Meu teste
exigia soma 1,0 exata. Decisão consciente (skill: "decide qual dos dois está errado
antes de tocar"): o teste cedeu (`abs(sum-1.0)<0.001`).

### F4 [P3, corrigido] `load_key(str)` → AttributeError opaco
Assinatura `Path | None` quebrava com str. Ampliada para `Path | str | None` com
coerção (retrocompatível — os 2 call sites existentes seguem; pytest confirma).

### F5 [P3, corrigido] `s54-replay/report.md` ausente
Drift OKF — results.json sem o report. Escrito com os números + a decisão derivada
(rerank vence em direção, gargalo é recuperação — por isso a flag é opt-in).

### F6 [decisão Gabriel — pendente] checkpoints/ 224 KB versionados
141 KB são 2 cópias do mesmo `prompt_enhance.rs`. Gabriel optou por não decidir
agora (ficou fora do "fix 1–5"). Registrado, sem ação.

## E2E PROOF — o que foi executado NESTA sessão (não citado)

| Prova | Comando/ação | Resultado |
|---|---|---|
| Jev API real | `jev_client.system_one` (sky→blue) | `choice=blue, conf 1.0`, 494,7ms, 297 tok — journal `route_taken:api` |
| Cache Jev | mesma chamada 2× | 2ª = `route_taken:cache`, 0 tokens |
| Fail-open | `key_file` inexistente | `None` sem exceção + journal `{fallback_reason:"no_key"}` |
| Aluno Laya warm | 5 POSTs /v1/systemone | **p50 9,9ms p99 26,9ms** (claim W42: 14,9/25,6) · probs somam 1,0 |
| τ-gate do aluno | 6 prompts ad-hoc | todos maxp<0,6 → recusa calibrada (dia-0: 1/16 claims, suporte 22,5% medido) |
| pre_review_triage | `--git-ref HEAD` | 7 Nouls tipados, advisory, exit 0 |
| w45 report | regenerado agora | jev 1.183 calls p50 367,1ms · recall 242/0 measured · intent laya@0,6011 |
| skill_picker | journal lido | `said:true` (sugeriu **esta auditoria**, conf 1.0) + `said:false` com `silent_reason` incl. `hook_fail_open:JSONDecodeError` (fail-open provado em produção) |
| recall journal | lido | schema per_query {candidates, injected} — 47 candidatos/sessão |
| RED→GREEN | pytest 2× | 5 failed → 39 passed |

## IN PROGRESS (não verificado por completo — honestidade de gate)

- **insight_key em produção**: o fix está em source + binário dev (11:37) + testado
  (upsert collapse: 2 emissões → 1 row, access_count 2). A prova de PRODUÇÃO depende
  de um bridge MCP nascer pós-build — o writer único atual (bridge stale da sessão
  touring-02, código de 30/09) segue na grade :03. Watcher armado (exit 0 em
  access_count≥3 na chave `insight:cila_progression:a924b810a5bc`). Causa raiz
  fechada bilateralmente (a 02 confirmou: writer é quem executa, não quem compila).
- **Item 6** (checkpoints): decisão de Gabriel, fora do escopo aprovado.

## HARMONY

- Órfãos: nenhum entre os símbolos novos. Debt scan: 0 findings nos arquivos tocados.
- Pausa humana registrada: Gabriel aprovou "fix 1–5" (item 6 excluído por ele).

## ACTIONS

1. ~~F2.4 pragma~~ · ~~F3.1 ×4~~ · ~~build_answer~~ · ~~422~~ · ~~load_key~~ · ~~s54 report~~ — tudo executado e re-medido acima.
2. Aguardar PROOF-COMPLETE do watcher (ou medição sob demanda quando a 02 reiniciar).
3. Após a prova: cortar **30.4.90** (sequencing acordado: prova no dev → release → segunda medição no caminho toolchain).
4. 08/10: regenerar `w45_production_report.py` com o braço laya ativo (opt-in de Gabriel pendente).
