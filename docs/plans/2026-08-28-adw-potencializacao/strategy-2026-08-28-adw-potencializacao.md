---
type: Strategy
title: "ADW — análise exaustiva e potencialização máxima"
description: "Diagnóstico completo da estrutura ADW (runner adw.py 3.901L, factory, library, fragments, governança) + estratégia de potencialização em 6 fases"
tags: [adw, runner, factory, racing, sandbox, kpi, loop-engineering]
timestamp: 2026-08-28T08:52:00-03:00
plan: /plan.md
---

# Estratégia — ADW potencializado ao máximo

> OUTER: strategy-loop run `strategy-loop-1787917484-a718151d.1471095` (recall → diagnose →
> explore-until-dry **converged: true**, 37 findings, 7 rodadas 30→2→1→4→0→0, lente externa
> Context7 LangGraph visitada). Diagnostic: `diagnostics/touring-20260828T084516.md`.

## 1. O que a estrutura É (mapa verificado por execução)

| Camada | Realidade medida |
|---|---|
| **Runner** | `~/.claude/skills/Touring/scripts/adw.py` — 3.901 L, 182 defs; 8 tipos de nó (`code · agent · gate · loop · human · parallel · probe · control`); o CLI Rust `touring adw` (command_table.rs:196) delega |
| **Contratos fail-closed** | `NEW_FINDINGS= · METRIC= · COVERAGE= · FACT= · CONTROL=pass · VERDICT=` — `DEFAULT_VERDICT=REJECT`; narrativa de sucesso detectada (`SUCCESS_NARRATIVE`) vira Class-D quando diverge do veredito |
| **Lints** | 20+ (`cycles` — corrigido 18/08, varre TODOS os pontos de partida; `dead_node`, `fake_waiting`, `readonly_claim`, `critique_without_brief`, `gate_feedback`, `persona`, `budget`, `dynamic_parallel` com lens fixa rejeitada, `sweep_declares_floor`, `unsandboxed_writer_wants_human`…) |
| **Durabilidade** | journal fsync + `--resume-run` (prefixo), lock por run, run-ids session-keyed |
| **Governança** | `promotions.json` — cobertura **12/12** da library com evidência comportamental; `tiers.toml` (sota=opus · workhorse=sonnet · light=haiku) |
| **Fator de fluxo** | `factory.py` route/start — auto-instancia da library (`from-template`) quando o spec falta, reward por outcome |
| **Retry** | nós `code`: `retries` + backoff exponencial (teto 10s), timeout POR tentativa (= TimeoutPolicy LangGraph); gates: retry-com-feedback verbatim (W4 S-4.6, resolvido); agentes: `session="resume_on_fail"` |
| **ZTE** | conformal fail-closed (KnowNo), warmup ≥3 runs, journal `zte_bypass` com audit a-posteriori — **uso real: 0** |
| **KPIs** | `touring.adw.runs=45 PASS · explore_rounds_to_dry=7.7 PASS · zte_bypass_rate=0.0 PASS · plan_refine_iters=STUB · router_accuracy=STUB` |

## 2. Achados (evidência por execução, conf. anotada)

| # | Achado | Evidência | Sev |
|---|---|---|---|
| A1 | **Racing copia `target/` e `.git` por lane** — `RACE_IGNORE=("*.pyc","__pycache__",".touring",".touring-explore",".touring-plan")` não exclui `target` (dezenas de GB), `.git`, `.claude` (symbols.db 348MB), `node_modules`; `_copy_lane` usa `shutil.copytree(root,…)`; o merge é por bytes (não usa git) | adw.py:3698-3706 [1.0] | **P0** (viola REGRA #12; `adw race` no touring é inviável) |
| A2 | **`SANDBOX_MAX_TIMEOUT_MS=120_000` stale** — o teto real do `touring run` subiu para **600_000** hoje (QW-3); nó com `timeout_ms>120s` + `sandbox=true` é clampado com nota que afirma um teto FALSO (anti-padrão D8) | adw.py:96 vs ctx_execute_tools clamp [1.0] | **P0** |
| A3 | **2 KPIs STUB nunca medem**: `plan_refine_iters`, `router_accuracy` — anunciados no kpi, valor None | `touring kpi -j` [1.0] | P1 |
| A4 | **ZTE: infra pronta, uso 0** (`zte_bypass_rate=0.0`, 45 runs) — infra-desligada-não-é-infra-pronta | kpi [1.0] | P1 |
| A5 | **Sandbox adoption 24%** (16/66 nós code com `sandbox=true`) — falta a afordância: `node_readonly()` existe mas nenhum lint sugere sandbox p/ nó read-only | grep specs [1.0] | P1 |
| A6 | **Agent node sem retry de TRANSPORTE** — exit≠0/timeout 124 do `claude -p` não re-tenta (code tem `retries`; veredito é do gate, mas transporte é infra) | adw.py:2069-2102 [1.0] | P1 |
| A7 | **5 specs locais fora da library** (`error-teach, herdr-fanout-demo, omarchy-audit-2, xaudit-gates, xaudit-lintscan`) — sem curadoria: promover ou etiquetar experimental | ls diff [1.0] | P2 |
| A8 | **Memória stale**: `adw-lint-cycles-return-prematuro` reporta bug já corrigido em 18/08 | docstring _lint_cycles [1.0] | P2 |
| A9 | Lente externa (LangGraph): replay de checkpoint ARBITRÁRIO (`--from-node`) não existe (resume = prefixo); interrupt/human ✓, caching ✓, retry-policy ✓ (code) | Context7 [0.9] | P3/backlog |

## 3. Estratégia — 6 fases (INNER)

| Fase | Entrega | Gate |
|---|---|---|
| **F1 P0s cirúrgicos** | `RACE_IGNORE` += `target`, `.git`, `.claude`, `node_modules`, `*.db` + teste de não-cópia; `SANDBOX_MAX_TIMEOUT_MS=600_000` + nota verdadeira; memória A8 corrigida | test_adw verde + prova: race dry em repo-fixture |
| **F2 KPIs honestos** | `router_accuracy` real (dados do factory reward já gravados) e `plan_refine_iters` real OU rebaixados a advisory com razão declarada; decisão por dado | `touring kpi -j` sem STUB mudo |
| **F3 Afordância sandbox** | lint novo `readonly_sem_sandbox` (warning + remédio derivado, dual do `unsandboxed_writer_wants_human`); aplicar `sandbox=true` nos nós readonly da library; re-lint 100% | adoção medida ≥60% dos elegíveis |
| **F4 Retry de transporte p/ agente** | `retries` no agent node SÓ para exit≠0/timeout (veredito continua no gate); custo contado no journal | teste + 1 run real |
| **F5 Curadoria da library** | triagem dos 5 specs locais (promover generalizáveis c/ `adw promote`, etiquetar experimentais); `adw lint` 100% da library implantável | promotions.json atualizado |
| **F6 Exercitar ZTE + docs + close** | 1 exercício real de ZTE em spec low-risk (ou decisão documentada de mantê-lo reserva); docs sincronizadas; memory store; convergência | `loop_converged.py` exit 0 |

**Backlog consciente (não nesta wave)**: A9 replay from-node; tier `frontier` (exige provar `--model` novo no CLI headless antes).

## 4. O que NÃO muda

Contratos fail-closed, Class-D, promotions comportamentais, session-keyed runs, o desenho
on-demand do from-template (a "divergência" library↔implantado é design, não bug — factory
auto-instancia).
