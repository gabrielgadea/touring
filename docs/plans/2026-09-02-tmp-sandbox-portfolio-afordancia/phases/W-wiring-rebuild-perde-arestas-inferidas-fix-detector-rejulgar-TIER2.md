---
type: PhaseReport
title: "W-wiring-rebuild-perde-arestas-inferidas-fix-detector-rejulgar-TIER2 — phase report"
description: "W fechado: 4 furos do detector de orfaos corrigidos sob TDD — (1) query tree-sitter sem chamada livre f() (rust_method_calls.scm +free_fn); "
plan_id: 2026-09-02-tmp-sandbox-portfolio-afordancia
okf_version: 0.1
tags: [loop, phase, W-wiring-rebuild-perde-arestas-inferidas-fix-detector-rejulgar-TIER2]
timestamp: 2026-09-02T07:06:46.467316-03:00
---

# W-wiring-rebuild-perde-arestas-inferidas-fix-detector-rejulgar-TIER2 — phase report

**Status**: done

Part of the [bundle](/index.md) · [log](/log.md).

## Summary

W fechado: 4 furos do detector de orfaos corrigidos sob TDD — (1) query tree-sitter sem chamada livre f() (rust_method_calls.scm +free_fn); (2) super::/self:: resolvidos a partir da raiz do crate em vez do modulo pai (resolve_scope_relative; scope_keyword 4519→808); (3) universo legado ./crates/ nunca fundido (canonicalize strip ./ + migrate_canonicalize_paths funde wiring_map/wiring_unresolved; rows 205k→90k); (4) caminho do hook (update_wiring_after_edit) limpava arestas inferidas sem recria-las — record_inferred_consumers unico para rebuild e hook (C08). B3 junto: XSS on<event>= so dentro de tag aberta. Deploy 30.4.31 (update-touring --no-kill + --no-build), rebuild: orfaos no escopo 5535→1754; residual 111 vs baseline TIER-2 = 42 consumidores em outro arquivo (arquivos editados apos o rebuild, antes do fix 4 — cai no proximo rebuild), 23 pub so locais, 22 sem consumidor, 24 genericos. Rejulgamento TIER-2 pendente do proximo deploy+rebuild (fix 4) e da decisao sobre a divida real.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/W-wiring-rebuild-perde-arestas-inferidas-fix-detector-rejulgar-TIER2.json](/knowledge/W-wiring-rebuild-perde-arestas-inferidas-fix-detector-rejulgar-TIER2.json).
