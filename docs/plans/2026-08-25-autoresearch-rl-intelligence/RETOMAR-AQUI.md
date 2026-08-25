---
type: ResumePointer
title: RETOMAR AQUI — Touring Autoresearch (RL/inteligência que melhora a cada execução)
description: Estado congelado do plano em 2026-08-25 — OUTER completo, aguardando decisão humana na estratégia.
plan_id: 2026-08-25-autoresearch-rl-intelligence
tags: [loop, resume, autoresearch, rl]
timestamp: 2026-08-25T22:00:00-03:00
okf_version: "0.1"
---

# RETOMAR AQUI — 2026-08-25 (sessão encerrada pelo Gabriel)

## Estado: OUTER completo ✅ · AGUARDANDO decisão humana na estratégia (HUMAN GATE, step 9)

### O que já está feito (não refazer)
- `strategy-loop` rodado (diagnose OK; `explore_round` exit 3 → completado manualmente nesta sessão)
- Ledger CCE **convergido** (2 dry rounds, lente `external` marcada com 11 fontes):
  `.touring-explore/autoresearch--camadas-de-inteligencia-e-rl-em-lo.ledger.json`
- Diagnóstico OKF: `diagnostics/touring-20260825T181916.md`
- **Estratégia completa: `strategy-2026-08-25-autoresearch.md` (9,1 KB) — LER PRIMEIRO**
- Memória semantic: `strategy:autoresearch-rl-intelligence:2026-08-25`
- Manifesto do flow `strategy-outer`: `complete: true` (loop_outer_gate.py)

### Ground truth medido nesta sessão (citar, não re-derivar)
- `corpus_coverage=0.861` · `never_recalled=0.199` — a citação que motivou o plano tinha **sinal invertido** (media a fração recuperada com o nome da perdida)
- **`outcome_reward` em 1,7% das memórias (147/8.641; global 4/241) ← o poço real do aprendizado**
- LinUCB em 95 arquivos prod · QTable em 70 · reward callsites em 19 · **`dspy_cluster` = 0 arquivos em `crates/`**
- `adw.py`: **1 callsite de reward (só `campaign`)** — `adw run` comum não aprende; `touring.adw.runs=35`; `router_accuracy` STUB
- Code mode: `adoption_ratio=0.083`; counters `t3_turn_*` sem consumidor de aprendizado
- `learning status`: LinUCB `update_count=7`, agentic_rl `=5`, `ema_reward=0.40`

### Decisão pendente (Gabriel) — a estratégia propõe 3 sistemas (A sinal / B políticas / C research loop Karpathy) em 5 fases P0–P4
1. Aprova como está → decompor DAG P0–P4 e iniciar INNER no P0
2. Aprova com ajuste de escopo → registrar o ajuste e seguir
3. Só o P0 → reward por `adw run` + retro-escrita de `outcome_reward`, reavaliar com delta medido
4. Cancela/reformula → estratégia fica arquivada

### Próximo passo ao retomar
1. Ler `strategy-2026-08-25-autoresearch.md` (seções 3–4: sistemas e fases)
2. Com a decisão: `touring decompose create plan "autoresearch-rl-intelligence"` + subtasks P0–P4 com dependências
3. INNER P0 (Sistema A): reward por `adw run` em `~/.claude/skills/Touring/scripts/adw.py` (espelho `client/skills/`) + retro-escrita de `outcome_reward` nos caminhos de gate — critério de convergência: coverage medido por probe SQL sobe de 1,7% (query: `SELECT COUNT(*) FROM memory_entries WHERE outcome_reward IS NOT NULL`)
4. Convergência do plano: `loop_converged.py --task <id> --scope . --rust-full` exit 0

### Baselines KPI (para medir o delta do plano)
- `touring.memory.corpus_coverage=0.861` · `never_recalled_ratio=0.199` · `curated_recall_share=0.175`
- `touring.code_mode.adoption_ratio=0.083` · `touring.flow.compliance_ratio=0.514`
- `touring.adw.runs=35` · `explore_rounds_to_dry=7.13` · orphans=2358 (ADVISORY, medidor não confiável)

### Anomalias da sessão 2026-08-25 (registradas, não investigadas)
- Emissão de tool-calls corrompida 11× nesta sessão (chamadas destinadas ao Context7 MCP e ao AskUserQuestion saíram como outras ferramentas — playwright/render/exa/MiniMax). Context7 coberto via endpoint HTTP `context7.com/stanfordnlp/dspy/llms.txt` (34 KB, índice de otimizadores GEPA/MIPROv2/Bootstrap). **Se persistir na próxima sessão, reportar ao Gabriel.**
- `touring adw journal` não existe (subcomandos: list/lint/run/test/from-template/explain/promote/fragments/new/campaign/race)
