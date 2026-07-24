# Touring — Mapa de Capacidades & Acoplamento (o que e como acoplar)

> **Catálogo estrutural** das funcionalidades do Touring + a dimensão de **acoplamento** (a LLM usa?
> qual a barreira? como induzir?). Insumo para decidir **o que** acoplar e **como**.
> **Data**: 2026-06-26 | **Sessão**: `e0f553d0` | **Autor**: TACO (Opus 4.8 1M) p/ Gabriel Gadea
> **Método**: 3 exploradores paralelos (read-only) sobre os 48 crates + VGP. `[FACT]` = lido do código (file:line).
> **Companheiros**: `2026-06-26-harness-architecture-insights.md` (análise) · `…coupling-strategy.md` (estratégia).

---

## 0. A superfície de acoplamento (em números)

`[FACT]` **48 crates, 636.937 LOC.** ~50 capacidades expostas por **5 modos de acesso** — cada modo com
um *prior* diferente na LLM e, portanto, uma adesão diferente:

| Modo | Quantidade | Latência | Prior da LLM | Adesão atual | Papel ideal (pós-pesquisa) |
|---|---|---|---|---|---|
| **CLI** `touring <cmd>` | **114** handlers | <10ms (RPC) | médio (é "bash") | **média** — usa o que o hook sugere | **dominante** — é o que a LLM topa rodar; densificar saída |
| **MCP** `touring serve` | **171** tools | ~200ms | baixo (sem prior) | **baixa** (paradoxo de Gabriel) | **curar a ~15 + Code Mode** (não 171 atômicas) |
| **Inferlets** `touring inferlets run` | **17** WASM | ~WASM | ~zero | **~nula** | **Code Mode pronto** — induzir; é o PTC do harness doc |
| **Hooks** Pre/Post/Session | **416** eventos | <5ms | n/a (automático) | **alta** (injeção) | **enriquecimento de alto sinal** (cli-suggest/prompt-enhance) |
| **Rust API** | lib pub/crate | — | n/a | inter-crate | fundação interna |

> O desalinhamento é estrutural: a capacidade vive em CLI/MCP/Inferlet (baixa adesão), e o único canal de
> alta adesão (hooks) hoje gasta o orçamento empurrando texto, não reduzindo atrito (ver coupling-strategy).

---

## 1. Catálogo por subsistema `[FACT]`

### 1.1 Code Intelligence — `touring-intelligence` · `touring-code` · `touring-storage` · `touring-simd`
| Capacidade | O que faz | Acesso | Impl |
|---|---|---|---|
| Symbol index | Busca exata de símbolo (file-watch incremental + LRU) | CLI `touring index find` · MCP | `touring-intelligence/src/index/incremental.rs:122` |
| Hybrid search | BM25 (Tantivy) + vetor + rerank (Wilson + co-edit) | CLI `touring tantivy search/fuzzy` | `touring-storage/src/hybrid_search/mod.rs` |
| AST meta/blast | Complexidade + qualidade + **blast radius** (impacto de editar 1 símbolo) | CLI `touring ast meta/blast` -j · MCP | `touring-code/src/ast/graph/blast_radius.rs` |
| AST semantics | `syn`: generics, trait bounds, lifetimes, unsafe, async | CLI `touring ast rust-semantic` | `touring-code/src/ast/rust_semantic.rs` |
| Callgraph/TDG | Cadeias de chamada (Tarjan SCC) + grade A+..F | CLI `touring ast callgraph/tdg` | `touring-code/src/ast/{call_graph,quality}` |
| Polyglot SSR | Search-&-replace estrutural (ast-grep, **18+ linguagens**) | CLI `touring ssr apply`/`ast grep` · MCP | `touring-code/src/polyglot/{search,rewrite}` |
| Surgery | Substituição de corpo de símbolo com format-preserve (prettyplease) | CLI `touring ssr` / interno | `touring-code/src/ast/surgery.rs` |
| Wiring | orphans / impact(BFS) / cycles / chains / **purpose** | CLI `touring wiring <op>` | `touring-code/src/semantics` + `touring-storage/src/functional_wiring.rs` |
| Knowledge DB | Grafo SQLite: símbolos, imports, relations, access patterns | daemon-interno + hooks | `touring-storage/src/knowledge.rs` |
| SIMD | cosine/jaccard/top-K/fuzzy + quantização f16/u8 + HNSW ANN | interno (backend de search) | `touring-simd/src/{similarity,quantization,ann}` |

### 1.2 Quality & Generation — `touring-quality` · `touring-analysis` · `touring-generator` · `touring-lsp`
| Capacidade | O que faz | Acesso | Impl |
|---|---|---|---|
| 50-dim quality | Score F1.1–F4.12 + 6 gates BLOCK (OWASP, coverage…) | **binário** `touring-quality score/check` | `touring-quality/src/lib.rs:605` |
| Analysis pipeline | blast (BFS/HNSW) + orphan + churn + health (Wilson CI) | lib + `cli_e2e` | `touring-analysis/src/lib.rs:16` |
| Generator (VGP) | LLM→JSON→**VGP verify**→Tera render→speculate→commit atômico + RL | lib typestate + taco-forge | `touring-generator/src/lib.rs:12` (`VgpEngine`) |
| 29 templates | 28 generator kinds pré-compilados (Tera OnceLock) | interno | `touring-generator/template_engine.rs` |
| LSP | references/rename real + QualityDiagnostics→LSP severity | binário `touring-lsp` (feat lsp-bridge) | `touring-lsp/src/mapping.rs` |

### 1.3 Cognition & Reasoning — `touring-intelligence/reasoning` · `touring-server-reasoning` · `touring-cortex`
| Capacidade | O que faz | Acesso | Impl |
|---|---|---|---|
| **MCTS + GoT + ACO** | Monte-Carlo Tree Search + Graph-of-Thought + pheromone (UCB-augmented) | interno (reasoning) | `touring-intelligence/src/reasoning/cognitive_mcts.rs` |
| Session predictor | **Markov chain de tool-call** com decay (prevê próxima ferramenta) | interno (RL bridge) | `touring-intelligence/src/reasoning/session_predictor.rs:12` |
| Decomposer/CILA | DAG (Kahn topo-sort) + `CilaLevel L0-L5`→topology + TestGate/ClippyGate | CLI `touring decompose` | `touring-server-reasoning/src/reasoning/decomposer.rs:47,238` |
| **Cortex (81 handlers)** | Motor de hooks H1-H84 + **enrichment (RRF fusion, token-budget)** + call-graph + co-edit | hook pipeline (daemon) | `touring-cortex/src/{runtime,enrichment,call_graph}.rs` |
| Cache strategy | Estratificação prompt-cache (**StableSession** + **VolatilePrompt**) | interno (cortex) | `touring-cortex/src/cache_strategy.rs` |

### 1.4 Memory & Learning — `touring-intelligence/rl` · `touring-learning` · `touring-storage` · gotcha
| Capacidade | O que faz | Acesso | Impl |
|---|---|---|---|
| Memory 5-tier | Ephemeral/Working/Reference/Core; SQLite WAL + FTS5 + vetor | CLI `touring memory recall/store` | `touring-intelligence/src/rl/memory/rlm.rs:32,376` |
| RL flywheel | QTable+TD(λ), DoubleQ, PrioritizedReplay, Curiosity, **ACO**, OnlineRL (CUSUM drift) | CLI `touring learning reward` | `touring-intelligence/src/rl/{rl,evolution,online_rl}.rs` |
| Gotcha DB | Padrões-armadilha (regex) + hit_count + **F1 proxy** + decay | CLI `touring gotcha match/list` + hook | `touring-storage/src/knowledge/gotchas.rs:20` |
| Evolution | insights/drift/tools (ACO multi-objetivo) | CLI `touring evolution` | `touring-intelligence/src/rl/meta.rs` |

### 1.5 Prediction & Session — `touring-hooks-prediction` · `touring-server-session` · `touring-antt`
| Capacidade | O que faz | Acesso | Impl |
|---|---|---|---|
| Action-outcome ANN | PreToolUse learning: embeddings + índice quantizado (u4) de pares ação→resultado | hook | `touring-hooks-prediction/src/ann_memory/mod.rs:29` |
| Co-edit/heat predict | `predict_next()` de edições antecipadas por file-heat + co-edit | hook | `touring-hooks-prediction/src/layer7_prediction.rs:28` |
| LLM judge | Prediz recusa / violação de política (heurístico, LLM-free) | hook | `touring-hooks-prediction/src/llm_judge.rs` |
| Reranker (antt) | BM25 + semantic + authority + NDCG | interno (RAG) | `touring-intelligence/src/ann/reranker.rs` |
| Session/diary | SessionManager (lifecycle) + AgentDiary (Palace JSON) | CLI `touring session/diary` | `touring-server-session/src/session/manager.rs` |

### 1.6 Execution & Safety — `touring-ceg` · `touring-wasm` · `touring-hooks-saga` · `touring-resilience`
| Capacidade | O que faz | Acesso | Impl |
|---|---|---|---|
| **CEG X0-X9** | Pipeline typestate (capture→static→VGP→predict→sandbox→capability→decision→exec→learn) | hook (PreToolUse) | `touring-ceg/src/gateway/pre_exec.rs` |
| ctx_execute | **Code Mode**: roda código arbitrário (11 langs) sandbox + AST forbidden-call | **MCP** `touring_ctx_execute` | `touring-server/src/tools/ctx_execute_tools.rs:176` |
| Capability | 4 profiles deny-by-default + landlock + rlimit | interno (X6) | `touring-ceg/src/capability/builtins.rs` |
| WASM | runtime fuel/epoch metering (inferlets + plugins) | interno | `touring-wasm` (→ touring-bindings) |
| Saga 2PC | `DistributedSagaCoordinator` (prepare/execute/**compensate**) lock-free O(1) | interno (subagents) | `touring-hooks-saga/src/distributed.rs` |
| Resilience | Failover + circuit breaker (RECOVERY_THRESHOLD=3) | interno | `touring-resilience/src/failover` |

### 1.7 Visual & Web — `touring-server-visual` · `touring-web`
| Capacidade | O que faz | Acesso | Impl |
|---|---|---|---|
| Graph render | DOT/SVG/Mermaid/GraphML do blast/wiring (tier-colored) | CLI/interno | `touring-server-visual/src/visual/mod.rs:129,284` |

---

## 2. As capacidades de maior **valor de acoplamento** (o que a LLM deveria usar e não usa)

Ordenado por (valor para a tarefa × subutilização atual). Cada uma é uma capacidade que **vence o
bash/grep** mas que a LLM raramente alcança — com a **barreira** e o **vetor de indução**.

| Capacidade | Por que vence o bash | Barreira de adesão | Como induzir (pós-pesquisa) |
|---|---|---|---|
| **`ast blast` / `wiring impact`** | dá o raio de impacto real (não adivinha) antes de editar | saída pode ser grande; sem prior | **ACI**: saída densa default + hook injeta como redirect ao detectar Edit |
| **`index find` / `tantivy search`** | localização exata (WarpGrep: +3,7pp, −17% tok medido) | LLM dispara `grep` por reflexo | cli-suggest **redirect anti-atômico** com o número (alto sinal) |
| **`ctx_execute` + 17 inferlets (Code Mode)** | 1 script orquestra N comandos → sumário (CodeAct: −60% tok) | LLM não sabe que existe; sem prior | **search_tools** + induzir "escreva 1 script" no 2º+ comando repetido |
| **50-dim quality / gotcha match** | gate objetivo + armadilhas conhecidas vs adivinhação | binário separado; saída grande | hook injeta só os gates BLOCK relevantes ao arquivo |
| **MCTS / decomposer (CILA)** | planejamento estruturado vs improviso | totalmente interno; sem superfície LLM | expor `touring route` (RGAO) que devolve nível+plano |
| **memory recall / gotcha** | reusa lição passada vs re-descobrir | LLM não chama proativamente | hook recall no início + no erro repetido |

---

## 3. Como acoplar — a regra de mapeamento (síntese da pesquisa)

A pesquisa externa (ACI, CodeAct, Anthropic tool-writing, "Is Grep All You Need?") converge numa regra
de **qual modo acopla qual capacidade**:

1. **Metadados estruturados p/ o planner → MCP curado (~15).** `ast meta`, `wiring impact`, `index find`
   — alto sinal, estrutura limpa. Anthropic: *"fewer, high-impact tools"* + **namespacing** (`touring_*`).
2. **Trabalho em volume → Code Mode (ctx_execute + inferlets).** O Touring já tem o motor; falta
   `search_tools` (Anthropic Tool Search = **−85% tok**) + indução. **NÃO** expor 171 tools atômicas.
3. **Saída de CLI → ACI densa por default + `response_format`.** Anthropic: enum `concise/detailed`
   = **−⅔ contexto** (= o `--brief` de hoje). Generalizar a TODO comando `-j`. Truncar com **instrução**
   (não só cortar) e **sumário inline + metadata-first** (não só file-ref — senão "Codex pathology",
   "Is Grep All You Need?": file-based piorou 93→55% quando a LLM não relê).
4. **Indução → hooks de alto sinal, raros.** Tool-selection bias (BiasBusters): a LLM escolhe por
   **nome/descrição/ordem** superficiais → otimizar nomes/descrições dos comandos + injeção rara e
   acionável (não banner). Cortex `enrichment` (RRF + token-budget) já é o motor certo.
5. **Erros → mensagens acionáveis.** Anthropic + Class-D: erro guia o próximo passo; **preservar
   exit-code + assinatura** ao comprimir (não mascarar a falha).

> **Princípio único**: cada capacidade tem **um modo de menor atrito**. Hoje muitas estão presas no modo
> errado (interno/MCP-atômico). Acoplar = mover cada capacidade para seu modo de menor atrito (CLI-denso,
> Code-Mode, ou MCP-curado) e deixar o hook **apontar** para ela no momento certo — afordância, não sermão.

---

_Mapa produzido por 3 exploradores paralelos sobre 48 crates + pesquisa externa (9 papers/fontes 2025-26)
+ Anthropic tool-writing best practices. Próximo passo natural: §3 vira backlog de acoplamento por
capacidade. O Touring não precisa de mais capacidade — precisa que a existente seja **alcançável**._
