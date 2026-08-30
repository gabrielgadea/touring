---
type: AuditReport
title: Cross-audit 30/08/2026 — wave P1-P5 do contrato do grafo de memória
description: Auditoria de fidelidade-de-propósito dos 8 commits da janela 29-30/08 (P1-P5, fix quality_gold, releases 30.4.24/25) — 1 achado corrigido no turno, prova comportamental 7/7 contra o binário instalado.
plan_id: 2026-08-29-work-outer
tags: [audit, cross-audit, memory, graph-contract]
timestamp: 2026-08-30T02:30:00-03:00
okf_version: "0.1"
---

# Cross-audit 30/08/2026 — P1-P5 do contrato do grafo de memória

**Escopo**: commits `d1b0455` (fix quality_gold) → `eaca49f` (P1+P1.5) →
`c0cdc54` (P2) → `111eb09` (P3) → `29517be` (P4) → `e831bd6` (P5) →
`76adcf0` (release 30.4.25) → `03ff709` (F-1 desta auditoria).
17 arquivos, +886/−55 (medido: `git diff --stat d1b0455~1..HEAD`).

## VERDICT

**PASS com 1 achado corrigido no próprio turno (F-1).** P0 0/6 FAIL nos 5
arquivos Rust tocados; workspace tier **Platinum 0.918** (loop_converged
exit 0, judge intact 8 cláusulas); prova comportamental **7/7 contra o
binário 30.4.25 INSTALADO** (não contra a fonte); dívida declarada: 1
(F2.1 chevron, fora do escopo desta wave por decisão registrada).

## FASE 1 — MAP (executado)

Superfícies novas e consumidores (varredura por símbolo, `crates/`):
`key_shape_ok` (3 sites: tags/kpi/ceg — fonte única D8) · `IgnoredTag` (8) ·
`split_query_tags_reporting` (7: query, recall, wrapper) · `describe_violations`
(3) · `graph_contract_share` (kpi braço+fn+teste) · `memory_edge_density` (3) ·
`SuggestLinks` (3) · `memory_coserved` (6: escritor no recall, leitor no
suggest, teste e2e) · `structure_bucket` (3) · `link_provenance` (Python:
main + teste + dogfood executado). **Zero órfãos reais (REGRA #0).**

## FASE 2 — PURPOSE AUDIT (executado)

Cada fase auditada contra `docs/memory-graph-contract.md`:

| Cláusula do contrato | Implementação | Fidelidade |
|---|---|---|
| P1 fail-loud ternário | `ignored_facets`/`unknown_facets` SEMPRE presentes (vazio ≠ ausente ≠ cheio) | ✅ provado vivo (P1a/P1b/P1c) |
| P1.5 reaponte | id determinístico recalculado, sucessão preservada, fail-open | ✅ provado vivo (sonda old→peer→new) |
| P2 feromônio | bucket DENTRO da classe; attach ANTES do rank | ✅ unit + contraprova (estrutura nunca cruza classe) |
| P3 advisory+KPI | `contract:{}` + `graph_contract_share`, predicado único | ⚠ **F-1** (abaixo) — corrigido |
| P4 procedência | criada PELO executor do close; abstract = projeção | ✅ dogfood: `provenance_links=2`, arestas lidas de volta |
| P5 derived | co-serviço durável; sugestão nunca-automática com apply 1:1 | ✅ e2e + shape vivo |

**F-1 (MÉDIO, corrigido em `03ff709`)**: o KPI e o advisory julgavam "kind
curado" por `entry_type`, mas o contrato governa por **faceta** (cláusula 3)
— o mesmo padrão do achado da peer no quality_gold (o veredito num campo, o
predicado lendo outro). Medido no corpus real: ~39 nós semantic com faceta
curada e entry_type divergente invisíveis; os 880 `transcript_lesson`
inicialmente suspeitos são tier `reference` e ficam fora **corretamente**.
Fix: KPI faz join em `memory_tags`; advisory aceita entry_type curado OU tag
kind curada. Mutação que mata: reverter para `entry_type IN (...)`
(`graph_contract_share_counts_by_facet_not_entry_type`, temp DB, `Some(0.5)`).

**Limites conhecidos registrados (não regressões)**:
- **F-2**: `attach_one_hop_links` lê só o memory.db canônico — entrada
  FEDERADA de outro projeto rankeia como órfã no P2 mesmo que ligada na
  origem (comportamento pré-existente do attach; candidato quando o P2 for
  calibrado com o corpus da peer, que tem 2 populações independentes de ilha).
- **F-3**: `portfolio.rs:264` segue no wrapper silencioso `split_query_tags`
  — aceitável (não é superfície de resposta de memória); anotado.

## FASE 3 — DEBT SCAN (executado)

`TODO|FIXME|HACK|unimplemented!|todo!` no delta: **0 marcadores** (varredura
nos 7 arquivos de código tocados). Dívida declarada e datada, fora do escopo
desta wave por decisão: falso positivo **F2.1** (chevrons em string Python
lidos como XSS CWE-79, sem sink — reportado pela peer por ordem de Gabriel,
memória `debito:f2-1-chevron-falso-positivo-xss:2026-08-30`, discriminante de
sink proposto para a wave do detector).

## FASE 4 — HARMONY (executado)

- **P0 BLOCK (6 dims × 5 arquivos)**: `tags.rs`, `rlm.rs`, `ceg_impls.rs`,
  `cli/memory.rs`, `kpi.rs` → **P0_FAILS: none** em todos.
- **50-dim no escopo**: tier **Platinum, composite 0.9183** (cláusula
  quality_gold do loop_converged, corpo inteiro, 0 dims truncadas).
- **Wiring**: orphans do escopo ≤ baseline nomeada (cláusula orphans_base
  PASS); guard braço↔commitment do KPI verde nas 2 direções (40 testes).

## FASE 5 — FIX & POTENTIALIZE (executado)

F-1 corrigido nos DOIS executores (KPI + advisory) a partir da mesma fonte
governada — potencializa (o predicado agora cobre o superconjunto correto),
nunca reduz. Nenhum outro fix necessário.

## FASE 6 — E2E PROOF (executado, dupla)

1. **Suites**: touring-intelligence memory 217/217 · touring-cli 486/486 +
   kpi 40/40 · touring-hook-runtime 382/382 · e2e `cli_handlers`
   suggest-links 1/1 · skill loop-engineering 56/56 (pytest) ·
   clippy `-D warnings` **0** nos crates tocados.
2. **Comportamental contra o binário INSTALADO 30.4.25**
   (`docs/plans/2026-08-29-work-outer/validate_p1_p5_e2e.sh`, re-executável):
   **7/7** — ignored_facets com razão que ensina as 7 facetas · array vazio
   quando tudo aceito · unknown_facets no query · supersede reapontando
   aresta viva · contract advisory com key_shape=true · suggest-links shape
   honesto · braços KPI avaliados no `kpi -j`.
3. **Pipeline de release**: propagate 30.4.25 → analise + konverter
   `lock=30.4.25, touring 30.4.25`, gate 5.5 **35/35**.

Cobertura P2-vivo: a lógica do rank é unit-provada com contraprova; o efeito
em produção depende de corpus com as duas populações (a calibração
antes/depois combinada com a peer `analise-e0` quando os recalls acumularem
co-serviço).

## FASE 7 — este report

Convergência de registro: `loop_converged --task task_1788054456569794142`
→ **converged: true, exit 0** (judge_intact PASS — mudanças de grader
declaradas via `judge_attest --attest`). O F-1 está na fonte (`03ff709`) e
viaja na próxima propagação; a fonte à frente da toolchain imutável é o
desenho L1/L2, e a prova de instalação é sempre comportamental, nunca o
rótulo.

## Procedência

Auditoria executada pela mesma sessão que implementou a wave — mitigada por:
juízes determinísticos (exit codes, counters, sondas SQL), validador
re-executável no bundle, e o achado F-1 ter nascido de autocrítica com
medição (a sonda refutou metade da hipótese inicial: transcript_lesson fora
por tier é CORRETO). Painel cego (`touring adw run cross-audit`, 3 críticos)
fica disponível como segunda passada se Gabriel a ordenar.
