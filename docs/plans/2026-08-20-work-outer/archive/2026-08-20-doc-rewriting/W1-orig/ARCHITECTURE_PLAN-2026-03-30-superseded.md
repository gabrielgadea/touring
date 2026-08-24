# Plano: Touring Memory & Database Architecture — Implementação Completa

> Criado: 2026-03-30 | Atualizado: 2026-03-30 | Autor: TACO Orchestrator v5.1
> Baseado na análise profunda de 13 crates e 7 DBs SQLite

---

## Contexto

O ecossistema Touring possui **13 crates** com **7 bancos de dados SQLite isolados** (fragmentation score = 7/10).

### Problemas Identificados (Red Flags)

| # | Problema | Impacto |
|---|----------|---------|
| 1 | AnnMemoryRecall é puramente in-memory | Embeddings perdidos a cada restart |
| 2 | 7 DBs SQLite isolados, zero transações cross-DB | JOINs impossíveis, consistência quebrada |
| 3 | rkyv files sem integridade | Corrompido = falha silenciosa |
| 4 | RecallCache invalida tudo em cada mutation | Cache hit rate ≈ 0 |

### Lição Aprendida (2026-03-30)

**Tentativa de PersistedAnnMemoryRecall falhou** por razões arquiteturais:
1. `AnnMemoryRecall` não implementa `Clone` — necessário para write-through
2. API rkyv requer `Archived<Vec<f32>>` não `Vec<f32>` direto
3. `deadpool-sqlite` não pode ser optional se outros módulos usam sem feature gate
4. Feature gate em Error enum quebra impl From internals

**Abordagem correta**: Adicionar `Clone` primeiro, usar serialização simples (bincode/JSON), fazer persistência como módulo separado.

---

## Arquitetura Alvo

```
L1-HOT (In-Process)
  AnnMemoryRecall + AsyncRlmMemory + RecallCache (DashMap)
         │
         ▼ mpsc channel
L2-WARM (SQLite WAL)
  RLM + SemanticRecall + Symbols + Knowledge
         │
         ▼
L3-COLD (File-based)
  GraphSnapshot (MessagePack) + rkyv (LinUCB/QTable)
```

---

## Fases de Implementação (REVISED)

### PHASE 1: Fundação (Esforço: M)
**Impacto**: 10x query cache + persistent ANN + write-behind cache

| # | Item | Arquivo | Esforço | Status |
|---|------|---------|---------|--------|
| 1.0 | Adicionar `Clone` a AnnMemoryRecall | touring-hooks/src/ann_memory.rs | 1h | ✅ CONCLUÍDO |
| 1.1 | PersistedAnnMemoryRecall (bincode+SQLite) | touring-hooks/src/ann_memory/persistence.rs | 8h | ✅ CONCLUÍDO |
| 1.2 | RecallCache v2 (per-key TTL) | touring-learning/src/recall_cache.rs | 4h | PENDENTE |
| 1.3 | Write-Behind Batch FSYNC | touring-learning/src/async_rlm.rs | 4h | PENDENTE |

### PHASE 2: Consolidação (Esforço: L)
**Impacto**: 3 DBs lógicos + ACID cross-namespace + backup/restore

| # | Item | Arquivo | Esforço |
|---|------|---------|---------|
| 2.1 | Unificação knowledge.db | touring-hooks/src/knowledge.rs | 8h |
| 2.2 | Unificação memory.db | touring-learning/src/memory.rs | 8h |
| 2.3 | Unificação graph.db | touring-cognitive/src/graph_store.rs | 4h |
| 2.4 | Connection Pool Manager | touring-core/src/pool.rs (NOVO) | 4h |

### PHASE 3: Exponencial (Esforço: XL)
**Impacto**: 100x faster graph ops + 8x memory reduction + hybrid search

| # | Item | Arquivo | Esforço |
|---|------|---------|---------|
| 3.1 | Quantization u4 para Embeddings | touring-simd/src/quantized_index.rs | 8h |
| 3.2 | ANN Index Rebuild from SQLite | touring-hooks/src/ann_memory.rs | 4h |
| 3.3 | Hybrid Search FTS5 + ANN RRF | touring-learning/src/semantic_recall.rs | 8h |
| 3.4 | Tier Promotion/Demotion Scheduler | touring-learning/src/tier_manager.rs (NOVO) | 4h |
| 3.5 | Circuit Breaker per Domain | touring-core/src/circuit.rs | 4h |
| 3.6 | Backup/Restore CLI | touring-cli/src/backup.rs (NOVO) | 4h |

---

## Critério de Sucesso

| Métrica | Antes | Depois (Meta) |
|---------|-------|---------------|
| Semantic search latency | 50-100ms | 1-5ms |
| Session warm-start | 0% | 100% |
| Query cache hit rate | 0% | 80%+ |
| Embedding memory | 4 bytes/símbolo | 0.5 bytes/símbolo |
| DBs isoladas | 7 | 3 |
| Graph serialization | JSON | MessagePack+rkyv |

---

## Validação

```bash
cargo check --workspace 2>&1 | grep "^error" | wc -l  # deve ser 0
cargo clippy --workspace -- -D warnings 2>&1 | grep "^error" | wc -l  # deve ser 0
cargo test --workspace --exclude touring-python 2>&1 | tail -1  # deve ter 4570+ passing
```
