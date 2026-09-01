---
type: Analysis
title: "Medição 57-vs-hooks reconciliada + exploração de sinais para o SignalLayer"
description: "Recuperação da medição 57-vs-hooks (10 diretas + 6 parciais + 41 gaps = 72%), reconciliação da partição 14 reativos únicos + 27 sob-demanda, e exploração ranqueada (Tier A/B/C) de quais sinais merecem promoção a SignalLayer — com régua de 4 eixos e evidência de dano por candidato"
plan_id: 2026-08-31-complementacao-hooks
content_blake2b: 3de1c224393cd353558d9297e68bc671
tags: ["#kind:decision", "#domain:hooks", "#process:exploration", "#status:pending-human-gate"]
timestamp: 2026-09-01T17:19:41.256540-03:00
---

## A medição 57-vs-hooks — recuperada e reconciliada

Fonte primária: `docs/plans/2026-08-31-code-mode-sinal/analise-comparativa-57-vs-hooks.md` (31/08 15:35), cruzando as **57 funções SDK propostas** (survey F1.9 dos 42 crates) contra a cadeia real de hooks (27 eventos × 82 handlers × 47 subcmds touring-hook × 5 bridges × 146 pub_symbols).

**Os números [FACT 1.0]:**

| Categoria | N | % | Significado |
|---|---:|---:|---|
| Cobertura DIRETA | 10 | 17% | hook já injeta o equivalente (ast_meta, blast_radius, doctor, memory_recall, recommend…) |
| Cobertura PARCIAL | 6 | 11% | subset chega (tantivy 1-2 hits, speculate-like no pre-edit…) |
| **GAP** | **41** | **72%** | o sinal existe no CLI/daemon e NUNCA chega ao contexto do LLM |

**Reconciliação dos off-by-ones [INFERÊNCIA 0.9, aritmética fecha]:** a tabela MUST do doc lista 14 linhas com 2 duplicatas explícitas ("já listado": call_graph, scan_vulnerabilities) → **12 MUST únicos** + 29 SHOULD/NICE = 41 ✓. Na partição, os "15 reativos" H1-H15 carregam 1 alias (H10 = H5 gotcha_match) → **14 sinais reativos únicos** + **27 sob-demanda** = 41 ✓. A partição "15 ↔ 27" fecha exatamente com o dedup — e é LIMPA no critério: **reativo = empurrado-por-evento** (o sinal muda a decisão do próximo tool_use, então tem de chegar sem ser pedido) vs **sob-demanda = puxado-por-SDK** (análise profunda que o programa consulta quando precisa, via `touring run --orchestrate`).

**Estado pós-execução das duas frentes [FACT 1.0, verificado hoje]:** dos 12 MUST-gaps, a complementacao-hooks cobriu 8 (quality, symbols, dependents, pub_api_diff, gotchas, scan_vulnerabilities, code_mode_status, entity_id) + 6 do tier SHOULD (orphans, impact, find_references, audit_unsafe, temporal_drift, evolution_status). **Ficaram 4 MUST-gaps descobertos: `call_graph`, `tasks_compile`, `assists_for_file`, `related_symbols`.** Os 27 sob-demanda são alcançáveis hoje pelos 71 hooks do allowlist do SDK orchestrate.


## A régua de promoção (4 eixos)

Um sinal merece promoção a SignalLayer quando os 4 eixos concordam — a régua deriva da doutrina de enriquecimento (E2: o sinal alcança a decisão ANTES de ela ser tomada) e do STR:

1. **Valor decisório no instante do evento** — o sinal muda o que o LLM faz no PRÓXIMO tool_use? (Se só informa análise, é sob-demanda.)
2. **Frequência × latência** — o SignalLayer Rust direto roda in-process (<5ms; precedentes documentados no pipeline: BlastRadius ~8ms budget-capped, EnrichedBlast ~2ms, WeightedBlast ~5ms). O que era inviável por subprocess virou viável — mas evento de alta frequência (todo Edit) exige a ponta baixa do budget.
3. **O silêncio atual custa erro MEDIDO** — só promove sinal cuja ausência tem evidência de dano (gotcha, post-mortem, KPI), nunca por completude.
4. **STR — densidade** — o injetável é o RESUMO que decide (1 linha, campos tipados), jamais o artefato completo (grafo inteiro satura contexto; `additionalContext` gordo é dívida).

Governança de cada promoção: 1 `impl SignalLayer` (name/enrich/should_run) + `add_layer` no pipeline do evento, gated por `CilaGatedLayer` quando caro; nasce com teste golden no harness dos 15 e entra na régua `hooks_complement` (o que de quebra força materializar o composite no `touring kpi -j` — pendência #2 da avaliação do plano).


## Exploração ranqueada — Tier A, B, C

**TIER A — promover já (evidência de dano viva):**

1. **`check_compile` — PostToolUse(Edit|Write) em `.rs`** (≈ o MUST-gap `tasks_compile` generalizado). Fire-and-forget `cargo check -p <crate tocado>` com o veredito injetado no turno seguinte (1 linha: `check: OK | 2 erros em X.rs:41`). Evidência do dano [FACT 1.0]: o G5 mediu **P9 verify-after em 17%** — e avisou isso DUAS VEZES nesta própria sessão. É o antipadrão A5 (mutar sem validar) fechado por afordância, não por persuasão. Cuidado de desenho: lock do target/ (usar `--message-format=short`, target-dir próprio ou debounce) e debounce por rajada de edits.
2. **`assists_for_file` / cross-caller — PostToolUse(Edit)** (MUST-gap descoberto). Sítios ANÁLOGOS ao editado (ANN/assist): "3 callsites com o mesmo padrão não foram tocados". É o C08 institucionalizado como sinal. Evidência [FACT 1.0]: o post-mortem de 2026-05-10 (~40min perdidos por assimetria de callers) é a origem da própria Decision Matrix.
3. **`related_symbols` — PreToolUse(Write) em arquivo novo/símbolo novo** (MUST-gap descoberto). Contratos vizinhos por similaridade antes de criar código (C07/VGP): "já existem `parse_hook`, `classify_bash_command` neste módulo". Ataca a duplicação na ORIGEM — os 5.409 órfãos do workspace (REGRA #0) são em parte símbolos criados sem ver os vizinhos.

**TIER B — promover com desenho de STR (o resumo, nunca o artefato):**

4. **`call_graph` compacto symbol-level — PreToolUse(Edit)** (MUST-gap): fan-in/fan-out da FUNÇÃO-alvo em 1 linha (`fn X: 3 callers, 5 callees, 1 recursivo`). O H3/dependents é file-level; a decisão de refactor é symbol-level. O grafo COMPLETO permanece sob-demanda.
5. **`tdg_grade` — PreToolUse(Edit)**: a LETRA + ação (`TDG D — STOP refactor antes de edit`). A regra constitucional já existe; hoje o pre-edit injeta quality_score mas não a letra acionável [INFERÊNCIA 0.8 — verificar se o Signal 13 atual já a carrega antes de implementar].

**TIER C — manter sob-demanda (NÃO promover):** wiring chains/cycles completos, TDG report integral, rust-semantic, hybrid_search/embed/fuse_rrf, predict_layer7/psi_pressure, e2e composite, session_graph, diagnostics profundos. Motivo comum: raramente decisórios no instante do evento, caros, e o SDK `--orchestrate` (71 hooks no allowlist) já os alcança puxados — que é o canal certo para análise.

**Sequência recomendada**: A1 → A2 → A3 (cada um = 1 SignalLayer + golden test + entrada na régua), depois B4/B5 sob medição de latência real. Nenhum novo handler no settings.json — tudo entra pelo pipeline SignalLayer existente (a arquitetura que a execução da Frente 2 provou melhor).
