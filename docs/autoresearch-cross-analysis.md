# Análise Cruzada: autoresearch × touring-rust

> **Data**: 2026-03-22 | **Autor**: TACO Orchestrator (claude_code)
> **Fontes**: github.com/karpathy/autoresearch (49K+ stars) × touring-rust workspace (60.2K LOC, 10 crates, v10.8.0+)
> **Context7**: tokio 1.49, moka 0.12, petgraph latest — best practices verificadas

---

## 1. O Que É o autoresearch

**NÃO** é um sistema de busca de papers acadêmicos. É um **runner autônomo de experimentos ML**: um agente AI (Claude/Codex) modifica código de treino, executa, avalia com uma métrica escalar (`val_bpb`), e decide keep/discard via git. ~630 linhas, 3 arquivos, zero frameworks.

### Números
- 126+ experimentos → melhoria de 0.9979 → 0.9697 val_bpb
- Shopify adaptou: 19% ganho de performance, 53% rendering mais rápido
- SkyPilot escalou: 16 GPUs, 910 experimentos em 8h, 9x speedup
- $260 custo total GPU para 910 experimentos

### Arquitetura (3 arquivos)

| Arquivo | Papel | Mutável? |
|---------|-------|----------|
| `prepare.py` (~400 LOC) | Trust boundary: dados, tokenizer, `evaluate_bpb()`, constantes (`TIME_BUDGET=300s`) | **NÃO** — agente proibido de editar |
| `train.py` (~600 LOC) | "Genoma": modelo GPT, optimizer MuonAdamW, hiperparâmetros, loop de treino | **SIM** — único arquivo editável |
| `program.md` (~200 LOC) | "Agent program": instruções em linguagem natural que governam o agente | Pelo humano apenas |

### Loop Principal
```
1. Analisar código + git history
2. Formar hipótese → editar train.py
3. Git commit (descritivo)
4. uv run train.py > run.log 2>&1
5. grep '^val_bpb:' run.log → extrair métrica
6. val_bpb melhorou? → KEEP (branch avança) : DISCARD (git reset --hard HEAD~1)
7. Log em results.tsv
8. REPEAT forever (NEVER STOP)
```

---

## 2. touring-rust: Estado Atual

### Workspace (10 crates)

| Crate | LOC est. | Papel |
|-------|----------|-------|
| `touring-core` | ~1.5K | Foundation: types, config, error, MemoryTier, CILALevel |
| `touring-simd` | ~3K | SIMD ops: cosine, Jaccard, Wilson, drift, financial |
| `touring-learning` | ~12K | RL brain: QTable TD(λ), LinUCB, OnlineRL, RLM memory, ESAA, evolution |
| `touring-hooks` | ~8K | Claude Code hooks: knowledge DB, classifier, error predictor, shadow workspace |
| `touring-ast` | ~6K | Code intelligence: tree-sitter, incremental parsing, blast radius, surgery |
| `touring-nlp` | ~5K | NLP: BM25 search, chunking, reranking, keyword matching, monetary parsing |
| `touring-cognitive` | ~6K | Reasoning: MCTS, GoT, semantic graph, co-edit predictor, focus cache |
| `touring-server` | ~21K | MCP server + cortex: 73 handlers, 26 tools, pipeline, context compiler |
| `touring-python` | ~2K | PyO3 bindings: ACO, NLP, SIMD exports |
| `touring-rules` | ~1K | Business rules: zen-engine JDM decision tables |

### Forças Sistêmicas
1. **Feedback loop** post-hook → learn → pre-hook (crown jewel)
2. **Dual-mode binary** MCP server + hook accelerator
3. **Quality discipline** clippy deny-all, 0 unwrap(), 0 TODO/FIXME
4. **Proactive error prevention** gotchas + Markov predictor
5. **Self-improving templates** UCB1 evolution

### Gaps Sistêmicos
1. **MemoryTier divergência** — 2 enums incompatíveis (core:5 vs rlm:4)
2. **MCTS desconectado do QTable** — não compartilham knowledge
3. **Server monolith** — 21K LOC, 73 handlers, depende de TODOS crates
4. **Shadow workspace só Python** — ruff ok, mas sem clippy/tsc
5. **file_risk sempre None** — feature slot desperdiçado no RL
6. **No end-to-end integration test** — loop completo nunca testado
7. **Python bridge thin** — só ACO/NLP/SIMD, sem AST/memory/cognitive

---

## 3. Os 12 Padrões do autoresearch

### P1. Immutable Trust Boundary
**autoresearch**: `prepare.py` imutável, agente só edita `train.py`.
**touring**: GoalTracker está em `touring-learning::aco::tracker` — mesmo crate que gera código.
**Lição**: Sealed traits em crate separado para evaluation functions.

### P2. Git as Transaction Log
**autoresearch**: Commit → evaluate → keep/rollback. Branch avança monotonicamente.
**touring**: `ShadowWorkspaceV2` faz speculative edits, mas QTable/LinUCB sem rollback atômico.
**Lição**: WAL (Write-Ahead Log) para RL state com commit/rollback transacional.

### P3. Fixed-Budget Time Boxing
**autoresearch**: `TIME_BUDGET=300s`, warmup excluído, >10min → kill.
**touring**: Handlers sem timeout enforcement. Handler lento bloqueia pipeline.
**Lição**: `TimeBudget` struct com `tokio::time::timeout` por handler.
**Context7 (tokio)**: `timeout(Duration::from_millis(15), handler.execute()).await` — cancela automaticamente.

### P4. Single Scalar Metric
**autoresearch**: `val_bpb` — uma métrica governa tudo.
**touring**: `RewardBreakdown` 5-dim + GoalTracker 81 checks. Complexidade funcional mas arriscada.
**Lição**: Composite sempre reduz a escalar `Ord`-implementado. Decisão keep/discard é binária.

### P5. Natural Language as Agent Program
**autoresearch**: `program.md` — instruções em markdown que o agente segue.
**touring**: 3 JDM tables em `touring-rules`. Maioria da lógica hardcoded nos handlers.
**Lição**: Migrar thresholds e decision rules para JDM tables declarativos.

### P6. Output Redirection + Selective Extraction
**autoresearch**: `> run.log 2>&1` → `grep '^val_bpb:'` — output nunca no contexto do agente.
**touring**: `ObservationMasker` filtra APÓS entrada. Não é preventivo.
**Lição**: `OutputCapture` struct — retorna só metrics + summary ao agente.

### P7. Fail-Fast Detection
**autoresearch**: NaN ou loss > 100 → `exit(1)` imediato.
**touring**: `ErrorPredictor` Markov-based (mais avançado). Mas sem graduated severity.
**Lição**: `HealthStatus { Healthy, Warning, Critical, Fatal }` com ações escalonadas.

### P8. Simplicity as Constraint
**autoresearch**: "0.001 improvement + 20 hacky lines = discard."
**touring**: `touring-ast` computa complexity. Não integrado ao RL reward.
**Lição**: `simplicity_delta` no `RewardBreakdown` — penalizar complexity creep.

### P9. Deterministic Seed + Pinned Validation
**autoresearch**: `torch.manual_seed(42)`, validation shard fixo.
**touring**: Seeds implícitos em testes. Sem pinned evaluation set para RL.
**Lição**: Evaluation datasets fixos para validar RL policy changes.

### P10. Dual Audit Trail
**autoresearch**: Git history (keeps) + results.tsv (tudo).
**touring**: `MemoryStore` sem separação committed vs tentativas.
**Lição**: `ExperimentLog` append-only com filtro committed/all.

### P11. Experiment Deduplication
**autoresearch**: Não tem — gap reconhecido.
**touring**: Bloom filter no relay `anti_loop.py`. Não no RL engine.
**Lição**: Hash state+action → bloom filter → rejeitar duplicados no RL.

### P12. Agent-Agnostic Design
**autoresearch**: Standard Unix commands, zero custom tool definitions.
**touring**: MCP server com 26 tools, bem definidos. Design é bom mas acoplado.
**Lição**: Thin `agent-interface` layer que expõe capabilities como CLI.

---

## 4. Mapeamento de Gaps × Padrões

| Gap do touring | Padrão autoresearch que resolve | Prioridade |
|---------------|-------------------------------|------------|
| MemoryTier divergência (2 enums) | P1 Immutable Trust Boundary | **ALTA** |
| No handler timeout | P3 Fixed-Budget Time Boxing | **ALTA** |
| MCTS desconectado do QTable | P4 Single Scalar Metric (compose) | **ALTA** |
| QTable sem rollback | P2 Git as Transaction Log | **ALTA** |
| ObservationMasker reativo | P6 Output Selective Extraction | **ALTA** |
| Shadow workspace só Python | P7 Fail-Fast Detection | **MÉDIA** |
| file_risk always None | P8 Simplicity Constraint | **MÉDIA** |
| No experiment log | P10 Dual Audit Trail | **MÉDIA** |
| 3 JDM tables apenas | P5 Natural Language Program | **MÉDIA** |
| No graduated severity | P7 Fail-Fast Detection | **MÉDIA** |
| No deduplication no RL | P11 Experiment Dedup | **MÉDIA** |
| Python bridge thin | P12 Agent-Agnostic Design | **BAIXA** |
| Server monolith 21K LOC | P12 Agent-Agnostic Design | **BAIXA** |
| No pinned eval set | P9 Deterministic Seed | **BAIXA** |

---

## 5. Context7 Best Practices Aplicáveis

### tokio 1.49 — Timeout + Cancellation
```rust
// Pattern: handler timeout wrapping
use tokio::time::{timeout, Duration};

let result = timeout(
    Duration::from_millis(budget.remaining_ms()),
    handler.execute(&mut ctx)
).await;

match result {
    Ok(Ok(decision)) => decision,
    Ok(Err(e)) => { log::warn!("Handler error: {e}"); Decision::Allow },
    Err(_elapsed) => { log::warn!("Handler timeout"); Decision::Allow },
}
```

### moka 0.12 — Cache com Per-Entry TTL + Eviction Listener
```rust
// Pattern: RL state cache com eviction audit
use moka::sync::Cache;

let rl_cache: Cache<StateAction, f64> = Cache::builder()
    .max_capacity(10_000)
    .time_to_idle(Duration::from_secs(7 * 24 * 3600)) // 7-day TTI
    .eviction_listener(|key, value, cause| {
        experiment_log.append(EvictionEvent { key, value, cause });
    })
    .build();
```

### petgraph — Topological Sort para DAG de Handlers
```rust
// Pattern: handler ordering via topo sort ao invés de registration order
use petgraph::algo::toposort;

let order = toposort(&handler_graph, None)
    .map_err(|cycle| format!("Handler dependency cycle: {:?}", cycle.node_id()))?;
```

---

## 6. Síntese: O Que o touring Pode Aprender

### A Lição Macro
> **Complexidade não é sofisticação.** O autoresearch resolve com 630 linhas o que muitos frameworks tentam com 60K+. O touring tem 60K LOC por razões legítimas (10 crates, multi-domínio), mas cada novo handler deveria passar pelo "autoresearch test": *"Isso poderia ser declarativo ao invés de código?"*

### O que o touring já faz MELHOR que o autoresearch
1. **Feedback loop** (post → learn → pre) — autoresearch é greedy hill-climbing puro
2. **RL pipeline** (QTable + LinUCB + forced exploration) — autoresearch não tem exploration
3. **Proactive error prevention** (Markov predictor) — autoresearch só detecta NaN/explosion
4. **Multi-language AST** (4 linguagens + surgery) — autoresearch opera em 1 arquivo Python
5. **Self-improving templates** (UCB1 evolution) — autoresearch não evolui suas próprias instruções

### O que o autoresearch faz que o touring DEVERIA incorporar
1. **Trust boundaries** reais (sealed traits, not convention)
2. **Transactional state** com rollback atômico (WAL/git2-rs)
3. **Time-boxed execution** como primitivo de runtime (tokio::time::timeout)
4. **Output capture** preventivo (nunca poluir contexto do LLM)
5. **Dual audit trail** (committed vs all events)
6. **Simplicity reward** (complexity delta no RL)
7. **Declarative rules** (expandir JDM tables)

---

*Documento gerado automaticamente pela análise TACO. Fonte JSON completa: `~/autoresearch-analysis.json`*
