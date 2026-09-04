---
type: Resume
title: "Retomar aqui — Touring próxima geração (harness modular com reinjeção medida)"
description: "Plano L4 aprovado e pausado por Gabriel em 02/09/2026 antes da W0; DAG registrada; retomar pela S-1 sob TDD."
plan_id: 2026-09-02-touring-proxima-geracao
okf_version: 0.1
tags: [resume, harness, modularizacao, reinjecao]
timestamp: 2026-09-02T07:55:00-03:00
---
# Retomar aqui — Touring próxima geração

Parte do [bundle](/index.md). Estado em 02/09/2026 07:55 BRT: **plano aprovado, execução pausada por decisão de Gabriel** ("Aprovar o plano, mas pausar aqui").

## O que está pronto
- [`criacao.md`](/criacao.md) — Briah, ratio 1,0 · [`strategy-2026-09-02-touring-proxima-geracao.md`](/strategy-2026-09-02-touring-proxima-geracao.md) — OUTER convergido · [`plan.md`](/plan.md) — Pln2 L4, 19 subtasks W0-W5, gap_detector 0 P0, plan_validator strict OK, score 7,33 (scalability/dependencies/potentiation em 6 pelo proxy de densidade; não amplificar mais).
- DAG `task_1788344667602060021` — `touring decompose ready task_1788344667602060021` devolve **S-1**; 18 pendentes.
- Marker do loop **arquivado** (pausa). Rearmar ao retomar: `python3 ~/.claude/skills/loop-engineering/scripts/hooks/loop_marker.py write --task task_1788344667602060021 --scope $PWD --bundle docs/plans/2026-09-02-touring-proxima-geracao`.

## Estado verificado no encerramento (02/09 ~15:40 BRT)

- **DAG persistida em disco** (o handler do daemon estava com timeout, então a prova é direta):
  `~/projects/touring/.claude/touring/knowledge.db` → `task_decompositions` 1 linha ·
  `decomposition_subtasks` **19 linhas** · `decomposition_events` 20 linhas, todas com
  `task_1788344667602060021`. Quando o daemon voltar a responder,
  `touring decompose ready task_1788344667602060021` deve devolver **S-1**.
- **Daemon degradado por carga concorrente**: `cli-decompose-ready`/`get` e `index find` estouraram o
  orçamento de 15 s (load ~3,4; outra sessão com ~80 arquivos no tree). O socket está vivo
  (`daemon-ctl status` OK, PID 4016168) e o `doctor` responde. É transiente — REGRA #19: aguardar e
  reconsultar, **nunca** `pkill`.
- **O binário já é 30.4.37** (o diagnóstico deste bundle foi tirado numa versão anterior, às 06:29).
  Some-se a isso a wave 14 de outra sessão: **re-medir órfãos, e2e e composite antes da S-1**.
- **Nada commitado**: o bundle está em disco como diretório novo (untracked) e o working tree tem
  trabalho concorrente de outra sessão. Commitar só o bundle: `git add docs/plans/2026-09-02-touring-proxima-geracao`.
- **Marker do loop arquivado** — o Stop hook não segura turno nenhum.

## Como retomar (INNER, W0)
1. `touring memory recall "plan:touring-proxima-geracao:2026-09-02"` + `touring decompose claim task_1788344667602060021 S-1 --owner <session>`.
2. S-1 sob tdd-enforcer: `crates/touring-hooks-shared/src/identity.rs` (**TurnId**, **InjectionId**, blake3, REGRA #17); testes `test_injection_id_is_deterministic_for_same_inputs`, `test_turn_id_prefers_tool_use_id_over_derived`.
3. S-2 ∥ S-3 ∥ S-4 → S-5 (KPI `telemetry_identity` ≥ 0,95) → `loop_phase_close.py --task task_1788344667602060021 --phase W0` → `loop_converged.py`.
4. Gotchas: `cargo test -p touring-hook-handlers --features pre-hooks,post-hooks`; `touring index rebuild` antes do `loop_converged` se editar por script; após tocar KPI/RPC, `cargo build -p touring-server --release` + `update-touring`.

## Decisões fixadas (não reabrir sem Gabriel)
- Contrato C: público fora-de-processo (JSON-RPC stdio + manifest) + built-ins Rust no mesmo trait · outcome v1 = sucesso da tool + adesão + STR · medição-primeiro · risco único: "morreu de contexto" (antídoto na W1, no executor) · Pronto #2/#3 com números PROPOSTOS a calibrar.

## Integração dos scouts (02/09 ~10:40, pós-pausa)

Os dois relatórios read-only chegaram ~40 min após o OUTER e foram **reverificados por leitura direta** antes de entrar nos docs (dois números do scout estavam errados: os nomes de hook são **239**, não 62; o despacho tem **245** entradas, não 225). Cinco correções materiais já aplicadas à estratégia §2.1/§2.2 e ao `plan.md`:

1. **O teto por chamada JÁ existe e é graduado por CILA** (`cila_budget_read` 800/2.000/4.000; `edit`/`write` 1.200/3.000/6.000 — cila.rs:33-51), truncado no executor (signal_pipeline.rs:139 e :195), com alertas a 75%/90% (`record_enrichment_metrics`, pre_read.rs:392-401). S-9 foi reescrita: o novo é o **acumulador por TURNO**, com a mesma forma e os mesmos contadores.
2. **`pre_edit` não grava `__context_injection_file__`** (só pre_read.rs:362 e pre_write.rs:279) — **edição é inatribuível hoje**; a S-6/S-7 fecha os três de uma vez (C08/REGRA #0).
3. **A identidade que existe é de CLASSE**: `ActionSignature::to_key()` (action_signature.rs:139-146) → `outcome:<tool_class>:<intent_class>:<qualifier>`. É o molde do **InjectionId** de instância (grep por `injection_id|Uuid::new|nanoid` nos 3 crates de hook = 0).
4. **Três vocabulários de evento incompatíveis**: 239 nomes em `ALL_DAEMON_HOOK_NAMES` (199 são `cli-*` de telemetria), 245 entradas de despacho, 27 tipos no settings.json do Claude Code. S-11 ganhou o guard `test_harness_event_covers_every_lifecycle_hook_name`.
5. **O dsh NÃO mede eficácia de contexto injetado** (`grep effectiveness|reward packages/` = 0): ele tem proveniência (`MessageSource{kind,plugin,form}`) e desfecho (`tools/result`), mas nada junta os dois. A forma vem dele; **a régua é nossa** — é o diferencial da W1.

Gates após a integração: gap_detector 0 P0 · plan_validator strict OK (6 fases, 19 subtasks, confiança 100%) · VGP 49 símbolos verificados · doc-link 7 docs, 0 quebrados · score 7,44.

## Pendências de ambiente vistas na sessão
- `plan-excellence`: nó `author` (headless sonnet + taco-planning) estourou 15 min duas vezes sem escrever — investigar antes de reusar para L4 (timeout, modelo, ou a skill exigindo scripts que o agente não acha).
- Scouts Explore ficaram ociosos sem entregar relatório nem responder mensagens (34 min).
- Graders corrigidos em `~/.claude/skills/taco-planning/scripts/` (gap_detector dict→list e fronteira de palavra; plan_validator header-only) — sincronizar `client/` no próximo `propagate-release` (`sync-client-skills.py --check`).
