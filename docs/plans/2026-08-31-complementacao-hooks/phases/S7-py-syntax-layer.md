---
type: PhaseReport
title: "S7-py-syntax-layer — phase report"
description: "PySyntaxSignalLayer (hooks-shared/qa_syntax.rs): parse tree-sitter do conteudo proposto de todo Write .py antes de gravar; Edit (fragmento) "
plan_id: 2026-08-31-complementacao-hooks
okf_version: 0.1
tags: [loop, phase, S7-py-syntax-layer]
timestamp: 2026-09-01T22:15:26.917756-03:00
---

# S7-py-syntax-layer — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

PySyntaxSignalLayer (hooks-shared/qa_syntax.rs): parse tree-sitter do conteudo proposto de todo Write .py antes de gravar; Edit (fragmento) e pre_read pulados com razao declarada (E4). Score 0.95, mensagem ensina a correcao (A5). Registrado no pipeline de pre_write; qa_syntax re-exportado em crate::shared (hook-handlers/lib.rs). Testes: 3 no layer + handler test_pre_write_flags_python_syntax_error_in_proposed_file.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/S7-py-syntax-layer.json](/knowledge/S7-py-syntax-layer.json).
