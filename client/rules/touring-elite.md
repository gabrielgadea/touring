---
name: touring-elite
description: Auto-load rule for the Touring Elite 17-Dimensional Harness. Activates on any code-intent (edit/refactor/audit/release) to surface the composite EliteScore and prevent regression below Diamond tier. Companion to the touring-elite skill (load on demand for full docs).
---

# Touring Elite — Auto-Load (constitutional, 2026-06-13)

**Gatilhos → ação:**

| Se disser | Rodar | Exigência |
|---|---|---|
| release · ship · merge · PR · publish | `python3 docs/elite_aggregate.py --check` | tier ≥ Gold (0,80) |
| audit · review · quality check | `python3 docs/elite_aggregate.py --json` | composite ≥ 0,95 (Diamond) |

**Abaixo de Gold (0,80) é BLOCK:** identificar o gate que falhou, remediar, só
então seguir. Não há caminho silencioso para release.

**Reflexos por arquivo > 500 LOC:** `touring ast tdg <file>` antes de refatorar.

> Corpo canônico: `~/.claude/skills/touring-elite/references/auto-load-rule.md`
> Catálogo das 17 dimensões e pesos: skill `touring-elite` (sob demanda).