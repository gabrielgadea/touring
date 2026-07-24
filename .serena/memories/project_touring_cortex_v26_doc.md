# touring-cortex v26 Documentation — Lessons Learned

## Contexto
Documentação completa de arquitetura do crate `touring-cortex` (v26.0) — 27 arquivos, ~13.500 LOC, 81 handlers, 511+ testes.

## Arquitetura Core

- **Pipeline execute()**: sort by priority → partition sync/async → E4-S2 tier check → E4-S3 budget check → execute → Block para imediatamente → async via rayon
- **FilterCache**: `RwLock<LruCache<FilterCacheKey, Vec<usize>>>` com read-through `peek()` (não atualiza LRU em read) — bug: sem evict counting
- **E1-S2 deduplicação**: `HashSet<u64>` via `DefaultHasher` — mesmo handler mesmo evento = mesmo hash — economiza 10-20% token budget
- **E4-S2 Dependency Tiers**: T0 (sem deps) → T1 (knowledge DB) → T2 (RLM/Persistence) → T3 (SemanticRecall) — graceful degradation

## Performance (E2)

- **E2-S2 rrf_parallel**: `fold(HashMap::new)` + `reduce(HashMap::new, merge)` — zero DashMap contention → 2-5x throughput improvement vs DashMap shard-lock
- **E2-S5 CallGraph SCC_VERSION**: `AtomicUsize version` por instância — mutações em cg1 NÃO invalidam cache de cg2
- **E3-S4 critical_path**: DP sobre ordem topológica para maior cadeia de chamadas em call graphs

## Handlers (81 = H1-H82)

- **H60-H69** (enrichment): migrados de Python project hooks v10.6 — 10 handlers
- **H75-H76** (rules): touring-rules integration via `OnceLock<Option<RulesEngine>>`
- **H51 CodeStandardsEnforcer**: diff-based ruff lint — blocks apenas NOVAS violações (penalty>2.0), não pré-existentes
- **ShadowLintGate (H39)**: explicitamente REMOVIDO — superseded por CodeStandardsEnforcer
- **DSPy bridge**: fail-open via `OnceLock<bool>` check de `python3 -c "import dspy"`

## 7 Gaps Documentados em ARCHITECTURE.md

1. **Gap 1 (P1)**: CircuitBreakerRegistry definido em circuit_breaker.rs mas NÃO integrado em Pipeline::execute()
2. **Gap 2**: Dynamic handler registration (feature flag) — `register_all_filtered` existe mas não é chamado
3. **Gap 3**: FileChanged handler (H82) registrado em lifecycle.rs mas evento não documentado em types.rs HookEvent
4. **Gap 4**: RLM schema drift — F401-F403 em HookEvent mas persistence.rs não valida version
5. **Gap 5 (P2)**: DSPy fail-open — quality bridge não integrada no output do pipeline
6. **Gap 6 (P3)**: FilterCache capacity fixa (1000) sem auto-tune
7. **Gap 7 (P3)**: BM25 avg_doc_len estimado (150) sem métricas reais

## Padrões Importantes

- **Exit 0 sempre**: `runtime.rs` — fallback standalone se daemon indisponível
- **IncrementalPipeline**: `std::sync::Mutex` guard (não tokio) — mesmo em contexto async
- **CoEditTracker**: `HashMap<(String, String), u32>` com chaves ordenadas (a < b) para simetria
- **RL reward**: base(±0.3/0.5) + tool_bonus + rework_penalty + first_try_bonus + sequential_failure_penalty

## Como Documentar Crates Touring (Template)

1. lib.rs → ler todos os `pub mod` + `pub use` exports
2. handlers/mod.rs → BUILTIN_HANDLER_COUNT + `register_all()` order
3. Cada handler module → ler o suficiente para LOC/CC/eventos
4. Módulos core (pipeline, runtime, fusion, call_graph) → arquitetura + patterns
5. GAP analysis → identificar código definido mas não usado (ex: circuit_breaker.rs)
6. E2 strategies → procurar padrões de otimização (DashMap→HashMap, rayon, caching)
7. cargo check → validar compilação
