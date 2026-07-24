# Plano Completo de Implementação — Oportunidades Touring v29.x

## Visão Geral

8 oportunidades identificadas, priorizadas e sequenciadas em 4 sprints atômicos. Entrega estimada: **~2 semanas** (assumindo 1 engineer dedicado, revisões paralelas).

---

## SPRINT 1 — Foundation (CircuitBreaker + Higienização)

### 1.1 CircuitBreaker Trait Unification — P1 HIGH

**Tarefa**: Unificar o trait `CircuitBreaker` em `touring-core/shared/circuit_breaker.rs` com as implementações duplicadas em `touring-cortex/pipeline.rs` (`HandlerBreaker`) e `touring-hooks/circuit_breaker.rs` (`OpClass`).

**Entregáveis**:
- [ ] `HandlerBreaker` implementa `CircuitBreaker` trait (mesmo comportamento, move o trait bound para o objeto)
- [ ] `OpClass` implementa `CircuitBreaker` trait (avaliar se `OpClass` é realmente um circuit breaker ou se é outro padrão)
- [ ] Trait default implementations para `is_open()`, `record_success()`, `record_failure()` com `#[inline]` onde aplicável
- [ ] Substituir todos os `unwrap()` nos métodos do trait por `should_skip()` pattern
- [ ] Testes: 3 novos testes em `touring-core` para o trait + testes de regressão em `touring-cortex` e `touring-hooks`

**T-shirt**: **M** (~4h)

**Dependências**: Nenhuma

**Riscos**:
- `OpClass` pode não ser um circuit breaker real → **Mitigação**: Auditoria prévia de `OpClass::record_success/failure` para confirmar semantics
- Quebrar `HandlerBreaker` existente → **Mitigação**: Testes de regressão em `cargo test -p touring-cortex` antes do commit

**Ordem de execução**:
1. Implementar trait em `HandlerBreaker` primeiro (menor risco, escopo fechado)
2. Auditar `OpClass` — se circuit breaker real, implementar trait; se não, documentar como pattern diferente
3. Cleanup dos `unwrap()` via `should_skip()`

---

### 1.2 Dead Commented Fusion Imports — P3 LOW

**Tarefa**: Remover imports comentados em `fusion.rs` ou descomentar se forem necessários.

**Entregáveis**:
- [ ] `fusion.rs` linha ~32: `use touring_cortex::fusion::{...}` — remover se dead, ou verificar se precisa de `pub use`
- [ ] `fusion.rs` linha ~71: mesmo tratamento
- [ ] Verificar se `fusion.rs` precisa de `pub use` para algum construtor que está sendo exportado por `lib.rs` mas não funciona externamente

**T-shirt**: **XS** (~30min)

**Dependências**: Nenhuma

---

## SPRINT 2 — MCP Surface Expansion

### 2.1 58 H-Handlers Não Expostos como MCP Tools — P1 HIGH

**Tarefa**: Avaliar quais handlers hacen sentido como MCP tools. 84 handlers registrados, ~26tools. Hands-on evaluation.

**Análise prévia (da iteração anterior)**:

| Handler | Potencial como MCP tool | Razão |
|---------|------------------------|-------|
| H84 StreamingMCTS | **ALTO** | `StreamingMCTSHarness` já existe, tokio watch channel — expõe MCTS como tool async |
| H90 DriftAudit | **ALTO** | `EvolutionAnalyzer` já tem drift detection — expor como `touring_evolution_drift` |
| H91 InsightEngine | **MÉDIO** | Já parcialmente exposto via `touring_evolution_insights` |
| H76 RulesHealthMonitor | **MÉDIO** | Health monitoring é MCP tool natural |
| H83 IntegrationCompleteness | **ALTO** | Wiring audit como MCP — `touring_wiring_audit` |

**Entregáveis**:

- [ ] **H84 como MCP tool** (`streaming_mcts`): Criar `StreamingMctsTool` em `touring-server/tools/` seguindo o padrão de `MctsTool`. Tool: `touring_streaming_mcts(search_state: string) -> StreamingMctsResult`. Usa tokio watch channel para output progressivo.

- [ ] **H90 como MCP tool** (`evolution_drift`): Já existe `EvolutionAnalyzer` em `touring-learning`. Criar `DriftTool` em `touring-server/tools/`. Tool: `touring_evolution_drift(metric_name?: string) -> DriftReport`. Ja feito parcialmente via `touring_insights` — avaliar se `drift` merece tool separada.

- [ ] **H83 como MCP tool** (`wiring_audit`): Handler `IntegrationCompletenessHandler` roda no cortex. Expor como `touring_wiring_audit` — retorna orphan count, integration scores, recommendations.

- [ ] **Decisão de não-exposição documentada**: Para os ~55 handlers restantes (internals, lifecycle hooks, enforcement), documentar razão (event-driven, não-MCP, etc.) em comentario no código.

**T-shirt**: **L** (~8h para H84 + H90 + H83 + documentação)

**Dependências**: Sprint 1 (CircuitBreaker) para evitar conflitos de merge

**Riscos**:
- H84 tokio runtime isolation dentro do server → **Mitigação**: Pattern `Handle::try_current()` já existe em `streaming_mcts.rs` — verificar que funciona no contexto MCP server
- MCTS tool pode ser heavy → **Mitigação**: Budget limits via CILA, timeout no tool definition

---

## SPRINT 3 — Feature Gates & Integration

### 3.1 inferlets-wasm Feature Documentation — P2 MEDIUM

**Tarefa**: Documentar como ativar `inferlets-wasm` ou decidir se deve ser default.

**Entregáveis**:
- [ ] Seção em `touring-system.md` ou `docs/` explicando: o que é InferletPool, como ativar (`inferlets-wasm` feature), quando usar (hot paths com baixa latência)
- [ ] Teste de integração: `cargo test -p touring-server --features inferlets-wasm` passa
- [ ] Se o feature é estável e beneficial, mover para default em `touring-server/Cargo.toml` (remover da lista de optional features)

**T-shirt**: **S** (~2h)

**Dependências**: Nenhuma

---

### 3.2 touring-antt NLP Enrichment Decision — P2 MEDIUM

**Tarefa**: Decidir se `nlp_enrichment` feature deve ser включен (enabled) no default bundle.

**Entregáveis**:
- [ ] Avaliação de overhead: benchmark com/sem `nlp_enrichment` (latência de pre-edit hook)
- [ ] Se overhead < 5ms e utilidade > threshold → habilitar por default
- [ ] Se overhead > 5ms ou utilidade baixa → documentar como opt-in e marcar como experimental
- [ ] Documentar em `touring-system.md`: o que o feature faz, quando usar, overhead esperado

**T-shirt**: **S** (~2h)

**Dependências**: Nenhuma

---

### 3.3 smart-cache Feature Gate Isolation — P2 MEDIUM

**Tarefa**: Verificar se `smart-cache` em `touring-index` (cache eviction via LinUCB bandit) funciona corretamente isolado de `touring-learning`.

**Entregáveis**:
- [ ] Teste: `cargo check -p touring-index --features smart-cache` compila sem `touring-learning`?
- [ ] Se não compila: corrigir feature gate para ter runtime fallback (cache eviction dummy quando learning não está disponível)
- [ ] Documentar o feature gate em `docs/` ou `touring-system.md`

**T-shirt**: **S** (~2h)

**Dependências**: Nenhuma

---

### 3.4 touring-server Deeper Cortex Integration — P2 MEDIUM

**Tarefa**: O server atualmente chama só `CortexRuntime::run()`. Pipeline, enrichment e cache_strategy existem mas não são usados.

**Entregáveis**:
- [ ] **Enrichment wiring**: `compose_enriched_context()` (presente em `touring-cortex`) — wire no server para enricher queries antes de retornar
- [ ] **CacheStrategy**: `StableSessionContext` + `VolatilePromptContext` — avaliar se o server deveria usar stratified context para queries recorrentes
- [ ] **Fusion**: `reciprocal_rank_fusion()` — usar para agregar resultados de múltiplas fontes (index + memory + session)
- [ ] **Code review**: Session com `cargo test -p touring-server` completo antes de cada entrega parcial

**T-shirt**: **L** (~6h)

**Dependências**: Sprint 1 (CircuitBreaker) — pode usar circuit breaker pattern para graceful degradation se enrichment falhar

---

### 3.5 touring-hooks FileKnowledgeDB Re-export — P3 LOW

**Tarefa**: `FileKnowledgeDB` é privado mas `KnowledgeRef` é público. Re-exportar se faz sentido para integrações externas.

**Entregáveis**:
- [ ] Avaliar se alguma integração externa precisa de `FileKnowledgeDB` diretamente (provavelmente não — `KnowledgeRef` é a interface pública)
- [ ] Se não precisa: adicionar comentario em `lib.rs` explicando que `KnowledgeRef` é a interface pública e `FileKnowledgeDB` é internal
- [ ] Se precisa: `pub use knowledge::FileKnowledgeDB` em `touring-hooks/lib.rs`

**T-shirt**: **XS** (~30min)

**Dependências**: Nenhuma

---

## SPRINT 4 — Polish & Documentation

### 4.1 Cross-Crate Audit Final

**Tarefa**: Verificar que todas as mudanças de Sprint 1-3 não quebraram wiring entre crates.

**Entregáveis**:
- [ ] `cargo check --workspace --exclude touring-python` → 0 errors
- [ ] `cargo clippy --workspace --exclude touring-python -- -D warnings` → 0 warnings
- [ ] `cargo test --workspace --exclude touring-python` → 4,096+ passed (regression check)
- [ ] `touring wiring status` → orphan count não aumentou
- [ ] Session de teste com todas as novas tools (H84, H90, H83)

---

## Resumo de Dependências e Timeline

```
Sprint 1 (Foundation)
├── 1.1 CircuitBreaker Unification    [M]
└── 1.2 Dead Fusion Imports          [XS]
    ↓
Sprint 2 (MCP Surface) — Depends on Sprint 1
└── 2.1 58 Handlers → MCP Tools     [L]
    ↓
Sprint 3 (Feature Gates) — No deps, parallel to Sprint 2
├── 3.1 inferlets-wasm docs         [S]
├── 3.2 NLP enrichment decision     [S]
├── 3.3 smart-cache isolation       [S]
├── 3.4 Server deeper cortex        [L]
└── 3.5 FileKnowledgeDB re-export   [XS]
    ↓
Sprint 4 (Polish)
└── 4.1 Cross-crate audit final     [S]
```

**Timeline estimada**:
- Sprint 1: Dia 1 (manhã)
- Sprint 2: Dia 1 (tarde) + Dia 2
- Sprint 3: Dia 2 + Dia 3 (paralelo ao Sprint 2 onde possível)
- Sprint 4: Dia 4

**Total: ~4 dias de engineer** (com revisões e testes)

---

## Riscos Consolidated

| Risco | Prob | Impact | Sprint | Mitigação |
|-------|------|--------|--------|-----------|
| `OpClass` não é CircuitBreaker real | Baixa | Média | 1 | Auditoria prévia antes de implementar |
| H84 runtime isolation falha no MCP | Média | Alta | 2 | Pattern já existe e foi testado em cortex |
| NLP enrichment overhead > 5ms | Baixa | Baixa | 3 | Opt-in com documentação se overhead alto |
| Breaking change no trait CircuitBreaker | Baixa | Alta | 1 | Testes de regressão em todos os crates |

---

## Critério de Done

Todas as sprint items marcadas ✓. Nenhuma regressão em:
- `cargo test --workspace` (4096+ tests)
- `cargo clippy --workspace -D warnings` (0 warnings)
- Wiring orphan count estável
- Todas as novas MCP tools respondem corretamente
