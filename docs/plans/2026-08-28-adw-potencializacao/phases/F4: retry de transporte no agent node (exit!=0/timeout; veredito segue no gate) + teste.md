---
type: PhaseReport
title: F4: retry de transporte no agent node (exit!=0/timeout; veredito segue no gate) + teste — phase report
description: F4 fechada: run_agent_node ganhou retries de TRANSPORTE (paridade com o nó code) — exit!=0/timeout 124 do claude -p re-tenta com backoff, ca
plan_id: 2026-08-28-adw-potencializacao
tags: [loop, phase, F4: retry de transporte no agent node (exit!=0/timeout; veredito segue no gate) + teste]
timestamp: 2026-08-28T09:09:26.562798-03:00
okf_version: "0.1"
---

# F4: retry de transporte no agent node (exit!=0/timeout; veredito segue no gate) + teste — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

F4 fechada: run_agent_node ganhou retries de TRANSPORTE (paridade com o nó code) — exit!=0/timeout 124 do claude -p re-tenta com backoff, cada tentativa auditável no journal (agent_transport_retry); veredito segue exclusivo do gate (feedback verbatim). Teto A14=3 imposto como ERRO de lint (custo LLM real). 2 testes novos: replay de infra-only (3ª tentativa passa, 2 registros no journal, sem-retries preserva comportamento) + lint barra retries=4. Suíte 230/230 do test_adw; espelho client/ sincronizado.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/F4: retry de transporte no agent node (exit!=0/timeout; veredito segue no gate) + teste.json](/knowledge/F4: retry de transporte no agent node (exit!=0/timeout; veredito segue no gate) + teste.json).
