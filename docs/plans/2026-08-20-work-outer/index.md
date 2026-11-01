---
okf_version: "1.0"
type: LoopBundle
title: "Touring Documentation Rewriting — Wave 1"
description: "Bundle for the documentation premiumization initiative. W1 reescreve os arquivos de entrada do repo (README, CLAUDE, ARCHITECTURE) com padrao MkDocs + frontmatter OKF, sem destruir historico."
plan_id: 2026-08-20-work-outer
tags: [documentation, wave-1, README, CLAUDE, ARCHITECTURE, okf, mkdocs]
timestamp: 2026-08-20T11:20:00-03:00
---

# Touring Documentation Rewriting — Wave 1

Bundle for the documentation premiumization initiative, opened 2026-08-20.
Wave 1 addresses the **root-level entry documents** (README, CLAUDE, ARCHITECTURE).
Subsequent waves (W2-W10) will follow the strategy in [strategy-2026-08-20-doc-rewriting.md](./strategy-2026-08-20-doc-rewriting.md).

## Fases (W1)

| # | fase | status |
|---|---|---|
| W1.S1 | Snapshot pre-W1 dos 5 arquivos (SHA256 + mtime) | ✓ done |
| W1.S2 | `ARCHITECTURE.md` consolidado; `ARCHITECTURE.v29.5.0.md` → `ARCHITECTURE-historical-v29.5.0.md` | ✓ done |
| W1.S3 | `ARCHITECTURE_PLAN.md` (2026-03-30, superseded pela Wave H) arquivado | ✓ done |
| W1.S4 | `README.md` com frontmatter OKF + versão 30.4.13 | ✓ done |
| W1.S5 | `CLAUDE.md` com frontmatter OKF | ✓ done |
| W1.S6 | Validação por `loop_doc_link_gate.py` | ✓ done |
| W1.S7 | Guarda semantica: SHA256 contra baseline | ✓ done |
| W1.S8 | Este index.md | ✓ done |

## Veredito

- **Zero regressões**: o `loop_doc_link_gate.py` reporta os mesmos **5 links quebrados** do baseline pre-W1 — todos `-> /index.md` no bundle, fechados por este arquivo.
- **Preservação**: 5 arquivos originais preservados em [archive/2026-08-20-doc-rewriting/W1-orig/](archive/2026-08-20-doc-rewriting/W1-orig/) com SHA256 idêntico ao baseline.
- **Prosa preservada**: README e CLAUDE mantiveram o conteúdo escrito; só ganharam frontmatter OKF e bump de versão.

## Proveniência

- [provenance/W1-before.json](./provenance/W1-before.json) — hashes, sizes, mtimes dos 5 originais antes da W1.
- [archive/2026-08-20-doc-rewriting/W1-orig/](archive/2026-08-20-doc-rewriting/W1-orig/) — backup integral.

## Histórico de fases

- [diagnostics/touring-20260820T020259.md](./diagnostics/touring-20260820T020259.md) — diagnostic inicial do estado documental
- [diagnostics/touring-20260820T104945.md](./diagnostics/touring-20260820T104945.md) — diagnostic após tentativa de explore
- [diagnostics/touring-20260820T105003.md](./diagnostics/touring-20260820T105003.md) — diagnostic com topic do tema
- [diagnostics/touring-20260820T105429.md](./diagnostics/touring-20260820T105429.md) — diagnostic final do baseline

## Próximas ondas (W2-W10)

Ver [strategy-2026-08-20-doc-rewriting.md](./strategy-2026-08-20-doc-rewriting.md) para o plano completo de 10 ondas.
