# touring-hooks Pre-Hooks — Estratégias de Aperfeiçoamento Exponencial

> **Data**: 29/03/2026 | **Versão**: v1.0 | **Baseline**: v29.7.0 (4,096 testes workspace)
> **Método**: Scout (análise profunda 5 hooks, 4,770 LoC) × Researcher (context7: tower, rayon, moka, tracing, serde, parking_lot, tree-sitter)
> **Objetivo**: Elevar qualidade média de 8.2/10 para 9.5/10, eliminar inconsistências arquiteturais

---

## Executive Summary

Os 5 pre-hooks do touring-hooks são o **sistema nervoso** que enriquece cada ação do Claude Code.
Com 4,770 LoC, eles injetam contexto antes de Read, Edit, Write e Bash.

**Estado atual**: 3 hooks excelentes (pre_read 8.5, pre_write 8.5, pre_bash 9.0), 1 bom (pre_edit_prevention 8.0), e **1 ponto fraco crítico** (pre_edit 7.0).

**Problema central**: `pre_edit.rs` divergiu arquiteturalmente dos outros hooks — não usa signal scoring, não tem CILA budget, não usa rayon, faz 2 disk reads por invocação, e tem um god function com CC=37. É o hook que mais executa (a cada Edit) e o que menos evoluiu.

### Impacto Estimado

| # | Estratégia | Esforço | Impacto | Prioridade |
|---|-----------|---------|---------|------------|
| S1 | Unificar pre_edit com scored signals + CILA | L3 | Crítico | P0 |
| S2 | Shared Signal Library (deduplicate) | L3 | Alto | P1 |
| S3 | Eliminar disk I/O redundante em pre_edit | L1 | Alto | P0 |
| S4 | Cache ErrorPredictor em pre_edit | L1 | Médio | P0 |
| S5 | recv_timeout em pre_read (bug fix) | L1 | M��dio | P0 |
| S6 | Tower-style signal pipeline | L4 | Transformacional | P2 |
| S7 | moka cache para result_cache | L3 | Alto | P1 |
| S8 | Dedicated rayon ThreadPool | L2 | Médio | P2 |
| S9 | Score normalization [0,1] + RRF fusion | L2 | Médio | P2 |
| S10 | QueryCursor pooling + set_byte_range | L2 | Médio | P2 |

**Prioridade de execução**: S3 → S4 → S5 → S1 → S2 → S7 → S9 → S8 → S10 → S6

---

## Análise Por Módulo

### pre_read.rs (2,075 LoC) — Score: 8.5/10

**Forças**:
- Rayon parallel: `rayon::join` nested para blast+callers+similar+source signals simultaneamente
- Budget-aware assembly com early termination
- CILA tiers: L0-L1=800, L2-L3=2000, L4+=4000 chars
- Hit-count boost: `(ln(hit+1) * 0.2).min(0.5)` — matematicamente sólido
- Gotcha drift detection via KS test (`touring_simd::DriftDetector`)
- 1,100+ linhas de testes

**Fraquezas**:
- `source_rx.recv().unwrap_or_default()` — espera ilimitada se rayon worker panic (pre_write usa recv_timeout)
- `std::fs::metadata` síncrono no hot path
- `enrich_with_cognitive()` duplicado com pre_write

### pre_edit.rs (977 LoC) — Score: 7.0/10 ⚠️ PONTO FRACO

**Fraquezas Críticas**:
- **CC=37** em `compose_edit_context()` — god function com 12 branches de signals
- **Sem signal scoring** — usa `Vec<String>` flat (vs `Vec<(f32, String)>` em pre_read/pre_write)
- **Sem CILA budget** — contexto cresce sem limite
- **Sem rayon** — todos os signals sequenciais
- **2x disk reads** — `compose_quality_evolution()` e `compose_file_overview()` leem o mesmo arquivo
- **ErrorPredictor uncached** — `ErrorPredictor::new() + train_from_db()` a cada invocação (O(n))
- **90% duplicado** com pre_write `knowledge_signals()` mas com implementação inferior

### pre_edit_prevention.rs (492 LoC) — Score: 8.0/10

**Forças**: Bem calibrado (MIN_GOTCHA_HITS=2, COMPLEXITY_THRESHOLD=10, MAX_SHADOW_WARNINGS=3).
**Fraquezas**: Anti-patterns Rust-only (pre_write é multi-language). Sem CILA budget.

### pre_write.rs (866 LoC) — Score: 8.5/10

**Forças**:
- Scored signals `Vec<(f32, String)>` com CILA budget (1200/3000/6000)
- ErrorPredictor CACHED via `ContextRuntime`
- Rayon parallel com `recv_timeout(100ms)` — graceful degradation
- Multi-language anti-patterns (Rust, Python, TS/JS)

**Fraquezas**: `detect_language` local wrapper vs shared. Sort comparator duplicado.

### pre_bash.rs (360 LoC) — Score: 9.0/10

**Forças**: Filosofia "SILENCE IS THE DEFAULT". 3-tier relevance. Systemic failure detection (≥80% failure rate). Cleanest module.

---

## Inconsistências Arquiteturais (Cross-Cutting)

| Aspecto | pre_read | pre_edit | pre_edit_prevention | pre_write | pre_bash |
|---------|----------|----------|---------------------|-----------|----------|
| Signal scoring | `Vec<(f32, String)>` | `Vec<String>` ❌ | `Vec<String>` | `Vec<(f32, String)>` | Tiered |
| CILA budget | ✅ 800/2000/4000 | ❌ Nenhum | ❌ Nenhum | ✅ 1200/3000/6000 | N/A |
| Budget truncation | ✅ | ❌ | ❌ | ✅ | N/A |
| Rayon parallel | ✅ join+spawn | �� | �� | ✅ spawn | ❌ |
| ErrorPredictor | N/A | ❌ Uncached | N/A | ✅ Cached | N/A |
| Disk reads | 1 | ❌ 2 | 0 | 0 | 0 |
| recv timeout | ❌ unbounded | N/A | N/A | ✅ 100ms | N/A |

### Código Duplicado (5 instances)

1. `knowledge_signals()` — pre_write vs `compose_edit_context()` pre_edit (90% overlap)
2. `enrich_with_cognitive()` — pre_read + pre_write (id��ntico)
3. `blast_radius_signal()` — pre_read + pre_write (implementações diferentes)
4. Anti-pattern detection — 3 lugares (pre_edit, pre_edit_prevention, pre_write)
5. Sort comparator `partial_cmp().unwrap_or(Equal)` — 3x

---

## TOP 10 ISSUES

| # | Sev | Módulo | Issue | Impacto |
|---|-----|--------|-------|---------|
| 1 | P0 | pre_edit | CC=37 god function sem scoring, sem budget | Context injection ilimitada, inconsistente |
| 2 | P0 | pre_edit | 2x disk reads por invocação | Latência >10ms para arquivos grandes |
| 3 | P0 | pre_edit | ErrorPredictor uncached (O(n) retrain) | CPU waste a cada Edit |
| 4 | P0 | pre_read | `recv().unwrap_or_default()` sem timeout | Pode travar se rayon worker panic |
| 5 | P1 | cross | knowledge_signals duplicado (90% overlap) | Manutenção, comportamento divergente |
| 6 | P1 | cross | enrich_with_cognitive copy-paste | 2 cópias idênticas |
| 7 | P1 | cross | blast_radius_signal duplicado | Mesmo conceito, outputs diferentes |
| 8 | P2 | cross | Anti-patterns em 3 lugares | Rust-only vs multi-lang inconsistente |
| 9 | P2 | cross | Sort comparator repetido 3x | DRY violation |
| 10 | P3 | pre_edit_prev | Comentário duplicado L226-227 | Cosmético |

---

## Estratégias de Melhoria

### S1 — Unificar pre_edit com Scored Signals + CILA Budget (P0, L3)

**Problema**: `compose_edit_context()` tem CC=37, sem scoring, sem budget.

**Solução**: Decompor em signal functions individuais retornando `Vec<(f32, String)>`:

```rust
fn compose_edit_context(
    rt: &HookRuntime,
    file_path: &str,
    old_string: &str,
    new_string: &str,
    source: &str,  // lido UMA VEZ, passado por ref
) -> String {
    let cila = rt.ctx.session_cila_level();
    let budget = match cila {
        0..=1 => 1200,
        2..=3 => 3000,
        _ => 6000,
    };

    let mut signals: Vec<(f32, String)> = Vec::with_capacity(16);

    // Signal functions — cada uma retorna Option<(f32, String)>
    if let Some(s) = blast_radius_signal(&rt.infra, file_path) { signals.push(s); }
    if let Some(s) = quality_failures_signal(&rt.ctx.knowledge, file_path) { signals.push(s); }
    if let Some(s) = gotcha_signal(&rt.ctx.knowledge, file_path) { signals.push(s); }
    if let Some(s) = wiring_signal(&rt.ctx.knowledge, file_path) { signals.push(s); }
    if let Some(s) = risk_signal(&rt.ctx.knowledge, file_path) { signals.push(s); }
    if let Some(s) = speculate_signal(new_string, file_path) { signals.push(s); }
    if let Some(s) = antipattern_signal(new_string, file_path) { signals.push(s); }
    if let Some(s) = scope_shadow_signal(new_string, file_path) { signals.push(s); }
    if let Some(s) = complexity_signal(new_string, file_path) { signals.push(s); }

    // Rayon parallel: expensive signals
    let (import_s, cognitive_s) = rayon::join(
        || import_prediction_signal(&rt.ctx.knowledge, new_string, file_path),
        || cognitive_signal(&rt.ctx.cognitive, file_path),
    );
    if let Some(s) = import_s { signals.push(s); }
    if let Some(s) = cognitive_s { signals.push(s); }

    // Sort + budget truncate (shared utility)
    assemble_scored_context(&mut signals, budget)
}
```

### S2 — Shared Signal Library (P1, L3)

Criar `src/shared/signals.rs` com implementações únicas:

```rust
// shared/signals.rs

/// Blast radius signal — single implementation for all pre-hooks.
pub fn blast_radius_signal(idx: &SymbolIndex, file_path: &str) -> Option<(f32, String)> { ... }

/// Cognitive enrichment — single implementation.
pub fn enrich_with_cognitive(cognitive: &CognitiveState, file_path: &str) -> Vec<(f32, String)> { ... }

/// Knowledge-based signals (gotchas, notes, failures, deps).
pub fn knowledge_signals(db: &FileKnowledgeDB, file_path: &str) -> Vec<(f32, String)> { ... }

/// Multi-language anti-pattern detection.
pub fn antipattern_signals(content: &str, file_path: &str) -> Vec<(f32, String)> { ... }

/// Scored context assembly with budget truncation.
pub fn assemble_scored_context(signals: &mut Vec<(f32, String)>, budget: usize) -> String { ... }

/// Score comparison utility (NaN-safe).
pub fn score_cmp(a: &(f32, String), b: &(f32, String)) -> std::cmp::Ordering {
    b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
}
```

**Impacto**: ~300 linhas de deduplicação, single source of truth para pesos de signals.

### S3 — Eliminar Disk I/O Redundante em pre_edit (P0, L1)

**Problema**: `compose_quality_evolution()` e `compose_file_overview()` ambos leem o arquivo do disco.

**Solução**: Ler UMA VEZ em `run_returning()`, passar `&str` por referência:

```rust
// run_returning() — ler uma vez
let source = std::fs::read_to_string(file_path).unwrap_or_default();

// Passar por ref a todas as signal functions
compose_quality_evolution(rt, file_path, &source);
compose_file_overview(rt, file_path, &source);
```

### S4 — Cache ErrorPredictor em pre_edit (P0, L1)

**Problema**: `ErrorPredictor::new() + train_from_db()` a cada invocação — O(n).

**Solução**: Usar o cached predictor do `ContextRuntime` (como pre_write já faz):

```rust
// ANTES (pre_edit)
let predictor = ErrorPredictor::new();
predictor.train_from_db(&rt.ctx.knowledge);

// DEPOIS (como pre_write L95-98)
let predictor = &rt.ctx.error_predictor;
```

### S5 — recv_timeout em pre_read (P0, L1)

**Problema**: `source_rx.recv().unwrap_or_default()` pode esperar infinitamente.

**Solução**:
```rust
// ANTES
let source_signals = source_rx.recv().unwrap_or_default();

// DEPOIS (como pre_write L132-136)
let source_signals = source_rx.recv_timeout(std::time::Duration::from_millis(100))
    .unwrap_or_default();
```

### S6 — Tower-style Signal Pipeline (P2, L4)

**Problema**: Signals são acumulados ad-hoc em Vec. Difícil testar, compor, e reutilizar.

**Solução**: Cada signal como um Layer que envolve o inner:

```rust
trait SignalLayer {
    fn enrich(&self, ctx: &mut SignalContext) -> Vec<(f32, String)>;
}

struct SignalPipeline {
    layers: Vec<Box<dyn SignalLayer>>,
    budget: usize,
}

impl SignalPipeline {
    fn execute(&self, ctx: &mut SignalContext) -> String {
        let mut signals = Vec::new();
        for layer in &self.layers {
            signals.extend(layer.enrich(ctx));
        }
        assemble_scored_context(&mut signals, self.budget)
    }
}
```

**Benefícios**: Testabilidade individual por layer, composição declarativa, fácil adicionar/remover signals.

### S7 — moka Cache para result_cache (P1, L3)

**Problema**: `result_cache` usa HashMap simples sem TTL, sem eviction, sem size limits.

**Solução**: Migrar para moka:

```rust
use moka::sync::Cache;

let result_cache: Cache<String, String> = Cache::builder()
    .max_capacity(10_000)
    .time_to_live(Duration::from_secs(300))
    .time_to_idle(Duration::from_secs(60))
    .weigher(|_k, v: &String| v.len() as u32)
    .build();
```

**Elimina**: Manual `prewarm_result_cache()`, `file-changed` invalidation, unbounded memory growth.

### S8 — Dedicated rayon ThreadPool (P2, L2)

**Problema**: Hooks compartilham o rayon global pool com o tokio runtime do daemon.

**Solução**:
```rust
lazy_static! {
    static ref HOOK_POOL: rayon::ThreadPool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus::get().min(4))
        .thread_name(|i| format!("hook-worker-{}", i))
        .build()
        .expect("failed to create hook thread pool");
}

// Usar em pre_read/pre_edit/pre_write:
HOOK_POOL.install(|| {
    rayon::join(signal_a, signal_b)
});
```

### S9 ��� Score Normalization [0,1] + RRF Fusion (P2, L2)

**Problema**: Scores de signals têm escalas diferentes (blast_radius=int, complexity=float).

**Solução**: Normalizar antes de sorting:

```rust
fn normalize_signals(signals: &mut Vec<(f32, String)>) {
    if signals.is_empty() { return; }
    let max = signals.iter().map(|s| s.0).fold(f32::NEG_INFINITY, f32::max);
    let min = signals.iter().map(|s| s.0).fold(f32::INFINITY, f32::min);
    let range = max - min;
    if range > 0.0 {
        for s in signals.iter_mut() {
            s.0 = (s.0 - min) / range;
        }
    }
}
```

### S10 — QueryCursor Pooling + set_byte_range (P2, L2)

**Problema**: Novos QueryCursors criados a cada invocação. Sem range scoping.

**Solução**:
```rust
thread_local! {
    static CURSOR_POOL: RefCell<Vec<QueryCursor>> = RefCell::new(Vec::new());
}

fn with_cursor<F, R>(f: F) -> R
where F: FnOnce(&mut QueryCursor) -> R {
    let cursor = CURSOR_POOL.with(|pool| pool.borrow_mut().pop());
    let mut cursor = cursor.unwrap_or_default();
    let result = f(&mut cursor);
    CURSOR_POOL.with(|pool| pool.borrow_mut().push(cursor));
    result
}

// Usage with byte range:
with_cursor(|cursor| {
    cursor.set_byte_range(edit_start.saturating_sub(500)..edit_end + 500);
    cursor.set_match_limit(256);
    // ... execute query
});
```

---

## Roadmap de Execução

### Sprint 1 — Quick Fixes (1-2 horas)

| Task | Esforço | Arquivos |
|------|---------|----------|
| S3: Eliminar disk I/O redundante | L1 | pre_edit.rs |
| S4: Cache ErrorPredictor | L1 | pre_edit.rs |
| S5: recv_timeout em pre_read | L1 | pre_read.rs |
| Cosmético: remover comentário duplicado | L0 | pre_edit_prevention.rs |

### Sprint 2 — Shared Signal Library (3-5 horas)

| Task | Esforço | Arquivos |
|------|---------|----------|
| S2: Criar shared/signals.rs | L3 | shared/signals.rs (novo) |
| S2: Migrar blast_radius_signal | L2 | pre_read.rs, pre_write.rs |
| S2: Migrar enrich_with_cognitive | L2 | pre_read.rs, pre_write.rs |
| S2: Migrar antipattern_signals | L2 | pre_edit.rs, pre_edit_prevention.rs, pre_write.rs |
| S2: Shared score_cmp + assemble_scored_context | L1 | pre_read.rs, pre_write.rs |

### Sprint 3 — pre_edit Unification (4-6 horas)

| Task | Esforço | Arquivos |
|------|---------|----------|
| S1: Decompor compose_edit_context() | L3 | pre_edit.rs |
| S1: Adicionar signal scoring | L2 | pre_edit.rs |
| S1: Adicionar CILA budget | L2 | pre_edit.rs |
| S1: Adicionar rayon parallel | L2 | pre_edit.rs |

### Sprint 4 — Infrastructure (5-8 horas)

| Task | Esforço | Arquivos |
|------|---------|----------|
| S7: moka cache | L3 | runtime.rs |
| S8: Dedicated rayon ThreadPool | L2 | shared/ + pre_read + pre_edit + pre_write |
| S9: Score normalization | L2 | shared/signals.rs |
| S10: QueryCursor pooling | L2 | shared/ |

### Sprint 5 — Architecture (futuro)

| Task | Esforço | Arquivos |
|------|---------|----------|
| S6: Tower-style SignalPipeline | L4 | Novo módulo |

---

## Métricas de Sucesso

| Métrica | Baseline | Target |
|---------|----------|--------|
| Quality Score Médio | 8.2/10 | 9.5/10 |
| pre_edit score | 7.0/10 | 9.0/10 |
| CC max (compose_edit_context) | 37 | <15 |
| Disk reads em pre_edit | 2 | 0 (content from input) |
| Código duplicado | 5 instances | 0 |
| Signal scoring consistency | 2/5 hooks | 4/5 hooks |
| CILA budget coverage | 2/5 hooks | 4/5 hooks |
| Rayon coverage | 2/5 hooks | 3/5 hooks |
| recv_timeout coverage | 1/2 rayon hooks | 2/2 rayon hooks |

---

## Invariantes Pós-Implementação

```bash
# Nenhum hook com CC > 20
grep -rn 'compose_edit_context\|compose_pre_edit' src/pre_edit.rs | wc -l
# Esperado: 0 (decomposto em signal functions)

# Signal scoring em todos os hooks que injetam > 3 signals
grep -rn 'Vec<(f32, String)>' src/pre_edit.rs
# Esperado: presente

# recv_timeout em todo recv()
grep -rn '\.recv()' src/pre_read.rs src/pre_write.rs
# Esperado: 0 (todos usando recv_timeout)

# Zero disk reads redundantes
grep -rn 'std::fs::read_to_string' src/pre_edit.rs
# Esperado: 0 (content passed from caller)

# ErrorPredictor cached
grep -rn 'ErrorPredictor::new()' src/pre_edit.rs
# Esperado: 0 (usando cached from ContextRuntime)
```

---

*touring-hooks Pre-Hook Improvement Strategies v1.0 — Scout × Researcher synthesis*
*TACO Orchestrator output — 29/03/2026*
