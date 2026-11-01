---
plan_id: 2026-08-28-skills-adw-potencializacao
type: Strategy
title: "Curadoria 28/08 aplicada — os vereditos dos ADWs viram mudança"
description: "Aplicação aprovada por Gabriel dos 17 itens em curadoria: etiquetas na library + guard, 2 fixes do cross-audit, exercícios de ócio, calibrações A5/E4, fix do minerador REFINE"
tags: [adw, curadoria, code-mode, skills]
timestamp: 2026-08-28T13:58:00-03:00
plan: /index.md
---

# Curadoria aplicada (28/08/2026, tarde)

> Aprovação de Gabriel, item a item: (1) adw-curation ✔; (2) cross-audit scripts/ ✔;
> (3) exercise-idle-infra: safe + coverage_line + plan_refine_iters; mutation
> registrado; (4) code-mode-adherence ✔; (5) skill-refine ✔.

## O que mudou

| Item | Aplicação | Prova |
|---|---|---|
| Etiquetas adw-curation | `curation = generic\|local\|hold` no `[purpose]` dos 23 flows (13 vereditos do flow + critério estendido) | guard novo `test_every_shipped_flow_carries_a_curation_label`; 248 testes verdes |
| cross-audit desvio 1 | `generate_context_mode_plan.py` OUTPUT_PATH: árvore congelada → `~/projects/touring/docs` | comentário-lição no sítio |
| cross-audit desvio 2 | **falso-stale do auditor**: `~/.claude/tools/{disk-watch,safe-clean}.sh` JÁ eram symlinks → repo desde 25/08 (o auditor comparou conteúdo, não `ls -la`) | sha + `ls -la` verificados |
| Exercícios safe | test_count: **15.786 testes** (nextest instalado); 2× code_mode_arm rodados; daemon reiniciado ARMADO (pillar induction) | comandos executados, exit 0 |
| coverage_line / plan_refine | `cargo llvm-cov --workspace` + `explore-plan` (ledger seco) em execução | fontes acumulam; KPIs saem de STUB com amostras |
| mutation_kill_rate | adiado por aprovação — backlog `backlog-mutation-kill-rate-estreia` | memória #status:backlog |
| Calibração A5 (exception 71,6%) | `ctx_execute_tools.rs`: exit≠0 com stderr vazio agora ensina o caso grep/test "no match" | cargo check + suíte touring-server verdes |
| Calibração E4 (timeout 26,8%) | banner nomeia `--timeout-ms` (default 30000) | guard D8 cruzado 13/13 |
| Fix minerador REFINE | `mine_transcripts.py` desconta eco do SKILL.md (`is_skill_echo`, wired no `mine()`) | 4 testes novos verdes |

## Nota honesta sobre os KPIs

Os exercícios provaram os COMANDOS; os KPIs `code_mode_arm_*` e
`pillar_induction_ratio` saem de STUB quando as fontes acumularem amostras
(ARM_MIN_SAMPLE=20) — a contagem começou agora. `test.count`/`coverage.line`
dependem de o derivador ler as fontes recém-criadas.
