---
type: AuditReport
title: "Cross-audit 28/08/2026 (noite) — estreia do KPI mutation, force-gate, RCA do lifecycle"
description: "Auditoria do delta pós-bump 30.4.17 (frontmatter OKF adicionado retroativamente em 30/08 — REGRA #21 do censo de audits sem frontmatter)."
tags: [cross-audit, kpi, mutation, lifecycle]
timestamp: 2026-08-28T22:00:00-03:00
---

# Cross-audit 28/08/2026 (noite) — estreia do KPI mutation, force-gate, RCA do lifecycle

> Escopo: o delta da sessão pós-bump 30.4.17 — commits `e7195cd` (propagate
> retry anti-transiente), `36faca8` (estreia kpi mutation + canal external +
> guard contrato×executor), `fdc9126` (force-gate + probe CARGO_HOME +
> instrumento do hang), `476cbba` (RCA do lifecycle) — mais o estado VIVO do
> daemon deployado. 20 arquivos, +2.767/−45. Toda alegação abaixo carrega o
> comando executado nesta auditoria (não narrativa).

## 1. VERDICT

**LIMPO — Diamond na média dos arquivos tocados (0,943–0,977), zero P0, zero
órfãos, zero débito novo, todas as provas E2E re-executadas verdes.** O único
achado da rodada foi contra o próprio auditor: dois falsos alarmes de
instrumento (`tail -1` cortando output; `unittest discover` sobre arquivo
pytest), ambos desmontados com o instrumento correto.

## 2. SCORECARD

| Gate | Resultado executado |
|---|---|
| 6 P0 BLOCK (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5) | `p0_fails=NENHUM` nos 7 arquivos Rust tocados |
| 50-dim composite por arquivo | kpi.rs 0,952 · handlers/mutation 0,977 · daemon.rs 0,943 · lifecycle/tests 0,968 · daemon_tests 0,977 · hooks-core/mutation 0,959 · server/mutation 0,970 — **todos ≥ Platinum** |
| Workspace (juiz de registro) | `loop_converged.py` exit 0 ×2 hoje — Platinum 0,9183, orphans = baseline 5469 |
| KPI vivo | 20 PASS / **0 FAIL** / 3 STUB honestos (amostra < 20) / 11 advisory |

## 3. FINDINGS (toda a largura)

| # | Severidade | Achado | Desfecho |
|---|---|---|---|
| 1 | — | DEBT SCAN: 13 arquivos de código do delta, **0** TODO/FIXME/HACK/`unimplemented!`/`allow(dead)` | limpo |
| 2 | — | HARMONY: 9 símbolos novos (`split_handler_scope`, `resolve_external`, `EXTERNAL_STALE_SECS`, `workspace_run_needs_force`, `workspace_requires_force_envelope`, `cargo_home_bin`, `has_subcommand_in`, `wait_doctor_clean`, `run_prova`) — **todos com consumidores reais** (1–6 usos cada) | REGRA #0 ✓ |
| 3 | instrumento | `python3 scripts/test_mutants_config.py \| tail -1` pareceu mudo — o runner próprio imprime 6×ok + PASSED, exit 0; pytest: 6 passed | falso alarme; lição re-paga |
| 4 | instrumento | `unittest discover` sobre `test_update_touring.py` → NO TESTS RAN exit 5 — o arquivo é pytest-style e o **CI usa pytest** (ci.yml:296); `pytest -q`: **10 passed** | falso alarme; guard vivo |
| 5 | residual conhecido | docs de teste antigos seguem no índice tantivy global (estancados por `476cbba`; limpeza = decisão humana, reindex apagaria event docs legítimos) | documentado, não regressão |
| 6 | pendência de terceiros | grafo release-TEST do touring-server (E0460 fingerprints stale, débito conhecido pré-existente do F4′) — fora do delta | inalterado |

## 4. FUSED RISK

Nenhuma unidade do delta combina defeito × blast: os arquivos de maior blast
tocados (daemon.rs, kpi.rs) receberam mudanças aditivas pequenas (1 braço de
match; resolvedor novo com guard estrutural) e pontuam 0,943/0,952 com P0
limpo. O risco residual do delta é ~zero; o risco real do sistema permanece
nos residuais pré-existentes (finding 5–6), ambos nomeados e com dono.

## 5. ROOT-CAUSE (a alavanca da sessão)

A classe de defeito unificadora do dia foi **"declaração ≠ executor"** em
quatro encarnações: fonte KPI declarada sem braço no resolvedor; probe de
binário mais estreito que a resolução real do cargo; client com floor menor
que o budget do server; fixture "isolada" cujo resolvedor de path tinha
fallback global. A alavanca institucional aplicada: guards que leem OS DOIS
lados (`every_declared_source_has_an_arm_and_every_derived_arm_a_commitment`,
`mutation_test_is_classified_heavy`, `fixture_root_normalizes_to_itself_not_home`)
— o contrato e o executor derivam da mesma fonte ou um teste reprova (D8).

## 6. PROVENANCE (comandos executados NESTA auditoria)

```text
prova_code_mode_ceg.py ................. RESULTADO: 35/35 asserções passaram
touring mutation-test (bare) ........... kind: workspace_requires_force | ok: False
touring mutation-test --cache-only ..... kill_rate 98.15 | killed 106 | timeout 0 | passed True
touring kpi -j ......................... mutation PASS 98.15 · test.count PASS 15786 ·
                                         coverage PASS 0.7528 · inspect_burst_share PASS 0.25
                                         summary: 20 PASS / 0 FAIL / 3 STUB / 11 advisory
touring doctor -j ...................... TODOS OK (7/7)
daemon environ ......................... TOURING_PILLAR_INDUCTION_ARMED=1; exe release/touring-daemon
touring-quality score ×7 ............... 0,943–0,977, p0_fails=NENHUM (audit_quality.sh)
cargo test -p touring-cli kpi .......... 37 passed; 0 failed
pytest test_mutants_config.py .......... 6 passed
pytest test_update_touring.py .......... 10 passed
debt scan (python, sandbox) ............ 13 arquivos, 0 marcadores
harmony scan (python, sandbox) ......... 9/9 símbolos com consumidores
suítes (nesta sessão, outputs exibidos): dispatch 1325/1325 paralelo 15,2s ·
  lifecycle 1247/1247 16s · cli 463+/0 · hooks-core 21/21 mutation · server 8/8
  mutation · clippy -D warnings limpo nos 4 crates · zero escritas no índice
  global durante a suíte (find -newermt vazio)
```

Enforcement verificado na fonte, não asserido: braço `cli-mutation-test` em
`invoke_handler` (kpi.rs); `| "cli-mutation-test"` no `is_heavy_hook`
(daemon.rs); recusa `workspace_requires_force` ANTES de `execute_and_cache`
(handlers/mutation_test.rs); `.git` na fixture `make_runtime` (lifecycle/tests.rs).

## 7. ACTIONS

1. **Nenhuma correção pendente do delta** — FASE 5 fechou vazia.
2. Humano decide: limpeza dos docs de teste residuais no índice global
   (`touring tantivy reindex` do root global apaga event docs legítimos junto).
3. Humano decide: rotação da GEMINI_API_KEY (pendência de segurança anterior,
   fora deste delta).
4. Backlog nomeado: grafo release-TEST do touring-server (pré-existente).
5. Observar: `arm_native`/`arm_both`/`pillar_induction_ratio` saem de STUB
   quando ARM_MIN_SAMPLE=20 acumular — a contagem está viva (daemon armado).
