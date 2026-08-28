---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-28-adw-potencializacao
tags: [loop, log]
timestamp: 2026-08-28T08:45:16.809582-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-28T08:55:52.874401-03:00 — F1: P0s cirúrgicos — RACE_IGNORE (target/.git/.claude/node_modules/*.db) + SANDBOX_MAX_TIMEOUT_MS=600000 + memória lint-cycles corrigida + testes done

F1 fechada: RACE_IGNORE agora exclui target/.git/.claude/node_modules/*.db (o race copiava N×~30GB por lane — REGRA #12); SANDBOX_MAX_TIMEOUT_MS 120000→600000 alinhado ao clamp real do QW-3 com guard D8 cruzado (teste lê o predicado do executor Rust e exige paridade); comentário stale do run_code_node corrigido; teste novo prova lane sem caches; memória adw-lint-cycles-return-prematuro atualizada para RESOLVIDO. Gates: 248/248 testes do runner (test_adw+test_factory+test_adw_gate_rejection).

## 2026-08-28T09:03:16.919023-03:00 — F2: KPIs honestos — router_accuracy e plan_refine_iters reais ou advisory declarado done

F2 fechada: os 2 STUBs eram medidores reais com fonte vazia — remédio foi EXERCITAR a infra: (1) factory start real (ticket chore com verify_cmd real; o gate recusou verify fake — L2 viva) → router_accuracy STUB→1.0 PASS vivo, runs 45→46; (2) plan_refine.py real sobre o strategy doc + ledger CCE → .refine.json no disco, que revelou BUG produtor≠consumidor: plan_refine grava {version,iterations}, o KPI só aceitava array cru — corrigido no kpi.rs aceitando ambas as formas + teste que espelha o formato do produtor (34/34). Vivo do plan_refine_iters vira PASS no próximo deploy (o dispatch roda no daemon instalado). ZTE fica para F6 como planejado.

## 2026-08-28T09:07:58.056393-03:00 — F3: afordância sandbox — lint readonly_sem_sandbox + aplicar sandbox=true na library + re-lint 100% done

F3 fechada: lint novo _lint_readonly_without_sandbox (dual do S-8.4) — leitor PROVADO (command_writes False ou readonly=true) fora do sandbox ganha warning com remédio derivado; calibrado para exigir leitor SUBSTANTIVO (_FS_READING_COMMANDS/multiplexers/programas — echo/true não têm o que conter, ruído com cara de rigor). O lint apontou 3 nós reais da library (critic-panel::quorum, graph-pack::relations, worker-critic-pair::judge) — sandbox=true aplicado nos 3; elegíveis 3/3 (100%). Prova viva: test_shipped_critic_panel executa o quorum de verdade, agora via touring run/CEG — 248/248 verdes. Espelho client/ sincronizado (guard library_and_repo_mirror_agree).

## 2026-08-28T09:09:26.562798-03:00 — F4: retry de transporte no agent node (exit!=0/timeout; veredito segue no gate) + teste done

F4 fechada: run_agent_node ganhou retries de TRANSPORTE (paridade com o nó code) — exit!=0/timeout 124 do claude -p re-tenta com backoff, cada tentativa auditável no journal (agent_transport_retry); veredito segue exclusivo do gate (feedback verbatim). Teto A14=3 imposto como ERRO de lint (custo LLM real). 2 testes novos: replay de infra-only (3ª tentativa passa, 2 registros no journal, sem-retries preserva comportamento) + lint barra retries=4. Suíte 230/230 do test_adw; espelho client/ sincronizado.

## 2026-08-28T09:10:52.345459-03:00 — F5: curadoria — triagem dos 5 specs locais (promover/etiquetar) + adw lint 100% da library done

F5 fechada: error-teach PROMOVIDO à library com evidência comportamental (10 runs completed; adw promote --run + cópia para adw-library — 13ª entrada em promotions.json); 4 specs locais etiquetados com razão (herdr-fanout-demo=demo, omarchy-audit-2=local-omarchy, xaudit-gates=local-touring/parametrizar antes, xaudit-lintscan=utilitário local). Lint 100% da library: 12/12 specs SEM erro (warnings restantes são advisories deliberados de outros lints). Espelho client/ sincronizado.

## 2026-08-28T09:12:06.316207-03:00 — F6: exercitar ZTE 1x (ou decisão documentada) + docs sincronizadas + memory + convergência done

F6 fechada: ZTE exercitado end-to-end ao vivo via zte-probe (3 warmups aprovados + 4ª run bypassa com conformal IN, confidence 1.0, journal audit a-posteriori; KPI zte_bypass_rate 0.0→0.02); CLAUDE.md item 8 atualizado com a potencialização; memória semantic persistida; probe etiquetado como local.
