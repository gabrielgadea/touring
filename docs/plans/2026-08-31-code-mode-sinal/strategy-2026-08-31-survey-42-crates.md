---
type: Strategy
title: "Strategy v2.0 — code-mode-sinal-canal (com validação F3.0)"
description: "Estratégia final pós-survey exaustivo (F1.5/F1.6/F1.7/F1.8/F1.9/F2.0/F3.0). Validação prática via 5 cenários reais → 17 MUST + 23 SHOULD + 17 NICE + 2 gaps. Recomendação: F2 começa focado nas MUST-have, expanda progressivamente."
plan_id: 2026-08-31-code-mode-sinal
bundle: docs/plans/2026-08-31-code-mode-sinal
okf_version: "0.1"
version: "2.0"
based_on: ["strategy-2026-08-31-yetzirah-v1.1.md", "inventory-hooks.md v3.0"]
tags: [strategy, strategy-loop, code-mode, 5-cenarios, must-have, gaps, f2-foco]
timestamp: "2026-08-31T15:34:00-03:00"
---

# Strategy v2.0 — code-mode-sinal-canal (pós-validação F3.0)

## TL;DR

Exploração COMPLETA (F1.5/F1.6/F1.7/F1.8/F1.9/F2.0/F3.0) cobriu:
- **Cadeia 7-camadas** de injeção (F1.5)
- **5 bridges** analisados em profundidade (F1.6)
- **42 crates** catalogados (F1.7+F1.8+F1.9)
- **`foundation::code_mode` investigation** — REUSAR, não duplicar (F2.0)
- **5 cenários REAIS** validaram as 57 SDK propostas (F3.0)

**Resultado final**: **17 MUST + 23 SHOULD + 17 NICE + 2 gaps** identificadas.

**Recomendação**: F2 Opção A (`touring-code/src/sdk/`) com **17 MUST-have** tipadas; expansão progressiva.

## Decisão arquitetural (gate humano revisado)

| Opção | Status | Recomendação |
|---|---|---|
| A. `crates/touring-code/src/sdk/` | ✅ | **Recomendada** — reusa deps, +650 LOC = 2% |
| B. Novo crate `touring-code-mode-sdk/` | ❌ | +1 dos42 crates, mais deps |

**Recomendação**: A. Reusa `touring-code` (lar do code intelligence) + `touring-foundation::code_mode` + `touring-cli::cli_suggester` + `touring-server::apply_curation` + `touring-daemon`.

## F2 — Foco MUST-have (17 funções)

| Rank | Função | Latência | Cenários |
|---|---|---|---|
| 1 | `touring.quality` | ~30ms | A, C, D |
| 2 | `touring.dependents` | < 5ms | B, C, D |
| 3 | `touring.edit_impact` | ~15ms | A, C |
| 4 | `touring.pub_api_diff` | ~10ms | C, D |
| 5 | `touring.file_metadata` | ~10ms | D, E |
| 6 | `touring.code_mode_status` 🆕 | < 5ms | B, E |
| 7 | `touring.scan_vulnerabilities` | ~40ms | B, D |
| 8-17 | (10 funções) | < 30ms cada | (1 cenário cada, ESSENCIAL) |

## F3 — Expansão progressiva (SHOULD + NICE)

**Após** MUST-have validado em produção:
- **F3.1** (1 semana): SHOULD-have tier 1 (PSI, hybrid_search, embed)
- **F3.2** (2 semanas): SHOULD-have tier 2 (linucb, recommend, session_graph)
- **F3.3** (3 semanas): NICE-to-have (mcts, fuse_rrf, mcp_tools)

## 2 GAPS identificados (F4 — endereçar)

1. **`touring.api_change_risk(diff)`** — "is breaking?" detector (estender `pub_api_diff`)
2. **`touring.session_recall(session_id)`** — memo entre sessões (persistir em knowledge DB)

## Mapa de dependências FINAL

```
touring-code (lar do SDK; F2 escreve sdk.rs aqui)
    ├── touring-foundation (code_mode + error types + chunker)
    │   └── touring-hooks-core (bridges: F1.6)
    │       ├── touring-intelligence (BM25, RL, MCTS, GoT, ANN)
    │       ├── touring-storage (knowledge, hybrid_search, embeddings)
    │       └── touring-analysis (Wilson CI, unwrap audit)
    ├── touring-ceg (X0..X9 + capability model) 🆕
    ├── touring-quality (50-dim scoring)
    ├── touring-offensive (CWE taxonomy)
    ├── touring-generator (VGP, speculate, scaffold)
    ├── touring-lsp (diagnostics)
    ├── touring-hooks-prediction (TF-IDF, LLM judge)
    ├── touring-hooks-rl (PPO policy)
    ├── touring-cortex (RRF, call_graph, 84+ handlers)
    ├── touring-server (elite_tools, MCP tools)
    ├── touring-orchestration (Tasksfile, flow)
    ├── touring-bindings (PyO3, WASM, Web)
    ├── touring-simd (ANN SIMD)
    ├── touring-hook-runtime (HookRuntime + scip_ingest)
    ├── touring-resilience (PSI, failover)
    └── touring-identity (EntityId)
```

## Critérios de pronto (F6 — 6 critérios AND revisados)

1. (a) `python3 scripts/test_code_mode_signal_injection.py --verbose` exit 0 com N≥8 testes verdes
2. (b) `touring kpi -j code_mode_signal_use.composite ≥ 0.80` em produção ≥ 7 dias
3. (c) `python3 docs/elite_aggregate.py --check` ≥ Gold (escopo code-mode-sinal)
5. (d) release v30.5.x propagada + 1 ADW end-to-end usando canal
6. (e) `code_mode_signal_token_reduction_pct ≥ 20%` (Anthropic paralelo)
7. (f) `code_mode_signal_rounds_reduction_pct ≥ 50%` (sandbox + canal)

## Próximo passo (gate humano)

Gabriel, decisão final antes de F2:

1. **Aprovar F2 Opção A + MUST-have primeiro** (17 funções) ✅
2. **Investigar paralelos externos** (Anthropic CodeAct, Cloudflare Code Mode) — opcional
3. **Parar entrega** — exploração completa; F2 aguarda code work

Quando aprovado, F2 inicia com `crates/touring-code/src/sdk/types.rs` (~200 LOC) cobrindo as 17 MUST-have.

## Citações

- F3.0: `inventory-hooks.md` v3.0 (validação prática 5 cenários)
- F2.0: investigation foundation::code_mode (recomendação REUSAR)
- F1.9: 57 funções SDK propostas (base da priorização)
- Decisões Gabriel: `strategy-v1.1.md` (4 decisões aplicadas)
- DAG: `task_1788196388043002698` (F1 done, F2-F6 pending)

---

_v2.0 — 2026-08-31 15:34 BRT | Strategy final pós-F3.0 | 17 MUST + 23 SHOULD + 17 NICE + 2 gaps | F2 focado nas MUST-have_