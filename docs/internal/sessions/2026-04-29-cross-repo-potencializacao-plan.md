# Plano de Potencialização Touring — Análise Cross-Repo

> **Data**: 2026-04-29 | **Análise**: rtk-ai/rtk + mksglu/context-mode + tirth8205/code-review-graph
> **Autor**: TACO Orchestrator (sessão Claude Code) | **Estado**: PLAN (não-executado)
> **Sprints**: 2 sprints, 10–12 dias eng | **Risco geral**: LOW–MEDIUM

---

## <objective>

**O quê**: Potencializar 5 subsistemas do Touring (wiring, memory recall, ast blast, PreCompact, hook outputs) extraindo padrões validados em produção de 3 repositórios análogos.

**Por quê**:
1. `composite_health_score` atual = **0.503** (alvo 0.65)
2. `learning.ema_reward` = **0.270** (alvo 0.40 em 4 semanas)
3. **199.656** orphans no wiring DB — alto volume de FPs (Cadeia 7 VP-Scout)
4. Hook outputs verbosos consomem context window do Claude (sem ceiling)
5. Memory recall usa cosine OR FTS5 separados — sem fusão hybrid

**Sucesso**: 5 deliverables atômicos shipados, métricas-alvo atingidas em 4 semanas, zero regressão E2E.

</objective>

---

## Estado do Sistema (Baseline 2026-04-29)

| Métrica | Valor | Origem |
|---|---|---|
| `daemon_health` | ok (2 projects) | `touring doctor -j` |
| `index.symbol_count` | 1.097.890 | `touring status -j` |
| `wiring.orphan_count` | 199.656 | `touring status -j` |
| `learning.ema_reward` | 0.270 | `touring status -j` |
| `composite_health_score` | 0.503 | `touring status -j` |

---

## Insights Extraídos por Repo

### Repo 1 — rtk-ai/rtk [FACT 1.0]
- **Stack**: Rust single-binary, v0.38.0, 38.4k stars, prod
- **Differential**: TOML-DSL filter pipeline compilado em build-time (8-stage)
- **Padrões úteis**: 3-tier graceful parser, tee-and-hint, learn-from-correction loop, single-exit-point invariant

### Repo 2 — mksglu/context-mode [FACT 1.0]
- **Stack**: TypeScript MCP, v1.0.103, 11.2k stars, prod (Elastic License 2.0)
- **Differential**: Subprocess sandbox + event-typed log + RRF (Reciprocal Rank Fusion)
- **Padrões úteis**: Priority-tiered ≤2KB snapshot (P1-P4), Porter+trigram+Levenshtein fusion, 24h TTL cache, throttling progressive degradation

### Repo 3 — tirth8205/code-review-graph [FACT 1.0]
- **Stack**: Python + SQLite + NetworkX + tree-sitter, v2.3.2, ativo
- **Differential**: Multi-dimensional knowledge graph + confidence tiers + risk score multifator
- **Padrões úteis**: `confidence_tier ∈ {EXTRACTED, INFERRED, AMBIGUOUS}`, surprise score explainable, knowledge gaps 4-categorias, token-budgeted BFS, diff→node range intersection

### Convergências Cross-Repo (3 padrões em 2+ repos)

| Padrão | Repos | Gap no Touring |
|---|---|---|
| **C1**: Hybrid retrieval (RRF fusion) | context-mode + CRG | Cosine OR FTS5 separados |
| **C2**: Bounded context budget | context-mode + CRG + RTK | Outputs CLI sem ceiling |
| **C3**: Confidence/risk tiers explícitos | CRG + RTK | Booleans + scores 0-1 sem distinção tier |

---

## <deliverables>

### **T1 — Confidence Tier Ternário em Wiring Edges** [FACT 0.9 → CRG]

**Origem**: `code_review_graph/graph.py` schema `edges.confidence_tier`

**Problema**: Wiring DB sofre staleness silenciosa — VP-Scout Chain 7 reporta ~5 FPs/sessão.

**Implementação**:
- Schema bump SCHEMA_VERSION 8→9 com migration idempotente
- Enum `ConfidenceTier { Extracted, Inferred, Ambiguous }`
- Float `confidence: f32` em [0,1]
- Tagging logic em `resolve_consumers`:
  - `EXTRACTED` (1.0): tree-sitter symbol_table direct match
  - `INFERRED` (0.7-0.9): re-export, generic monomorphization
  - `AMBIGUOUS` (<0.7): trait dispatch, macro expansion, dyn dispatch
- CLI: `touring wiring orphans --min-confidence 0.8`, `touring wiring audit --include-tiers`

**Componente**: `crates/touring-wiring/`
**Tamanho**: **M** (4–6h)
**Risco**: **LOW** (campos `#[serde(default)]` backward-compat)
**Validação**: -40% FPs em VP-Scout Chain 7 reports

---

### **T2 — Hybrid Retrieval (RRF) em memory recall + tantivy search** [FACT 1.0 → context-mode + Context7]

**Origem**: `context-mode/src/search/` + Context7 Tantivy `Bm25StatisticsProvider`

**Problema**: Touring usa cosine embeddings OR FTS5 separados — queries com typo ou semânticas diferentes do índice falham silenciosamente.

**Implementação**:
- `rrf_fuse(bm25_hits, cosine_hits, k=60.0) -> Vec<Hit>` em `touring-memory/src/recall.rs`
- Levenshtein typo correction como pre-rewrite (porta de `touring tantivy fuzzy`)
- `SnippetGenerator::create()` + `set_max_num_chars(100)` para snippets per-hit
- Heading-weighted boost (5×) em markdown indexed (`~/.claude/rust/docs/`)
- Feature flag `TOURING_RECALL_FUSION=1` por 2 weeks antes de default

**Componente**: `crates/touring-memory` + `crates/touring-tantivy`
**Tamanho**: **S–M** (1–2d)
**Risco**: **LOW** (additive ranking; old behavior preserved behind flag)
**Validação**: +20–40% precision em queries noisy/typoed (mock evaluation set)

---

### **T3 — Token-Budgeted BFS + Hard Byte Ceiling em PreCompact** [FACT 1.0 → CRG + context-mode]

**Origem dual**:
- `code_review_graph/graph.py::get_impact_radius(max_nodes, token_budget)`
- `context-mode/src/lifecycle.ts` — ≤2KB priority-tiered XML

**Problema**:
- `touring ast blast` em files de fan_out alto retorna 10k+ symbols
- `PreCompact` hook não tem ceiling de bytes → snapshot pode estourar context

**Implementação**:

**T3a** — Token-budget em ast blast:
```bash
touring ast blast <file> --max-nodes 200 --token-budget 4000
# Heap priority: (degree DESC, line_proximity ASC, test_first)
```
- Adicionar `gate_metrics.blast_truncated_count` counter

**T3b** — Priority-tiered PreCompact snapshot (≤2 KB total):

| Tier | Conteúdo | Budget |
|---|---|---|
| P1 | Active task DAG + recent decompose | 800 bytes |
| P2 | Wiring orphan deltas + gate-metrics | 600 bytes |
| P3 | RL ema_reward + linucb arms | 400 bytes |
| P4 | Peripheral counters | 200 bytes |

- Drop tiers em ordem (P4 → P3 → P2 → P1) se budget apertado
- E2E test mandatório: SessionStart restore round-trip

**Componente**: `crates/touring-ast/blast.rs` + `crates/touring-hooks/pre_compact.rs`
**Tamanho**: **M** (2–3d)
**Risco**: **MEDIUM** (PreCompact mexe com SessionStart restore; precisa E2E completo)
**Validação**: P99 latency `ast blast` (fan_out>1000) -3×; PreCompact bytes ≤ 2048

---

### **T4 — Surprise Score Explicável + Knowledge Gaps Taxonomy** [FACT 1.0 → CRG]

**Origem**:
- `analysis.py::find_surprising_connections` (5 fatores ortogonais com `reasons[]`)
- `analysis.py::find_knowledge_gaps` (4 categorias)

**Problema**: 45 WIRED_PAIRS estáticos sem ranking de discovery; orphans é única "gap" category.

**Implementação T4a — `touring synergy surprises -j`**:
```rust
struct SurprisePair {
    src: String, dst: String, score: f32,
    reasons: Vec<String>,
}
// Fatores:
// +0.30 cross-crate
// +0.20 cross-feature-flag
// +0.20 low-degree → hub
// +0.15 test↔prod boundary
// +0.15 confidence_tier=AMBIGUOUS (depende de T1)
```

**Implementação T4b — `touring wiring gaps -j`** (4 categorias):

| Categoria | Detector | Ação sugerida |
|---|---|---|
| **isolated** | degree ≤ 1 | Wire candidate |
| **thin_community** | community_id count<3 | Merge candidate |
| **untested_hotspot** | degree≥5 (p90) + `is_tested=false` | Test priority |
| **single_file_community** | community_id ≥3 + count(file)=1 | Extract crate candidate |

**Best practice Context7 NetworkX**: `louvain_communities(G, resolution=2.0)` quando comunidade > 25%.

**Componente**: `crates/touring-server/synergy.rs` + `crates/touring-cognitive/`
**Tamanho**: **M** (1–2d)
**Risco**: **LOW** (read-only sobre dados existentes)
**Dependência**: T1 (para fator AMBIGUOUS no surprise score)
**Validação**: ≥30 untested_hotspots + ≥5 single_file_communities reais detectados

---

### **T5 — TOML-DSL Filter Pipeline para Hook Outputs** [FACT 1.0 → RTK]

**Origem**: `rtk/src/core/toml_filter.rs` — 8-stage pipeline DSL compilada via `build.rs`.

**Problema**: Hook outputs (`pre-read`, `instructions-loaded`, `synergy --with-metrics`) emitem payloads verbosos que entopem context window do Claude.

**Implementação**:
- Novo crate `touring-filter`
- `build.rs` concatena `filters/*.toml` em runtime constant via `include_str!`
- 8-stage pipeline: `strip_ansi → replace → match_output → strip/keep → truncate → head/tail → max_lines → on_empty`
- Inline `[[tests.<filter>]]` blocks para validação compile-time (RTK pattern)
- Three-tier override (project → user → built-in, first-match-wins)

**Exemplo POC** — `filters/instructions_loaded.toml`:
```toml
[filter]
name = "instructions_loaded_compress"
match_tool = "instructions-loaded"

[[stages]]
op = "strip_ansi"

[[stages]]
op = "match_output"
pattern = "^Touring Knowledge:"
keep_block = 1

[[stages]]
op = "truncate"
max_chars = 2000

[[stages]]
op = "on_empty"
fallback = "[touring: no signals]"
```

**Componente**: Novo crate `touring-filter` + integração `touring-hooks`
**Tamanho**: **M** (2–3d)
**Risco**: **LOW** (additive; falls through to passthrough on no-match)
**Validação**: -50% a -80% tokens em top-10 hook outputs

</deliverables>

---

## <timeline>

### Sprint 1 (5 dias) — Fundamentos

```
Dia 1: T1 (Confidence Tier)
       ├─ Schema migration SCHEMA_VERSION 8→9
       ├─ Enum + tagging logic
       ├─ Backup .db.bak automático
       └─ Tests + memory store lessons

Dia 2: T2 (RRF Fusion)
       ├─ rrf_fuse() implementation
       ├─ Levenshtein pre-rewrite (port de tantivy fuzzy)
       ├─ Feature flag TOURING_RECALL_FUSION=1
       └─ Mock eval set para validação

Dia 3-4: T4 (Surprise + Gaps) [DEPENDE de T1]
         ├─ touring synergy surprises -j
         ├─ touring wiring gaps -j (4 categorias)
         ├─ Resolution scaling no Leiden (>25% threshold)
         └─ Integration com TDG dimensão "edge_confidence"

Dia 5: Validação E2E + memory store de lessons
       ├─ touring e2e -j round-trip
       ├─ Métricas baseline → atual (ema_reward, composite)
       └─ touring memory store wave_2026_04_29_lessons
```

### Sprint 2 (5 dias) — Context Hardening

```
Dia 1-2: T3a (token-budget blast)
         ├─ Heap priority BFS (degree, proximity, test_first)
         ├─ gate_metrics.blast_truncated_count counter
         └─ Benchmark P99 fan_out>1000

Dia 3-4: T3b (PreCompact priority tiers)
         ├─ Tier classifier P1-P4
         ├─ Hard byte ceiling 2048
         ├─ E2E SessionStart restore round-trip [CRÍTICO]
         └─ Drop policy P4→P3→P2→P1

Dia 5: T5 POC (TOML-DSL pipeline)
       ├─ Crate touring-filter scaffolding
       ├─ build.rs concat + include_str!
       ├─ 3 filtros POC (pre-read, instructions-loaded, synergy)
       └─ Inline [[tests]] validation
```

### Backlog Secundário (T6-T10, fase posterior, não bloqueante)

| ID | Origem | Descrição | Prioridade |
|---|---|---|---|
| T6 | RTK | 3-tier graceful parser (Full/Degraded/Passthrough) — combate cold-start race | MEDIUM |
| T7 | RTK | Tee-and-hint recovery — preserva acesso ao raw output truncado | LOW |
| T8 | context-mode | Throttling progressive degradation — anti-thrashing em pre-read | MEDIUM |
| T9 | context-mode | Subprocess sandbox para pre-bash — token envelope em commands heavy | LOW (L4 invasive) |
| T10 | CRG | Diff→node range intersection em pre-edit — granularidade RL per-symbol | MEDIUM |

### Dependências (DAG)

```
T1 ─────┬──► T4 (depende de confidence_tier)
        │
T2 ─────┘  (independente)

T3a ─── independente
T3b ─── independente

T5 ──── independente

Caminho crítico: T1 → T4 (Sprint 1)
Paralelizável: T2, T3a, T3b, T5
```

</timeline>

---

## <risks>

| ID | Risco | Probabilidade | Impacto | Mitigação |
|---|---|---|---|---|
| R1 | T1 schema migration corrompe wiring DB existente | LOW | HIGH | `SCHEMA_VERSION 8→9` migration idempotente; backup `.db.bak` automático; rollback testado em CI |
| R2 | T2 RRF fusion altera ranking esperado quebrando memory recall existente | MEDIUM | MEDIUM | Feature flag `TOURING_RECALL_FUSION=1` por 2 weeks antes de virar default; comparison metrics antes/depois |
| R3 | T3b PreCompact snapshot quebra SessionStart restore (perda de state cross-session) | MEDIUM | HIGH | E2E test mandatório `touring e2e -j` com restore round-trip; preserve old format por 1 release |
| R4 | T4 surprise score gera false positives "óbvios" inundando observability | MEDIUM | LOW | Threshold mínimo de 0.5; flag `--min-score N`; iterate threshold based on user feedback |
| R5 | T5 build.rs concat falha silenciosamente em filter inválido | LOW | MEDIUM | Validação inline `[[tests]]` blocks (RTK pattern); cargo test gate |
| R6 | Roadmap atrasa por dependência T1→T4 mais lenta que esperado | LOW | LOW | T2, T3, T5 são paralelos — Sprint 1 não bloqueia se T1 atrasar 1 dia |
| R7 | composite_health_score não atinge 0.65 mesmo após 5 entregas | MEDIUM | LOW | Aceito — métrica influenciada por múltiplos fatores externos; monitor evolution drift |
| R8 | Touring daemon degrada durante migrations (T1) | LOW | HIGH | Migrations rodam offline (daemon parado); `update-touring --no-restart` flag |

</risks>

---

## Self-Validation

| Critério | Status | Evidência |
|---|---|---|
| Cada deliverable é atômico e independentemente shipável? | ✅ | T1, T2, T3a, T3b, T5 podem ser merge separadamente; T4 depende de T1 mas é shipable após |
| Dependências são explícitas e acíclicas? | ✅ | DAG: T1 → T4; T2/T3a/T3b/T5 paralelos; sem ciclos |
| Estimativas são realistas? | ✅ | T-shirt sizing (S/M): T1=M, T2=S-M, T3=M, T4=M, T5=M; total ~10-12d eng condiz com complexidade |
| Riscos têm mitigações? | ✅ | 8 riscos identificados, todos com mitigação concreta |
| Métricas de validação são mensuráveis? | ✅ | Cada T tem métrica numérica (-40% FPs, +20-40% precision, P99 -3×, -50% tokens) |

---

## Validação Cross-Cutting (4 semanas)

| KPI | Atual | Target | Como Medir |
|---|---|---|---|
| `learning.ema_reward` | 0.270 | **0.40** | `touring learning status -j` |
| `composite_health_score` | 0.503 | **0.65** | `touring status -j \| jq '.composite_health_score'` |
| FPs em VP-Scout Chain 7 | ~5/sessão | **≤2/sessão** | Session reports + `touring memory recall "WIRING_STALE"` |
| Tokens médios em hook outputs | baseline | **-50%** | `gate_metrics.hook_output_bytes_avg` (novo counter) |
| P99 latency `ast blast` (fan_out>1000) | baseline | **-3×** | `gate_metrics.blast_p99_ms` |
| `wiring.orphan_count` reclassificação | 199.656 | ~120k EXTRACTED + ~80k AMBIGUOUS | `touring wiring audit -j --include-tiers` |

---

## Padrões NÃO Recomendados (Rejeitados)

| Padrão | Origem | Motivo da rejeição |
|---|---|---|
| `automod::dir!()` ergonomic registration | RTK | Quebra invariante "no WIP files leaking" (REGRA #0) |
| Subprocess sandbox completo | context-mode | L4 invasive, ROI não justifica vs T1+T2 |
| D3 force-directed visualization | CRG | Fora de scope CLI-first do Touring |
| Neo4j Cypher exporter | CRG | `touring-scip` já cobre export |

---

## Sinergia com Frameworks Existentes

| Recomendação | Sinergia |
|---|---|
| **T1** (confidence tier) | Alimenta TDG 6-dim com nova dimensão "edge_confidence" |
| **T2** (RRF fusion) | Combina com `touring tantivy suggest` (autocomplete) |
| **T3** (token budget) | Conecta com `gate_metrics.health_delta_*` (W12-13) |
| **T4** (surprise + gaps) | Estende `WIRED_PAIRS` (45) com `SURPRISE_PAIRS` dinâmico |
| **T5** (TOML-DSL) | Compatível com Wave 12 RFC-100 emission patterns |

---

## Insights-Chave (TL;DR)

1. **Os 3 repos convergem em "context budget hardening"** — Touring tem ~6 hooks que emitem outputs sem ceiling; este é o vetor de otimização de maior ROI.
2. **CRG é o mais arquiteturalmente alinhado** — confidence tiers + risk score + gaps taxonomy são adições aditivas de baixo risco que endereçam falhas reais documentadas (VP-Scout Chain 7).
3. **context-mode trouxe a maior inovação algorítmica** — RRF fusion + priority-tiered snapshot são padrões de alto impacto/baixo esforço.
4. **RTK contribui com pattern declarativo** — TOML-DSL pipeline é o único caminho realista para "filter as code" no Touring sem inflar a CLI.
5. **Touring já está à frente em**: RL stack (LinUCB+QTable+MCTS), VGP typestate pipeline, daemon actor pattern, hook registry coverage (176 events). Os repos analisados não têm equivalente.

---

## Confidence Tags

- **FACT [1.0]**: Estado dos 3 repos via WebFetch (READMEs, schemas, Cargo/package); estado do Touring via `touring doctor/status`; padrões algorítmicos (RRF, BM25, Leiden) via Context7
- **INFERENCE [0.85]**: Mapeamento de cada padrão a componente Touring específico; estimativas T-shirt; métricas de validação
- **SPECULATION [0.6]**: Target `composite_health_score 0.503 → 0.65` em 4 semanas — depende de adoção sequencial das 5 entregas + fatores externos

---

## Próximos Passos (aguardando autorização Gabriel)

1. ✅ Plano salvo em `~/.claude/rust/docs/2026-04-29-cross-repo-potencializacao-plan.md`
2. ⏳ Aguarda autorização para iniciar Sprint 1 (Dia 1: T1 Confidence Tier)
3. ⏳ Após autorização: TACO L4 — FASE 0 (cargo check + doctor) → FASE 1 (scout T1) → FASE 2 (architect schema migration) → FASE 4.5 (auditor anti-FP) → FASE 5 (engineer)

---

## Referências

| Tipo | Path |
|---|---|
| Repos analisados | `https://github.com/rtk-ai/rtk` (v0.38.0), `https://github.com/mksglu/context-mode` (v1.0.103), `https://github.com/tirth8205/code-review-graph` (v2.3.2) |
| Best practices Context7 | Tantivy `/websites/rs_tantivy_tantivy`; NetworkX `/networkx/networkx` |
| Touring frameworks relacionados | `~/.claude/rules/VP-Scout.md` (Cadeia 7 wiring staleness); `~/.claude/rules/TACO-subagent.md` (FASE 0 + 4.5 gates); `~/.claude/skills/Touring/SKILL.md` (skill v4.24.0) |
| Memória persistente | `~/.claude/projects/-home-gabrielgadea/memory/MEMORY.md` (Wave history) |

---

_TACO Phase 7 (documentação) — registrado por Claude Code orchestrator | 2026-04-29 | Estado: PLAN aguardando autorização_
