---
type: Reference
title: Índice — documentos movidos para docs/indefinidos/
description: Proveniência dos 14 arquivos movidos de docs/ para esta pasta em 26/08/2026 (F6b, decisão de Gabriel) — não classificáveis como sistema/repo/infraestrutura vivos.
plan_id: 2026-08-26-documentacao-touring
tags: [loop, indefinidos, archive]
timestamp: 2026-08-26T09:00:00-03:00
okf_version: "0.1"
---

# `docs/indefinidos/` — o que é esta pasta

Documentos que viviam em `docs/` sem se encaixar em nenhuma categoria clara
(sistema, repositório/infraestrutura, plano operacional datado, relatório de
trabalho) — a maioria são brainstorms, propostas e análises extensas de meses
atrás, sem link ativo de nenhum documento canônico (`CLAUDE.md`, `README.md`).

Movidos com `git mv` (histórico git preservado) em 26/08/2026, por decisão
explícita de Gabriel durante o loop de documentação
(`docs/plans/2026-08-26-documentacao-touring/`). Não apagados — a política de
20/08 (`strategy:doc-rewriting`) permanece: preservar, não destruir.

## Inventário e proveniência

| arquivo | idade quando movido | tamanho | por que não é "sistema vivo" |
|---|---|---|---|
| `touring-pro.md` | 153 d | 120 KB | brainstorm extenso, sem link ativo |
| `audit-touring-hooks-2026-03-28.md` | 150 d | 33 KB | auditoria pontual datada, superada por `docs/audits/` |
| `touring-pro-implementation-strategies.md` | 128 d | 13 KB | estratégia derivada de `touring-pro.md` |
| `Crates-para-Melhorar-Codigo-Touring.md` | 128 d | 68 KB | brainstorm de melhorias, sem link ativo |
| `Crates-Para-Melhorar-Geracao-de-Codigo.md` | 128 d | 54 KB | idem |
| `01-suggestions.md` | 127 d | 146 KB | lista de sugestões antiga, sem link ativo |
| `github-repos.md` | 124 d | 0 KB | arquivo vazio |
| `mutation-testing.md` | 123 d | 13 KB | nota técnica pontual; citada só por 1 relato histórico |
| `code-diagnostic.md` | 119 d | 18 KB | nota técnica pontual; citada só por 1 relato histórico |
| `Manual-Avancado-de Analise-de-Codigo.md` | 119 d | 115 KB | manual extenso, sem link ativo |
| `touring-tantivy-rebuild.md` | 110 d | 2 KB | nota pontual; citada só por 1 relato histórico |
| `touring-system.md` | 107 d | 52 KB | descrição de sistema desatualizada (33 menções a crates fantasma — ver F1) |
| `repo-health.md` | 55 d | 4 KB | snapshot pontual, sem link ativo |
| `CHANGELOG-releases.md` | 33 d | 1 KB | changelog manual não mantido (ver `docs/reference/` gerado) |

## Referências que ficaram apontando para cá

O gate de link (`loop_doc_link_gate.py`) detecta as citações abaixo. **Não
foram corrigidas** porque vivem em relatos históricos — a política é não
editar `docs/internal/sessions/` (registro do que aconteceu na época) nem
`client/` (espelho gerado, regra 6 do CLAUDE.md):

- `docs/internal/sessions/2026-04-24-waves-Q-R-M-A-T-P-plan.md` → cita `mutation-testing.md`, `repo-health.md`
- `docs/internal/sessions/2026-04-29-touring-excellence-pipeline-master-plan.md` → cita `code-diagnostic.md`, `Manual-Avancado-de Analise-de-Codigo.md`
- `docs/internal/sessions/2026-05-07-context-mode-integration-plan.md` → cita `touring-tantivy-rebuild.md`
- `client/skills/Touring/references/touring-cli-tiers-4-9.md` → cita `mutation-testing.md` (espelho; correção não se aplica aqui — ver regra 6)

## Excluídos deste movimento (verificados, permanecem em `docs/`)

Três arquivos apareceram na varredura inicial por não casarem nenhuma
palavra-chave de categoria, mas são documentação **viva**, confirmada por
citação ativa:

- `memory-hashtag-library.md` — citado 2× por `CLAUDE.md` como guia canônico da biblioteca de hashtags.
- `code-mode.md` — manual de usuário do Code Mode (Wave 2026-08-23), interlinkado com o arquivo abaixo.
- `code-mode-execution-layers.md` — arquitetura das camadas E0–E5 (Wave 2026-08-24), referenciado por `code-mode.md`.
