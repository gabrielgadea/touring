---
type: Certification
title: "Certificação das fontes externas da rodada 4 — qual notebook é o do Gabriel"
description: "Prova de identidade dos notebooks NotebookLM usados como base externa, e a armadilha do título duplicado."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# Certificação das fontes — rodada 4

> Gabriel pediu (26/10/2026): *"certifique-se de que o notebook do claude code é o
> que eu indiquei"*. A verificação encontrou **uma armadilha real**: existem
> **dois** notebooks "Claude Code" na conta, e o match por título escolhe o
> **errado**.

## 1. A armadilha

`mcp__notebooklm-mcp__notebook_list` (171 notebooks) devolve:

| notebook_id | título (literal) | fontes | `created_at` | `modified_at` |
|---|---|---:|---|---|
| `91efd704-8540-4a1c-950b-5e233b3e1ab3` | `Claude Code` | 42 | **2025-06-27T14:58:49Z** | 2026-10-26 |
| `65c3ecb1-a4ce-4901-9864-a2b28c04b469` | `Claude Code ` **(ESPAÇO FINAL)** | **91** | **2026-09-25T22:44:15Z** | 2026-10-26 |

**O indicado por Gabriel** — *"Claude Code" de **25 de setembro de 2026*** — é
`65c3ecb1-a4ce-4901-9864-a2b28c04b469`:

- `created_at = 2026-09-25T22:44:15Z` ⇒ **criado em 25/09/2026** ✓
- **91 fontes** (o outro tem 42) ✓
- As fontes citam material de **setembro a outubro de 2026**: `fast-jev-compaction`
  criado em 17/09/2026 e com 7.335 stars em 02/10/2026; Claude Code **2.1.287**
  lançado em 01/10/2026; tabela de adaptadores com `opencode` verificado em
  **2026-10-12** (a data mais recente citada).

**A diferença de título é um espaço em ASCII.** Qualquer match por `== "Claude Code"`
ou por `startswith` erra; e `batch` com `notebook_names: "Claude Code"` resolve
para o **id errado** (42 fontes) — foi o que aconteceu na primeira consulta.

## 2. Prova de identidade (consulta direta ao notebook correto)

Query de certificação ao `65c3ecb1-…b469`, respondida com `sources_used` e
`citations` resolvíveis:

- **91 fontes** declaradas.
- Primeiras 12 fontes, todas do domínio Claude Code/agentic-OS:
  1. *"Software Fundamentals Matter More Than Ever"* — Matt Pocock
  2. *"3 Claude Code mods you need to build"*
  3. *"9 NEW Claude Mods that can truly change how you work"*
  4. *"A NEW Agentic OS Standard for Claude 5.5 is here"*
  5. *"APRENDA A AGIR COMO SE FOSSE O MELHOR…"* — Flávio Augusto
  6. *"Agentic Engineering Operating Level: WHERE to FOCUS your AGENTS?"*
  7. *"Anthropic Engineers Just 10x'd Everyone's Claude Code"*
  8. *"Anthropic Just Donated Claude Skills. Here's **What to Delete**."*
  9. *"Anthropic Just Released 13 New Skills To Level Up Your Claude Setup"*
  10. *"Anthropic Just Revealed 10 NEW Rules for Claude Skills"*
  11. *"Anthropic Just Revealed 12 New Rules for Prompting Opus 5.5"*
  12. *"Anthropic Revealed Their Secret Guide to Mastering Opus 5.5"*

O teto de citação **[91]** (e não [42]) é o marcador de que o notebook certo
respondeu.

## 3. Por que isto importa para a análise

O notebook errado (42 fontes, de 2025-06-27) é um acervo de **terceiros sobre
Claude Code** — e o acervo que Gabriel indicou (91 fontes, 25/09/2026) é
**material primário da Anthropic e deOperators de agentic-OS**, incluindo:

- as **regras oficiais** para Claude Skills e para promptar Opus 5.5;
- um guia oficial de **o que deletar** de um skill;
- o `fast-jev-compaction` — um plugin de compactação que **corta em vez de
  resumir**, deixando texto intacto e removendo apenas *tool calls e saídas que
  já cumpriram o papel*;
- a crate `pond-db`, com uma **tabela de adaptadores por agente e data de última
  verificação** (`claude-code` 2026-08-14, `codex-cli` 2026-09-01,
  `opencode` 2026-10-12, `devin` 2026-09-26) — disciplina de *staleness*.

São exatamente as quatro coisas que a análise do Touring precisa: **critério de
qualidade de conteúdo injetado**, **política de remoção (não de acúmulo)**,
**estratégia de evasão de contexto** e **disciplina de obsolescência**.

## 4. Regra que este incidente fixa

> **Nunca resolva um notebook por título.** Resolva por **id**, e **certifique o id
> contra um sinal independente** (data de criação, nº de fontes, e o teto das
> citações `[n]` na resposta). Um título pode ter **espaço final** e um
> homônimo antigo pode sequestrar a busca.

O scout que consultou o id errado foi **corrigido e instruído a descartar** o que
extraiu do `91efd704`, refazendo tudo contra `65c3ecb1-…b469`.

---

_Certificação por TACO, 26/10/2026, a pedido explícito de Gabriel Gadea.
Fonte primária: `mcp__notebooklm-mcp__notebook_list` + `notebook_query` no id
`65c3ecb1-a4ce-4901-9864-a2b28c04b469`._
