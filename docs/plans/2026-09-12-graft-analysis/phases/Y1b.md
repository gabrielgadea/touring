---
type: PhaseReport
title: "Y1b — phase report"
description: "Conteúdo buscável: markdown vira texto indexado (documento com descrição + corpo até 20k, blocos de 600), doc comments de código, rebuild/ho"
plan_id: 2026-09-12-graft-analysis
okf_version: 0.1
tags: [loop, phase, Y1b]
timestamp: 2026-09-13T19:40:53.086212-03:00
---

# Y1b — phase report

**Status**: done

Part of the [bundle](/index.md) · [plan](/plan.md) · [log](/log.md).

## Summary

Conteúdo buscável: markdown vira texto indexado (documento com descrição + corpo até 20k, blocos de 600), doc comments de código, rebuild/hooks/reindex alimentam o tantivy pelo mesmo construtor (tantivy_docs::docs_for_file), analisador com acentos e stopwords pt/en, frase por analisador, cobertura constante (5 + 1,25 por subconjunto), agregação por arquivo λ 0,25, compactação após escrita. Medido por examples/search_eval.rs sobre 75 perguntas: conteúdo 0/11 -> 11/11, code-train 9/10 estrito (10/10 corrigido), total 64/75 ao vivo = offline. Defeitos corrigidos: cache de consulta nunca invalidado, frase contra stemmer, top dobrado, ordem dependente do limite, compactação silenciosa, reindex apagando doc comments, cache de consulta sem escopo entre projetos.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/Y1b.json](/knowledge/Y1b.json).
