# Análise Profunda: `pre_read.rs` — Estratégia de Potencialização Exponencial

> **Data**: 28/03/2026 | **Versão Touring**: v28.12.0 | **Arquivo Alvo**: `crates/touring-hooks/src/pre_read.rs`
> **Objetivo**: Aperfeiçoar o hook de pré-leitura do Claude Code integrando capacidades subutilizadas do workspace.

---

## 1. Arquitetura Atual — Diagnóstico Completo

### Pipeline atual (`run_returning`):

```
input.file_path
  ↓
1. CILA level → budget (800 / 2000 / 4000 chars)
  ↓
2. compose_high_signal_context_budgeted()
   ├── DB batch_pre_read_signals() → notes, gotchas, bash_failures, dependents
   ├── Ranking: recency × weight (Gotchas=2.0, Bash=1.5, Notes=1.5, Deps=1.0 fixo)
   └── Assembly budget-aware
  ↓
3. build_symbol_map_signal() → "📌 defs[N]: Name(12)·fn2(45)..."
  ↓
4. enrich_with_cognitive() → risk score + next tool prediction
  ↓
5. HookResponse::Context { context }
```

### Sinais injetados hoje:
| Sinal | Fonte | Peso | Exemplo |
|-------|-------|------|---------|
| Notes/gotchas | FileKnowledgeDB | 1.5–2.0 | `⚠️ GOTCHA [high]: unwrap here causes panic` |
| Bash failures | FileKnowledgeDB | 1.5 | `` `cargo` failed on this file: error[E0308] `` |
| Dependentes count | FileKnowledgeDB | **1.0 (fixo!)** | `3 files import this: [a.rs, b.rs, c.rs]` |
| Large file hint | fs::metadata | 1.2 | `💡 ~420 linhas: touring ast overview file.rs` |
| Symbol map | SymbolStore | (prepend) | `📌 defs[5]: run(12)·build_signal(88)·...` |
| Risk score | CognitiveRuntime | (append) | `⚠ Risk: 70%` |
| Next tool | CognitiveRuntime | (append) | `🔮 Next: Edit (80%)` |

---

## 2. Análise de Gaps — 7 Capacidades Não Integradas

### Gap #1 — `graph.rs` BlastRadius ❌ NÃO INTEGRADO — Valor: **ALTO**

**O que existe**: `runtime.infra.symbol_index: Option<SymbolIndex>` com
`blast_radius_with_depth(file, depth)` → `BlastRadius { affected_files, max_distance, file_count }` +
`weighted_blast_radius()` (Dijkstra + co_edit_weight) +
`EnrichedBlastRadius { direct_dependents, transitive_dependents, co_edited_files, severity: 0.0–1.0 }`.

**O que pre_read usa**: apenas `signals_data.dependent_count` (inteiro simples da DB).

**Diferença crítica**:
- Atual: `"3 files import this: [a.rs, b.rs]"` — Claude sabe que há 3 dependentes diretos
- Com BlastRadius: `"⚡ blast(dist=3, 12 files); diretos: [a.rs, b.rs]"` — Claude entende o impacto REAL

**Por que ALTO**: Uma mudança de assinatura pode afetar 12 arquivos transitivamente. Sem isso, Claude edita sem perceber o raio de impacto.

---

### Gap #2 — `call_graph.rs` + SymbolIndex external callers ❌ NÃO INTEGRADO — Valor: **ALTO**

**O que existe**: `SymbolIndex.symbols` mapeia nome → `Vec<SymbolLocation { file_path, is_definition }>`.
Cruzando definições neste arquivo com referências em outros arquivos, obtemos callers externos.

**O que pre_read usa**: NADA.

**Sinal potencial**: `"📞 callers externo: run_returning(3↑)·compose_signal(5↑)"` → Claude nunca quebrará uma função sem perceber.

**Por que ALTO**: Principal causa de bugs — Claude modifica uma função sem saber que é chamada em N arquivos externos.

---

### Gap #3 — `import_resolver.rs` / `module_tree.rs` re-exports ❌ NÃO INTEGRADO — Valor: **ALTO**

**O que existe**: `ModuleTree::build_from_source(source, path)` → `ModuleNode { re_exports: Vec<String> }`.
Detecta `pub use` re-exports — visibilidade pública invisível ao ler o arquivo.

**O que pre_read usa**: NADA.

**Sinal potencial**: `"⚡ pub re-exports[3]: HookResponse, run, DEFAULT_CONTEXT_BUDGET — break = API break"`

**Por que ALTO**: `pub use` é **invisível** ao ler o arquivo fonte — Claude frequentemente quebra API pública sem perceber.

---

### Gap #4 — `semantic_search.rs` SemanticSymbolIndex ❌ NÃO INTEGRADO — Valor: **MÉDIO**

**O que existe**: `SemanticSymbolIndex` com `find_similar_symbols(query, threshold, top_k)` e 16-dim feature vectors.

**Sinal potencial**: `"🔗 similar: [post_read.rs:run_returning(sim=0.92)]"` — guia para implementações análogas.

---

### Gap #5 — `module_tree.rs` hierarquia de módulos ❌ NÃO INTEGRADO — Valor: **MÉDIO**

**Sinal potencial** (para `lib.rs`/`mod.rs`): `"📦 módulos: [pub::pre_read, pub::post_read, hook_registry]"`

---

### Gap #6 — `scope_map.rs` ScopeMap ❌ NÃO INTEGRADO — Valor: **MÉDIO**

**Sinal potencial**: `"⚠ scope shadowing: runtime@12 shadows runtime@8"` — bugs silenciosos de shadowing.

---

### Gap #7 — `touring-simd` WilsonRanker + DriftDetector ❌ NÃO INTEGRADO — Valor: **MÉDIO**

**O que existe**:
- `WilsonRanker::new(0.95)` → Wilson confidence interval scoring
- `DriftDetector::ks_statistic(s1, s2)` → KS-test drift detection

**Problema atual**: gotchas com 1 hit e 50 hits recebem MESMO peso recency×2.0.

**Fix**: Usar `log(hit_count+1) * 0.2` como boost adicional ao score de gotchas.

---

## 3. Problemas Técnicos no Código Atual

### Problema #1 — Double syscall `fs::metadata` 🐛

```rust
// large_file_touring_signal() — linha 371
let line_est = std::fs::metadata(path).map(|m| m.len() / 60).unwrap_or(0); // syscall #1

// suggest_touring_for_code_file() — linha 398
let line_est = std::fs::metadata(path).map(|m| m.len() / 60).unwrap_or(0); // syscall #2 — REDUNDANTE!
```

**Fix**: Computar `line_est` UMA VEZ em `compose_high_signal_context_budgeted`, passar como parâmetro.

---

### Problema #2 — Score estático de dependentes

```rust
// linha 284 — score SEMPRE 1.0
scored_signals.push((1.0, format!("{} files import this: [{}]", ...)));
```

**Fix**: `score = 1.0 + (count as f32).ln().max(0.0) * 0.3`

---

### Problema #3 — Código duplicado em `recency_score_from_str`

Lógica `days → recency` duplicada nos 2 ramos do if-let chain.

**Fix**: Extrair `fn days_since_naive(created: NaiveDateTime) -> f32`.

---

### Problema #4 — Sem paralelismo no gathering de sinais

Pipeline sequencial: DB → symbol_map → cognitive. Rayon poderia paralelizar.
*(Sprint 3 — requer auditoria Send+Sync do runtime)*

---

## 4. Estratégia de Potencialização — 3 Sprints

### Sprint 1 — Quick Wins (Alta ROI, Baixa Complexidade) ⚡

| ID | Feature | ROI | Complexidade |
|----|---------|-----|-------------|
| S1.1 | Fix double `fs::metadata` syscall | Alto | 5 linhas |
| S1.2 | Escala logarítmica do score de dependentes | Alto | 1 linha |
| S1.3 | DRY `recency_score_from_str` | Médio | 5 linhas |
| S1.4 | Sinal de pub re-exports via `ModuleTree` | Alto | ~25 linhas |
| S1.5 | Sinal de hierarquia de módulos (lib.rs/mod.rs) | Médio | ~20 linhas |

**Implementação Sprint 1**: apenas `pre_read.rs`, sem dependências externas novas.
`touring_ast::{ModuleTree, extract_imports_resolved}` já são exports públicos.

---

### Sprint 2 — BlastRadius + External Callers (Médio-Alta ROI)

| ID | Feature | ROI | Complexidade |
|----|---------|-----|-------------|
| S2.1 | Sinal de BlastRadius enriquecido via `SymbolIndex` | Alto | ~30 linhas |
| S2.2 | External callers via `SymbolIndex.symbols` | Alto | ~25 linhas |
| S2.3 | Hit-count boost para gotchas (Wilson proxy) | Médio | 3 linhas |

**Implementação Sprint 2**: apenas `pre_read.rs`, acessa `runtime.infra.symbol_index.as_ref()`.
`SymbolIndex` já está no `infra` do `HookRuntime` — sem mudanças no `runtime.rs`.

---

### Sprint 3 — Semantic + Paralelismo (Médio ROI, Alta Complexidade)

| ID | Feature | ROI | Complexidade |
|----|---------|-----|-------------|
| S3.1 | SemanticSymbolIndex cross-file navigation | Médio | Requer runtime.rs |
| S3.2 | DriftDetector para freshness de gotchas | Médio | Requer série temporal na DB |
| S3.3 | Rayon parallelization | Médio-Alto | Requer Send+Sync audit |
| S3.4 | ScopeMap shadowing warnings | Baixo-Médio | ~20 linhas |

---

## 5. Análise de Budget de Performance

| Feature | Latência estimada | Total acumulado |
|---------|-------------------|-----------------|
| Baseline (atual) | ~2–4ms | 2–4ms |
| S1.1 fix syscall | -0.5ms | 1.5–3.5ms |
| S1.4 ModuleTree (fs::read + tree-sitter) | +0.5–1ms (rs files) | 2.0–4.5ms |
| S2.1 BlastRadius (in-memory BFS) | +0.3–0.8ms | 2.3–5.3ms |
| S2.2 External callers (in-memory scan) | +0.2–0.5ms | 2.5–5.8ms |
| S3.3 Rayon parallel | -1.5ms | 1.0–4.3ms |

**Conclusão**: Todos os sprints mantêm latência confortavelmente abaixo de 10ms.

---

## 6. Exemplo Visual — Before vs After

### Sinal atual (pre_read.rs hoje):
```
📌 defs[5]: run_returning(62)·compose_high_signal_context_budgeted(230)·...
3 files import this: [hook_registry.rs, daemon.rs, cli_handlers.rs]
⚠ Risk: 40%
```

### Sinal potencializado (após 3 sprints):
```
📌 defs[5]: run_returning(62)·compose_high_signal_context_budgeted(230)·...
⚡ blast(dist=3, 8 files); diretos: [daemon.rs, cli.rs]
⚡ pub re-exports[3]: HookResponse, run, DEFAULT_CONTEXT_BUDGET — break = API break
📞 callers externo: run_returning(3↑)·compose_signal(5↑)
⚠ GOTCHA [high] (hits=12): recency_score_from_str fails on ISO-8601+tz
⚠ Risk: 40% | 🔮 Next: Edit (80%)
```

O Claude ao receber esse contexto **antes** de ler o arquivo:
1. Sabe que qualquer mudança afeta 8 arquivos (incluindo 5 transitivos)
2. Sabe que 3 símbolos são re-exportados (API break risk)
3. Sabe que `run_returning` é referenciada em 3 outros arquivos
4. Tem o gotcha mais confiável (12 hits) sobre timestamp parsing

---

## 7. Mapa de Arquivos Modificados

| Arquivo | Sprint | Mudanças |
|---------|--------|----------|
| `pre_read.rs` | S1+S2+S3 | Todos os sprints |
| `runtime.rs` | S3.1 | Expor SemanticSymbolIndex (opcional) |
| `knowledge.rs` | S3.2 | Série temporal de outcomes (opcional) |
| `Cargo.toml` (touring-hooks) | — | Nenhuma mudança (touring-simd já é dep) |

---

## 8. Ordem de Implementação

```
Sprint 1 (puro refactor + ModuleTree):
  S1.1 → S1.2 → S1.3 → S1.4 → S1.5

Sprint 2 (usa SymbolIndex já no runtime):
  S2.1 → S2.2 → S2.3

Sprint 3 (mudanças maiores):
  S3.4 → S3.3 → S3.1 → S3.2
```

---

*Análise gerada em 28/03/2026 — TACO v5.1 · Touring v28.12.0 · 3.851 testes*
