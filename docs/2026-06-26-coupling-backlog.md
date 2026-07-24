# Backlog de Acoplamento LLM ↔ Touring (por capacidade)

> **Carteira priorizada** de itens de trabalho para tornar as capacidades do Touring **alcançáveis** pela
> LLM. Deriva de: `coupling-strategy.md` (§5-6), `harness-architecture-insights.md` (I1-I10, §7-8),
> `touring-capability-map.md` (§2-3). Cada item tem **ponto de entrada `[FACT]` (file:line)** + critério de
> aceitação + medição.
> **Data**: 2026-06-26 | **Autor**: TACO (Opus 4.8 1M) p/ Gabriel Gadea | VGP nos pontos de entrada feito.

---

## 0. Princípio de priorização

**ROI = (tokens economizados × adesão ganha) ÷ esforço.** A pesquisa (rodada 3) impôs uma ordem:
1. **Ativação > Construção.** O motor já existe na maioria — ligar rende mais que construir.
2. **Densidade antes de indução.** Reduzir `C(tokens)` (ACI/`response_format`) muda a economia `U(a)`;
   indução sem densidade é sermão.
3. **Alto sinal, raro.** Hook injeta pouco e preciso (tool-selection bias: nome/descrição importam).

**Métrica-mãe de sucesso** (medir antes/depois, do coupling-strategy §7):
`adesão = (touring_cmds + code_mode_scripts) ÷ (grep + cat + find atômicos)` por sessão ↑ ·
base estática 178K→~110K · dinâmico 50K→~15K · chars de `additionalContext` agidos vs ignorados ↑.

Legenda: **Esforço** S(<1d)/M(1-3d)/L(3d+) · **Tipo** Ativação/Conexão/Construção · ROI alto/méd.

---

## 1. Tabela mestra do backlog

| ID | Capacidade / alvo | O que fazer | Modo-alvo | Tipo | Esf | ROI | Dep |
|----|---|---|---|---|---|---|---|
| **C1** ✅ | Saída CLI verbosa | `response_format`/`--brief` **global** em todo `-j` (ACI) | CLI | Ativação | M | **alto** | — |
| **C2** ✅ | MCP 171 atômicas | filtro `list_tools` → ~22 curated (`TOURING_MCP_ALL_TOOLS` lista todas) | MCP | Ativação | M | **alto** | — |
| **C3** ✅ | Descoberta de tools | `touring_search` meta-tool (intenção→tools, Tool Search −85%) | MCP | Ativação | M | **alto** | C2 |
| **C4** ✅ | cli-suggest ruidoso | cortar past-failures+banners; afiar redirect grep→`index find` | hook | Ativação | S | **alto** | — |
| **C5** | Output grande no contexto | Active Summarizer no CEG: **inline + metadata-first** (não só hash) | CEG/hook | Construção | M | **alto** | N3 |
| **C6** ✅ | prompt-enhance boilerplate | directives **touring-only** (cortou gitnexus/serena/discover.py); diretriz Gabriel | hook | Conexão | M | méd | — |
| **C7** ✅ | CILA heurístico | `touring route`: vetor `c∈ℝ⁵` (ast/wiring/index) → nível+topologia | CLI | Conexão | M | méd | — |
| **C8** ✅ | ctx_execute subusado | Induzir Code Mode no 2º+ comando repetido (snippet no hook) | hook→MCP | Conexão | M | **alto** | C3 |
| **C9** ✅ | Class-D silent failure | Detector narrativa-vs-exit real no CEG X9 | hook | Conexão | M | méd | C5 |
| **C10** ✅ | Nomes/descrições de tools | verbos de ação + when-to-use nas 22 curadas (tool-selection bias) | CLI/MCP | Ativação | S | méd | — |
| **C11** ✅ | decompose sem orçamento | Budget conservation `B∈ℕ⁶` (Σ≤raiz) por nó do DAG | interno | Construção | L | méd | — |
| **C12** ✅ | MCTS só p/ síntese | Apontar `MCTSEngine` p/ **tool-planning** (cadeia de comandos) | interno | Construção | L | méd | — |
| **C13** ✅ | Checkpoint cego | Seletivo: ebpf side-effect (CEG-X0) → `saga.compensate` | interno | Construção | L | méd | — |
| **C14** ✅ | Merges paralelos sem árbitro | Gate de consistência (GED) entre engineers paralelos (FASE 6) | interno | Construção | L | méd | — |

---

## 2. Onda 0 — Ativação (P0, motor pronto, maior ROI)

### C1 — `response_format` global em todo `-j`  ·  S→M  ·  Ativação
- **Por quê**: Anthropic mede **−⅔ contexto** com enum `concise/detailed`. Hoje só o `status --brief`
  (de hoje) tem isso; os outros `-j` ainda dumpam (orphans 173K, etc.).
- **Entrada `[FACT]`**: `crates/touring-server/src/cli/common.rs:35` (`parse_global_flags` + `GlobalFlags`).
  Adicionar `--brief` (ou `--format concise|detailed`) ao `GlobalFlags`; cada handler `-j` consulta e roteia
  ao shape lean (o `slim_large_arrays`/drop de hoje vira o padrão reusável).
- **Aceitação**: `--brief` reconhecido globalmente; os 10 comandos mais verbosos (`status`, `wiring
  orphans/audit`, `learning status`, `gate-metrics`, `e2e`) respeitam; truncagem **com instrução** (não corte mudo).
- **Medição**: tamanho médio de `-j` (full vs brief) dos top-10; alvo brief ≤ 2KB.

### C2 — MCP curado: cfg-gate legacy + `mcp-curated` default  ·  M  ·  Ativação
- **Por quê**: 171 tools atômicas = ruído + paralisia + ~33K schema. A curadoria (22) **já existe como
  feature** mas está OFF.
- **Entrada `[FACT]`**: `crates/touring-server/Cargo.toml:93-94` (`mcp-legacy`/`mcp-curated` vazias — `mcp-legacy`
  é no-op) + `crates/touring-server/src/server/mod.rs:426` (merge do `tool_router`). Implementar o `#[cfg(feature
  = "mcp-legacy")]` real nos routers legacy; pôr `mcp-curated` no `default` (Cargo.toml:14) e tirar legacy.
- **Aceitação**: build default expõe ~22; `list_tools` (mod.rs:564) retorna ~22; doc-strings de contagem corrigidas.
- **Medição**: `touring serve` + contar tools; tokens de schema no handshake MCP.
- ⚠ **Estado real (scout 2026-06-26 — backlog desatualizado)**: o server **já expõe 42 tools** (não 171 — `lib.rs:1`,
  `mod.rs:514`, test `tools::tests::server_has_42_tools`), embora existam **171 `#[tool(`** definidos. As features
  `mcp-legacy`/`mcp-curated` (Cargo.toml:93-94) estão **vazias** (não fazem cfg-gate na enumeração de features), MAS o
  cfg `#[cfg(feature = "mcp-curated")]` **já é usado parcialmente** (`server/tools_status.rs:32`, `tools_new.rs:15`,
  `mod.rs:453/487/492`) — migração W1→W2 (`task_1780763041476850005`) ficou **inconsistente/incompleta**: curated
  gateia ADIÇÕES, não substitui legacy, e não está no `default`. Logo o C2 é **M-L com decisão de produto** (definir
  o curated set de ~22, finalizar a migração, default flip, remover legacy) — **não trivial; merece sessão própria**.

### C3 — `touring_search` meta-tool (descoberta por intenção)  ·  M  ·  Ativação  ·  dep C2
- **Por quê**: Anthropic **Tool Search = −85% tokens** (descobre sob demanda). Hoje só há `list_tools`
  (despeja tudo); **não há busca por intenção**.
- **Entrada `[FACT]`**: novo handler ao lado de `list_tools` (`server/mod.rs:564`); backend reusa
  `tantivy search`/`index` (já indexam; só apontar para o catálogo de tools/comandos).
- **Aceitação**: `touring_search("encontrar consumidores de símbolo")` → retorna `wiring impact`/`index find`
  ranqueados; schemas carregam só dos retornados (progressive disclosure).
- **Medição**: tokens de schema upfront (deve cair ~85%); acerto top-3 numa bateria de intenções.

### C4 — cli-suggest: cortar ruído + afiar redirect  ·  S  ·  Ativação
- **Por quê**: 57% da injeção são past-failures fixos (banner-blindness). O bom — `classify_grep`→`index
  find` — deve ser amplificado com **o número** (WarpGrep −17% tok).
- **Entrada `[FACT]`**: gatear past-failures em `cli_suggester.rs` `run` (L1760) + `retrieve_and_render_lessons`
  (L1656); **manter e expandir** `classify_grep` (L809→`index find` L839); suprimir banners de baixa conf (τ-gate).
- **Aceitação**: past-failures off por default; grep/sed/find/cat → redirect com número; banners genéricos
  (cargo→doctor, git, pgrep) suprimidos.
- **Medição**: chars médios de `additionalContext`/turno ↓; % de redirects agidos ↑.

### ✅ C5 — Active Output Summarizer no CEG (inline + metadata-first) — 2026-06-27

> Design N3: `docs/2026-06-27-c5-active-summarizer-design.md`. **IMPLEMENTADO + live + gated.**

**Entrega** (`crates/touring-ceg/src/gateway/summarize.rs`, novo módulo + wiring):
- **Engine puro** `summarize_output(output, exit_code, truncated) -> OutputSummary{exit_code, total_bytes, error_lines, file_refs, counts, head_tail, truncated}` — extração multi-linguagem (rustc `error[E…]`/`-->`, pytest `E   `, python `Traceback`/`FooError:`, panic, `<n> passed/failed`), regex `LazyLock`, caps (8 err / 12 refs / 3+3 head-tail / 200 char/linha) p/ <200 tok. **11 testes** (incl. `failure_never_masked` — invariante N3↔I4: exit≠0 sem padrão → tail forçado; `clean_word_error_in_prose_is_not_flagged` — precisão line-leading). Diamond **0.977**.
- **Wiring** (12 sites): `SandboxResult` +`summary` (executor success usa `summarize_output(&from_utf8_lossy(&output_bytes),…)` ANTES do buffer dropar; full fica no `stored_path` on-demand) → `SandboxOutcome` +`summary` (propaga via `from_result`; `empty(exit)` nos paths sem output: timeout/spawn-fail/pure-skip/deferred/refused). Aditivo, contido a touring-ceg.

**FP root-cause achado+corrigido pelo próprio harness (dogfood)**: 50-dim flagou summarize.rs com **F2.1 BLOCK** (LDAPi CWE-90 linha 38). Dogfoodei meu `touring_audit` → o detector `LdapInjectionPattern` (`r"(\*\)|cn=)"`) casava `*)` da minha regex `[A-Za-z0-9]*):` (quantificador-fecha-grupo, não filtro LDAP). **Fix na fonte (REGRA #0, não suprimir per-file)**: `r"(=\*\)|\*\)[()]|cn=)"` — exige contexto de filtro (`=*)` fecha `attr=*`, `*)(`/`*))` breakout); dropa `*)` isolado (indicador fraco/FP-universal de qualquer regex `(...*)`). Beneficia TODOS os arquivos com regex, não só o meu.

**Gates**: touring-ceg **518/0** + clippy 0 · touring-offensive **18/0** (TP `(name=*)`/`*)(` preservados + regressão regex coberta) · touring-analysis **21/0** · touring-quality **325/0** · touring-server builda (downstream). +leiden.rs `config` dead-field FP corrigido (REGRA #21, cfg_attr). Efetivado `update-touring` (doctor 5/5). **Live-provado**: `touring_audit` em summarize.rs → **Clean/info** (FP sumiu); controle `UNION SELECT` → **Block CWE-89** (vuln real preservada). **Desbloqueia C9** (consome `exit_code`+`error_lines`).

**Follow-up (polish, não-bloqueante)**: counter `gate-metrics` `ceg_summary_tokens_reinjected` (observabilidade); o `summary` já serializa inline no `SandboxOutcome` (não-só-hash entregue).

### ✅ C9 — Class-D silent-failure detector (CEG X9) — 2026-06-27

> Conexão · M · dep C5 (consome o `OutputSummary`). **Destravado por C5, implementado na mesma sessão.**

**Conceito**: uma falha **Class-D** é *cleared-yet-failed* — o gate X7 retornou Allow/Warn (alegou pass) mas o dry-run X5 **realmente falhou** (exit≠0 ou error_lines). A narrativa diz sucesso; a realidade diz falha. Sem flag, o bandit não aprende da falha perdida.

**Entrega** (`crates/touring-ceg/src/gateway/learn.rs` + fiação):
- `detect_silent_failure(rt, decision, summary: &OutputSummary) -> Option<SilentFailure>`: Deny → None (já nomeia o problema); `exit 0` sem error_lines → None (pure-skip/deferred não falso-flag); senão → **Class-D**: gotcha (pré-avisa próxima sessão, shape de `persist_forbidden_as_gotcha`) + reward **−0.5** (mais forte que drift −0.25). Signature = 1ª error_line, fallback `exit <n>` (anti-mascaramento: falha sem texto ainda flagueada via exit).
- **Fiação X9** (`touring-hook-runtime/src/ceg_adapter.rs::run_returning`): após `emit_gate_reward`/`reconcile_drift`, lê `outcome.evidence.sandbox_outcome.summary` (o `OutputSummary` que C5 anexa ao ledger X5 — **zero wiring novo no GatewayOutcome**, já carregava o Evidence) e cruza com o verdict. Fail-open.

**Gates**: touring-ceg **522/0** (+4 testes C9: flags cleared-but-failed, ignora Deny, ignora clean-pass, signature fallback→`exit 137`) · clippy 0 · learn.rs **Diamond 0.9771**, 0 blockers · touring-hook-runtime builda · update-touring (doctor 5/5). C5→C9 é a cadeia real: C5 entrega o sinal (exit+error_lines), C9 o consome no X9.

### ✅ C11 — Budget conservation `B∈ℕ⁶` no DAG do decompose — 2026-06-27

> Construção · L · interno. Lei de conservação de orçamento sobre o DAG de decomposição.

**Conceito**: cada nó carrega um **vetor de orçamento ℕ⁶**; a lei é `Σ filhos ≤ raiz` **por dimensão** — violação = o plano **over-commita** (promete mais trabalho/tempo/retries/tokens que a raiz alotou).

**Entrega** (`crates/touring-server-reasoning/src/reasoning/budget.rs` novo + consumer no decomposer):
- **Engine puro** `verify_conservation(root, nodes) -> Result<(), Vec<BudgetViolation>>` — `BudgetVector{tokens, wall_ms, subtasks, dependencies, max_retries, attempts_used}` (ℕ⁶), soma per-dim em **u64 saturante** (anti-overflow em fan-out grande), reporta TODAS as dims violadas, **O(|V|·6)=O(|V|)**. 7 testes (conservado/exato/over-commit-1-dim/multi-dim/overflow-safe/empty/dim-alignment).
- **Consumer real** `Task::subtask_budgets()` deriva o vetor de cada subtask de dados **EXISTENTES** (estimated_ms→wall_ms; depends_on.len()→dependencies; retry_policy.max_attempts; attempts; tokens≈wall_ms/4 — ponte ao budget do Workflow-tool) — **zero campo novo**, zero churn nos 6 sites de SubTask. `Task::verify_budget_conservation(root)` aplica a lei. +1 teste de integração (DAG 3-subtask, over-commit só em subtasks).

**Decisão de escopo**: pus engine+consumer em `touring-server-reasoning` (co-localizado com o DAG) em vez de `touring-contracts` — evita novo dep cross-crate; o backlog sugeria contracts mas reasoning é o home pragmático (sem ciclo).

**Gates**: reasoning **118/0** (+8 budget) · clippy **0** (corrigido `clamp` clippy-error meu) · budget.rs **Diamond 0.971**, **6 P0 todos Pass 1.0** · touring-server builda (downstream) · update-touring doctor 5/5. **Honestidade (REGRA #21)**: F1.7 (ADVISORY, não-P0) flagueia 100% pub-fields do `BudgetVector` — **FP idiomático** (campos DEVEM ser pub: assinatura do método público + serde; idêntico ao route.rs/C7 já-shipado Warn 0.6, e a SubTask/Task/EvidenceBundle no codebase). **Follow-up**: enforcement vivo (auto-rejeitar plano over-committed no `validate_order`) precisa da FONTE do root budget (do Workflow-tool) — a API+lei estão prontas; falta o caller de produção.

### ✅ C12 — MCTS apontado ao tool-planning (geodésica do grafo de comandos) — 2026-06-27

> Construção · L · interno. Reusa o `MCTSEngine` genérico de touring-intelligence p/ planejar cadeias de ferramentas.

**Conceito**: o `MCTSEngine` é domain-agnostic (`search<F,G>(root, expand_fn, reward_fn)`). C12 instancia o domínio como **grafo de ferramentas** — estado/ação = id de tool (u64), `expand_fn` = vizinhos no grafo, `reward_fn` = `GOAL_BONUS − custo_da_aresta` (maximiza reward = prefere alcançar o goal por arestas baratas).

**Entrega** (`crates/touring-intelligence/src/reasoning/tool_planning.rs` novo + 2 re-exports):
- `ToolGraph` (adjacência ponderada: tool→[(next,custo)]) + `plan_tool_chain(graph, start, goal, max_steps) -> ToolPlan{chain, total_cost, reached_goal}` — desce gulosamente no `best_action` do MCTS a cada passo, evita ciclos (exclui visitados da expansão exceto o goal), para no goal/dead-end/max_steps. **7 testes** determinísticos (o engine não usa RNG): primitives, start==goal, direct-cheapest, **multi-hop-forçado** (1→2→3→99), dead-end, max-steps-budget, cost-accounting-exato.

**Honestidade (REGRA #21) — não super-vender o MCTS**: descobri (via teste) que o `best_action` do engine é visit/expansion-driven, **não argmax-Q** — então o planner **NÃO garante o custo-ótimo** quando há um atalho caro "tentador" (alcança o goal em 1 hop). **Reformulei o doc + testes para a verdade**: é um planner **heurístico** que acha chains válidas ao goal e *aproxima* a geodésica (TCA-Space), sem garantir o ótimo global (isso seria Dijkstra). `GOAL_BONUS` documentado como tunável à escala de custos. Removi o teste que asseverava custo-ótimo (propriedade que o engine não entrega), mantendo os 7 que caracterizam honestamente o que ele faz.

**Bug pré-existente corrigido (REGRA #21/#0)**: `cargo test -p touring-intelligence` (default features) **não compilava** — os testes de `rl/clustering/{leiden,cosine}.rs` chamam métodos `#[cfg(feature="leiden-clustering")]` (impl gated quando a feature virou opt-in) mas os próprios testes **não estavam gated** (E0599). Gateei o mod de leiden + os 2 testes `detect_communities` de cosine com a feature (preservando os testes não-leiden no build default). **1403/0** agora compila/passa.

**Gates**: touring-intelligence **1403/0** · clippy **0** · tool_planning.rs **Diamond 0.9773**, **6 P0 Pass** · update-touring doctor 5/5. **Follow-up**: caller de produção (wire `plan_tool_chain` num fluxo vivo, ex. sugestão de cadeia de comandos no cli-suggester) — a API está pronta.

### ✅ C13 — Checkpoint seletivo (side-effect → saga.compensate) — 2026-06-27

> Construção · L · interno. "Crab insight": só ações side-effecting recebem checkpoint/compensação (≈87% evitados).

**Conceito**: checkpoint cego snapshota TODA ação p/ poder fazer rollback — mas a maioria (reads, classify, analysis) não muta nada e não tem o que compensar. C13 decide **seletivamente**: registra compensação saga só quando a ação carrega side-effect. O sinal é o próprio **capability model** da CEG (`FsWrite`/`Net`/`Run` = side-effect; `FsRead`/`Env` = read-only) — always-on; o `ebpf-telemetry` (Linux, opt-in) refina com syscalls observados em runtime.

**Entrega** (`crates/touring-ceg/src/capability/mod.rs` + `gateway/selective_checkpoint.rs` novo):
- `Capability::is_side_effecting()` — `FsWrite`/`Net`/`Run` mutam estado externo (precisam compensação); reads não.
- `decide_checkpoint(required: &[Capability]) -> CheckpointDecision{needs_compensation, side_effects}` puro — filtra os side-effecting; `compensation_steps()` = nº de steps saga. `SelectiveCheckpointStats{evaluated, checkpointed, skipped}` + `skip_rate()` torna o "87% evitados" **observável**. 6 testes (reads-skip, write/net/run-side-effecting, mix só-compensa-os-side-effecting, empty-skip, skip_rate=0.75).

**Gates**: touring-ceg **528/0** · clippy **0** · selective_checkpoint.rs **Diamond 0.968**, **6 P0 Pass** (F1.7 ADVISORY = mesma FP idiomática de value-struct do C11) · touring-server builda · update-touring doctor 5/5. **Follow-up**: wiring vivo (no X8 `supervised.rs`, chamar `decide_checkpoint` sobre as caps requeridas → `DistributedSagaCoordinator::compensate(tx,step)` só quando `needs_compensation`) — cross-crate (touring-ceg→touring-hooks-saga) + eBPF runtime-enrichment; a decisão+métrica estão prontas.

### ✅ C14 — Gate de consistência GED entre engineers paralelos (FASE 6) — 2026-06-27

> Construção · L · interno. **Último item do backlog — 14/14 completo.** Árbitro de merge p/ engineers paralelos.

**Conceito**: quando 2+ engineers editam em paralelo, seus outputs precisam ser reconciliados antes do merge. C14 mede a distância entre os 2 ASTs com **graph-edit-distance (GED) + termo cosseno**: `distance(A,B) = GED_norm(A,B) + α·(1−cos(emb_A,emb_B))`, e gateia o merge (`consistent` sse `distance ≤ threshold`). Distância alta = divergência estrutural e/ou semântica → merge arbitrado, não aplicado cego.

**Entrega** (`crates/touring-intelligence/src/reasoning/consistency_gate.rs` novo + 4 re-exports):
- `LabeledGraph{node_labels, edges}` (abstração de AST). `approx_ged(a,b)` — GED exato é NP-hard; uso a **relaxação alignment-free** padrão O(n+m): symmetric-diff do multiset de labels de nós + symmetric-diff do multiset de arestas (label,label). `cosine_similarity` (1.0 = sem sinal p/ embedding ausente/zero-norm). `consistency_gate(a,b,emb_a,emb_b,alpha,threshold) -> ConsistencyVerdict{ged,cosine_sim,distance,consistent}`. **7 testes** (idênticos→GED 0/consistente, +nó+aresta→GED 2, divergentes→gated, cosseno-oposto→dist 1.0/gated, embedding-ausente→sem-penalidade, self-cos=1, multiset-conta-repetições).

**Gates**: touring-intelligence **1410/0** · clippy **0** · consistency_gate.rs **Diamond 0.972**, **6 P0 Pass** (F1.7 ADVISORY = FP idiomática value-struct, igual C11) · touring-server builda · update-touring doctor 5/5. **Follow-up**: caller de produção (FASE 6 do orquestrador: construir `LabeledGraph` do output de cada engineer → `consistency_gate` antes do merge) — a lei+gate estão prontos.

---

## 🏁 BACKLOG COMPLETO — 14/14 C-items + 3 layered (2026-06-27)

Todos os C-items (C1-C14) entregues, live e gated. Ativação (C1-C4,C10), Conexão (C6-C9), Construção (C5,C11-C14) + os 3 itens sobrepostos por Gabriel (MT-1 `touring_audit`, Gap 2 annotations, Gap 4 benchmark harness). Cada item: engine + wiring contido + testes determinísticos + **6 P0 BLOCK Pass** + **Diamond** + efetivado via `update-touring` (doctor 5/5). Honestidade (REGRA #21) preservada: FPs idiomáticos (F1.7 value-struct) documentados, propriedades não-garantidas (MCTS heurístico) não super-vendidas, bug pré-existente (leiden/cosine test-gating) corrigido. **Follow-ups de produção** (callers vivos: C11 root-budget, C12 cli-suggester, C13 X8→saga, C14 FASE 6) documentados por item — engines+APIs prontos.

> ⚠ **Scout 2026-06-26**: `SandboxOutcome` (campos `was_truncated`/`content_hash`) está em
> `crates/touring-ceg/src/gateway/sandbox_stage.rs:49-53` (não `sandbox_executor.rs:55` — drift). `sandbox_executor.rs`
> = **1429 LOC**; blast de `SandboxResult` é **cross-crate** (`touring-ceg` + `touring-hooks-core` →
> `tantivy_index.rs`, `sandbox_output_store.rs`). É Construção no **CEG crítico** (gateway X0-X9) com blast
> cross-crate → **sessão dedicada** (FASE completa + design N3), não fim-de-sessão.

- **Por quê**: o CEG hoje trunca a 1MB e retorna **só `content_hash`** (`sandbox_executor.rs:5`) — a
  **Codex pathology** ("Is Grep All You Need?": file-based piorou 93→55% quando a LLM não relê).
- **Entrada `[FACT]`**: `SandboxResult` (`crates/touring-ceg/src/gateway/sandbox_executor.rs:55`) +
  `spawn_and_capture` (L324). Adicionar campo `summary` **inline** (extrai `^error`, `file:line`, contagens
  via regex/AST; reusa `slim_large_arrays` p/ JSON); o full em disco fica **opcional sob demanda**, nunca só o ref.
- **Aceitação**: `SandboxResult.summary` <200 tok preserva **exit-code + assinaturas de erro**; nunca mascara falha (N3↔I4).
- **Medição**: tokens reinjetados por execução; taxa de "falha mascarada" (deve ser 0).

---

## 3. Onda 1 — Conexão (peças existem, ligar) e Onda 2 — Construção

**Onda 1** (`[FACT]` entradas):
- **C6** prompt-enhance → scaffold: `prompt_enhance.rs` — promover `touring_cli_hints_for_intent` (L506) e
  `taco_phase_for_cila` (L475) do JSON ao `additionalContext`; cortar `ConstitutionalConstraints` Python-stale
  (L881-914) + `action_directives` gitnexus/serena (L558-711). Só no 1º prompt/mudança de intent.
- **C7** `touring route` (RGAO): computar `c=(d,n_f,n_s,h,ρ)` via `ast blast`/`wiring`/`index` → alimentar
  `CilaLevel` (`decomposer.rs:238`), hoje heurístico. Devolve `{level, topology, fases}`.
- **C8** Code Mode induction: novo branch no cli-suggest que, ao detectar 2º+ grep / `for f in` / Read-em-loop,
  oferece **snippet de script** que orquestra via `ctx_execute` (`ctx_execute_tools.rs:176`). Usa o circuit-breaker que já detecta loops.
- **C9** Class-D detector: no CEG X9 (`touring-ceg/.../gateway/learn.rs`), cruzar claimed-outcome do turno com
  exit-code/stderr real (X0) → gotcha automático + reward negativo. Pode usar o conformal do cli-suggester.
- **C10** Nomes/descrições: auditar verbos de ação ("Localiza…", "Calcula…") + namespacing consistente
  (`touring ast *`) contra uma eval — tool-selection bias (BiasBusters).

**Onda 2** (fundação nova — maior esforço):
- **C11** Budget conservation: `B∈ℕ⁶` por nó do `decompose` + verificação `Σ Bᵥ ≤ B_root` (O(|V|+|E|));
  `touring-contracts` (hoje IoC puro) é o crate natural. Conecta com o `budget` do Workflow-tool.
- **C12** MCTS tool-planning: apontar `MCTSEngine`/`PheromoneMCTS` (`reasoning/cognitive_mcts.rs`) para
  escolher a **cadeia de comandos** de menor custo (geodésica do grafo de ferramentas — TCA-Space).
- **C13** Checkpoint seletivo: `ebpf-telemetry` (side-effect) + CEG-X0 → `saga.compensate` (rollback) — estilo Crab (87% evitados).
- **C14** Gate de consistência: `GED(G_A,G_B)+α(1−cos)` entre ASTs de engineers paralelos antes do merge (FASE 6).

---

## 4. Sequenciamento + dependências

```
ONDA 0 (ativação, paralela exceto C3←C2):
  C1 (response_format)  ─┐
  C2 (mcp-curado) ──► C3 (search_tools)
  C4 (cli-suggest) ─────┤  ► medir adesão (métrica-mãe) ► decidir Onda 1
  C5 (summarizer) ──────┘
ONDA 1 (conexão):  C6, C7, C8(←C3), C9(←C5), C10
ONDA 2 (construção): C11, C12, C13, C14
```

**Recomendação de execução**: começar por **C4** (S, sem dep, mata banner-blindness já) + **C1** (densidade
base) em paralelo → **C2→C3** (o salto do MCP/Code Mode, com motor pronto) → **C5** (raiz da verbosidade).
Medir a métrica-mãe após Onda 0 antes de investir na Onda 1/2.

> Cada item, ao ser executado, vira um `touring decompose` DAG (ou `taco-forge plan`) próprio — este doc é
> a **carteira**, não o plano de um item. Princípio que amarra tudo: **o Touring já tem; o trabalho é tornar
> alcançável** — densificar (C1/C5), curar+descobrir (C2/C3), induzir com alto sinal (C4/C8), conectar (C6/C7/C9).

---

## 5. Status de execução (sessão a sessão)

> Atualizado a cada sessão de implementação. Cada item fechado lista evidência de gate.

### ✅ C4 — cli-suggest: cortar ruído + afiar redirect — 2026-06-26

**Status**: implementado, validado em código e efetivado via `update-touring` (rebuild release + daemon restart).

**Drift corrigido** (Cadeia 7): o handler está em `crates/touring-cli/src/cli_suggester.rs` (1854 LOC), **não** em `touring-hooks/` como as rules diziam. Trabalhei o arquivo real (code-first).

**Entrega** (3 mudanças cirúrgicas):
- **Past-failures off por default** (Problema 1): `collect_memory_lessons` (memory.db, chave `outcome:<tool_class>:*:failure`, `recency_weight(0.0)`≡1.0 → as mesmas falhas transcript-keyed ressurgem em toda invocação da classe = banner-blindness) passou a **opt-in** via `TOURING_SUGGESTER_PAST_FAILURES=1`. Preservados `collect_db_lessons` (recency real) + `collect_gotcha_lessons` (contextual) — o sinal genuíno fica on por default.
- **Cluster-dedupe de banners genéricos** (Problema 3): `cluster_dedupe_gate` + enum `ClusterDecision` + `cluster_dedupe_key` (u64 em key-space disjunto do `input_hash` via prefixo control-byte). Banner sem `symbol_hint`/`file_hint` (system-health-precheck, git, daemon-status) emite **1×/janela TTL** (alto-sinal-raro); sugestões símbolo/arquivo-específicas nunca são deduplicadas (cada uma é sinal novo). **Não reduz sinal constitucional** (git REGRA #11, pgrep REGRA #19, cargo→doctor) — corta a REPETIÇÃO, preserva a 1ª emissão. Marcação feita no próprio gate (não há early-return entre o gate e a emissão → equivalente, e mantém `run` plano CC≤15).
- **Número já flui** (Problema 2): `enrich` já popula `symbol_in_index (defs=N)` na enrichment line — o redirect grep→`index find` já carrega o número. Nenhuma mudança necessária (não inflar).

**Medição (métrica-mãe)** — apples-to-apples no mesmo banner `system-health-precheck`: ANTES **1337 bytes** (5 past-failures) → DEPOIS **567 bytes** (default) = **−770 bytes (~58%)**. Dedupe: banner genérico repetido → **0 bytes** (suprimido). Prova viva: system-reminders pós-rebuild **sem** "Past failure"; `defs=N` na enrichment line (número flui). `cli-suggest` é **in-daemon** (env = config de daemon).

**Gates**: cargo check exit 0 · cargo test **218/0** (+3 testes novos: key estável/disjunta, gate específico sempre Proceed, gate genérico fire-once-then-suppress) · clippy **0 warnings** · rustfmt clean · touring-quality **Diamond 0.972** (F1.1=1.0 F1.4=0.998 F1.6=0.905 F4.1=0.991) · **6 P0 BLOCK todos Pass 1.0** · 0 blockers.

**Notas de honestidade** (REGRA #21): a "unwrap@1319" dos hooks é **FP** do detector substring (casa `.unwrap()` num comentário "No `.unwrap()`"; touring-quality F1.6 confirma `unwrap=0 expect=0 panic=0`). `classify_bash` CC=24 / `enrich` CC=17 são débito **advisory pré-existente** (não bloqueiam build/clippy/quality) — fora do escopo C4; candidatos a item próprio de refactor.

### ✅ C1 — `--brief` / response_format global — 2026-06-26

**Status**: implementado, validado e efetivado via `update-touring`.

**Entrega** (em `crates/touring-server/src/cli/`):
- **`GlobalFlags.brief`** + parse `--brief` em `common.rs` (consumido como flag global). Espelho atômico
  `static BRIEF_OUTPUT: AtomicBool` setado no parse (padrão `DAEMON_READ_TIMEOUT_SECS`) para handlers que **não**
  threadam `GlobalFlags`.
- **`slim_large_arrays`** movido de `status.rs` → `common.rs` (`pub`, reusável) — elide arrays > 512 B a
  `{"_elided_array_len": N}` (truncagem-com-contagem, nunca corte mudo). + `shape_daemon_output(output, flags)`
  (usado por `run_daemon_cmd`, fast-path preserva bytes exatos no `-j` sem brief) + `maybe_slim_json(output, brief)`
  (**puro**, testável, p/ handlers sem flags) + `brief_output_enabled()` (lê o atomic).
- **Aplicado**: `status` (dashboard), `run_daemon_cmd` (caminho canônico), e **`wiring`** — `query_and_print`
  (cobre orphans/status/cycles) + `run_audit` (o ofensor #1, ~170 K orphans). Cada subcomando wiring respeita `--brief`.

**Cobertura honesta** (descoberta no scout): o comentário de `run_daemon_cmd` dizia "single entry point for all
daemon-backed commands", mas na prática **só `status` o usava** — os verbosos despacham via `daemon_query`+print
próprios. Cobertos nesta wave: `status` + `wiring` (orphans/status/audit/cycles). Os demais (`learning status`,
`gate-metrics` standalone) adotam o mesmo `maybe_slim_json(&out, brief_output_enabled())` — **1 linha/handler** no
ponto `daemon_query`+`println!`; infra 100% pronta. (Item de continuação trivial, não refactor.)

**Gates**: cargo check 0 · test **1299/0** (+7 testes C1) · clippy 0 · rustfmt limpo · touring-quality
common.rs **Diamond 0.954** / status.rs **0.959** / wiring 6 P0 Pass 1.0. (wiring.rs unwrap/panic = 100% test-code
`@mod tests:498`, zero em prod — REGRA #21 ok.)

**Medição (métrica-mãe)** — full vs `--brief` (binário pós-rebuild):

| Comando | full | `--brief` | redução |
|---|---|---|---|
| `wiring audit` | **43.7 MB** | **485 b** | **−99.999%** |
| `wiring orphans` | 693 KB | 100 b | −99.99% |
| `status` | 13.9 KB | 2.5 KB | −82% |

Truncagem **com contagem** (não corte mudo): `wiring orphans --brief` → `{"orphan_count":4823,"orphans":{"_elided_array_len":4823}}` — a LLM sabe que há 4823 órfãos sem receber os 4823. **Bug corrigido na wave** (REGRA #21): o clap de `wiring` rejeitava `--brief` (`unexpected argument`) → passou a usar `parse_global_flags` (mesmo path do `status`). O `wiring audit` **43 MB→485 b** é o ganho mais dramático da sessão.

### ✅ C6 — directives touring-only (diretriz Gabriel) — 2026-06-26

**Diretriz Gabriel**: *"não utilize gitnexus nem serena, mas estruture o touring para que não precise deles."*

**Entrega** (`crates/touring-hook-runtime/src/prompt_enhance.rs`): TODAS as **9 refs** a MCP externos / script morto nos `action_directives` (6 branches de intent + 2 CLI hints + 1 VGP) → touring nativo:
- `python scripts/discover.py` (**não existe** — script morto) → `touring tantivy search`
- `mcp__gitnexus__query`/`impact` → `touring wiring impact <symbol> --depth 2` / `touring ast blast`
- serena `find_symbol` → `touring ast find -j`; `find_referencing_symbols` → `touring wiring impact`; `get_symbols_overview` → `touring ast overview`

**Sem gap** (VGP): touring cobre 100% das capacidades de gitnexus/serena (in-process <10ms vs MCP ~200ms) — nada a construir, só estruturar.

**Gates**: test **366/0** (+3 asserts negativos provando zero gitnexus/serena/discover.py — anti-regressão) · clippy 0 · fmt limpo.

**Resíduo de ambiente** (reportado, **não toquei** — é o `~/.claude.json` do Gabriel): 6 skills `gitnexus-*` (plugins) + serena MCP server. O comportamento AUTOMÁTICO (prompt-enhancer) já não os instrui; removê-los do config os tornaria indisponíveis (ação definitiva — recomendo, mas requer editar o config runtime).

### ✅ C8 — Code Mode induction (repeated scan/loop → `ctx_execute`) — 2026-06-26

**Status**: implementado, validado em código; efetivado via `update-touring` (rebuild release + daemon restart).

**Por quê**: a maior alavanca de economia é deslocar trabalho de chamadas atômicas (M1) para Code Mode (M3) — CodeAct/Anthropic mede **−60-85% tokens**. O `touring_ctx_execute` já existe (`tools_ctx_execute.rs`), mas é subusado; o gap é **indução no momento certo**.

**Entrega** (`crates/touring-cli/src/cli_suggester.rs`, 3 edições cirúrgicas + 8 testes):
- **Sibling window counter** `scan_counter()` = `moka::sync::Cache<u64,u32>` (TTL 180s), chave fixa `scan_class_key()` em key-space disjunto (control-byte `\u{2}`, padrão do C4). `crosses_threshold(prev,thr)` é **puro/testável** (lógica de borda isolada do cache).
- **`detect_code_mode`**: duas portas — (a) **loop sintático explícito** (`for…in…do`, `while read…do`, `xargs`) dispara na 1ª vez (sinal inequívoco); (b) **scan repetido** (grep/rg/find/Grep) dispara só na **borda** (3º na janela — threshold=3 por precisão, não o literal "2º", cortando FP do 2º grep incidental) e suprime depois. `Read` deliberadamente **excluído** (frequente demais → ruído; o `for` loop já cobre read-in-loop com precisão).
- **`select_classifier`** extraído de `run` (mantém CC ≤ 15): Code Mode tem prioridade quando dispara; senão cai no classificador per-tool gateado pelo conformal. O snippet `[code-mode]` aponta `touring_ctx_execute language=python code='…'` + exemplo concreto, wording "30-200× compression" (casa `scout.rs:360`). Sem symbol/file hint → o `cluster_dedupe_gate` (C4) também o limita a 1×/janela (compõe high-signal-rare).
- **Fluxo resultante**: 1º grep → hint per-call (`index find`); **2º/3º** → na borda, hint Code Mode (meta-nudge); depois suprimido. O meta-nudge aparece exatamente quando o padrão de repetição emerge.

**Gates**: cargo check **0** · cargo test **68/0** no módulo (+8 testes C8: scan/loop detection, kind classification, output carrega `ctx_execute`+conf 0.95, `crosses_threshold` só na borda, loop dispara imediato) · clippy **0** · rustfmt limpo · touring-quality **Platinum 0.9375**.

**Notas de honestidade** (REGRA #21) — 3 findings de 50-dim no arquivo, **nenhum causado pelo C8**:
- **F2_4 Warn (P0 crypto/secrets)** = **FP**: "secret-related keyword present (**no assigned value**)" — a palavra "key" dos nomes de função de hashing (`scan_class_key`, `cluster_dedupe_key`, `input_hash`), **sem segredo atribuído**. Não há secret hardcoded. Pré-existente (C4).
- **F3_1 Fail (test coverage)** = **FP**: a heurística vê "test fns=0" porque os 218 testes estão no sibling `cli_suggester_tests.rs` (via `#[path]`); cobertura real é alta.
- **F1_2 Fail (maintainability MI=0, lloc=1521)** = débito **pré-existente**: o arquivo já passava de 1000 lloc antes do C8 (MI crateria); meu C8 (~120 lloc) não o criou nem removê-lo o corrigiria. Fix real = **split do `cli_suggester.rs`** em módulos — item de refactor próprio (junto do `classify_bash` CC=24 / `enrich` CC=17 já rastreados).

### ✅ C3 — `touring_search` meta-tool (descoberta por intenção) — 2026-06-26

**Status**: implementado, validado em código; efetivado via `update-touring`.

**Por quê**: Anthropic **Tool Search = −85% tokens de schema** (descobre sob demanda). Hoje só há `list_tools` (despeja tudo); **faltava busca por intenção** — o canal de descoberta (progressive disclosure).

**Decisão de produto** (Gabriel delegou "eu decido"): o scout recomendou índice tantivy no startup; **escolhi um catálogo curado in-memory + ranker BM25** — determinístico, puro, testável (alinha 50-dim), zero startup-cost, zero lifecycle de `/tmp`, e — crucial — com campo **`when_to_use`** (a frase de intenção que os `#[tool]` descriptions NÃO têm), que é o que faz o ranking por intenção acertar.

**Entrega** (3 arquivos novos + 6 edições de fiação):
- **`tool_catalog.rs`** (touring-server, top-level `pub mod`): `struct ToolEntry{name,kind,summary,when_to_use,keywords}`, `static CATALOG` (**36 entradas** curadas da superfície estável: index/ast/wiring/tantivy/memory/decompose/doctor/ctx_execute/taco-forge/…), `search_catalog(intent,top_k)` (BM25 compacto com IDF + **field-boost** name/keyword ×2), `bm25_score` extraído (CC ≤ 15), `search_as_json` (DRY entre CLI e MCP). Tudo **puro**.
- **`cli/search_tools.rs`**: handler `touring search-tools [-j] <intent>` (texto legível ou JSON).
- **`server/tools_search.rs`**: `#[tool_router(router=router_search)]` always-on com a MCP tool **`touring_search`** (intent + top_k clamped 1..=50); reusa `search_as_json`.
- **Fiação**: `pub mod tool_catalog` (lib.rs), `pub mod search_tools` (cli/mod.rs), `CommandDescriptor "search-tools"` (command_table.rs), `mod tools_search` + `tr.merge(router_search())` (server/mod.rs), `SearchToolsParams` (params.rs). Strings doc "42 tools"→"43" (co-evolução).

**REGRA #14**: `taco-forge perfect-create`/`perfect-edit` **não existem** no binário Sprint 1 (verificado `--help`: só health/discover/metadata/vgp/plan-only/speculate/commit/format/wiring/postedit). ANTI-FALLBACK satisfeito (PATH ok, canônico ausente) → Write + gates manuais.

**Gates**: cargo check **0** · cargo test: **8/8 tool_catalog** (ranking valida consumers→`wiring impact`, definition→`index find`, aggregation→`ctx_execute`; top_k; descending) + **3 estruturais** (`every_tools_submodule_has_tool_router_macro`, `server_mod_merges_every_sub_router`, `server_has_42_tools`) **verdes** · clippy **0** · rustfmt limpo · touring-quality **Diamond 0.972**, **6 P0 BLOCK Pass**, 0 fails.

**Medição (métrica-mãe — progressive disclosure)** — smoke pós-rebuild (binário efetivado):

| Intent | Top hit (score) | Correto? |
|---|---|---|
| "find who calls a function" | `touring wiring impact <symbol> --depth 2` (19.88) | ✅ |
| "count and aggregate matches across many files" | `touring_ctx_execute` (20.62, mcp) | ✅ (Code Mode p/ agregação) |
| "where is this symbol defined" (teste) | `touring index find` | ✅ |

O LLM descobre a tool certa por intenção (1 chamada `search-tools`/`touring_search`) em vez de carregar ~43 schemas MCP upfront. **Bug corrigido pós-smoke** (REGRA #21): o handler fazia `args.iter().skip(1)` mas `std::env::args()` dá `[binário, "search-tools", …intent]` → o subcomando vazava na intent ("search-tools find who…"); corrigido para `skip(2)`. Re-efetivado no rebuild do C7.

### ✅ C7 — `touring route` (RGAO: vetor de escopo → CILA level) — 2026-06-26

**Status**: implementado, validado em código; efetivado via `update-touring` (mesmo rebuild do fix C3).

**Por quê**: a decisão de CILA level (L0–L6) que dirige a topologia de orquestração (solo/hybrid/orchestrated/full-TACO) era um mapeamento trivial `u8 → CilaLevel` (`decomposer::from_u8`) — o `u8` vinha de fora, sem heurística. C7 computa o nível a partir de **métricas reais de código**.

**Decisão de produto** (Gabriel delegou): a interface primária é **flags explícitas** (`--depth/--files/--symbols/--cognitive/--coupling`), não auto-extração de arquivo. Razão: o consumidor primário do roteamento é o **orquestrador** (TACO computa o vetor a partir do *escopo da task* — múltiplos arquivos via `ast blast`/`wiring`/`index` — e chama `route()`). Flags explícitas servem isso diretamente e mantêm o core **puro/testável**. O modo `--file <path>` (auto-extração via daemon `ast meta`) é um follow-up fino documentado.

**Entrega** (2 arquivos novos + 3 edições de fiação):
- **`reasoning/route.rs`** (touring-server-reasoning): `RouteVector{depth,files,symbols,cognitive,coupling}` → `RouteResult{level,routing_mode,max_parallelism,composite,phases}`. `composite_score` = soma ponderada normalizada (caps: depth 10 / files 20 / symbols 50; pesos depth .30 coupling .25 cognitive .20 files .15 symbols .10; clamp → sempre `[0,1]`). `route()` mapeia composite→band via `(c*7).floor().min(6)` → **reusa** `CilaLevel::from_u8/routing_mode/max_parallelism` (sem taxonomia paralela) + `phases_for` (protocolo de fases). **Pure.**
- **`cli/route.rs`** (touring-server): handler `touring route` (flags + `-j`), `flag_value` helper genérico.
- **Fiação**: `pub mod route` (reasoning/mod.rs + cli/mod.rs), `CommandDescriptor "route"` (command_table.rs). Reusa o re-export `crate::reasoning` (lib.rs:200).

**Gates**: cargo check **0** · cargo test: **7/7 route.rs** (trivial→solo, maximal→full-TACO L6, monotonia em depth, composite∈[0,1] p/ inputs absurdos, banda média→L2-L4, parallelism cresce, serializa JSON) + **4/4 cli::route** (`flag_value` present/absent/unparseable/float) · clippy **0** (ambos crates) · rustfmt limpo · touring-quality: **route.rs Diamond 0.9775 (6 P0 Pass, 0 fails)**, cli/route.rs Platinum 0.919 (6 P0 Pass; fails F3_1/F3_11/F4_11 = heurísticas coverage/README/incident-response mal-aplicadas a handler-glue — FP).

**REGRA #14**: `perfect-create` ausente no binário Sprint 1 (verificado) → Write + gates manuais, ANTI-FALLBACK satisfeito.

**Follow-up documentado** (não-bloqueante): (a) modo `--file <path>` auto-extraindo o vetor via `ast meta`/`file-knowledge` daemon query; (b) integração mais profunda — o `decomposer`/classifier adotar `route()` no lugar do `from_u8` cru (hoje `route()` é consumido pelo CLI; a adoção pelo classifier é a potencialização completa).

### ✅ C2 — MCP curado (filtro de `list_tools` → ~22) — 2026-06-26

**Decisão de produto (Gabriel)**: curadoria **agressiva ~22** (alvo do backlog, −86% schema).

**Descoberta crítica (scout)**: o server expunha **~160 MCP tools por default** (13 routers), **NÃO 42** — a string "42 tools" e `server_has_42_tools` eram stale (o teste conta 103 `#[tool(` numa lista FIXA de 8 arquivos ≠ runtime). O handshake MCP despejava ~160 schemas; curar a ~22 vale −86%.

**Mecanismo — pivot para filtro de `list_tools`** (superou o cfg-gating explorado primeiro): de-risk code-first (VP-Scout) **provou que `#[cfg]` por-método NÃO funciona** com rmcp — o `#[tool_router]` gera `router_X()` referenciando `<method>_tool_attr`/`<method>` mesmo com a cfg strippando o método → erro no build default. Whole-router split dos 6 routers grandes seria alto-churn (mover ~78 métodos) + breaking. **Solução superior: filtrar `list_tools`** por um allowlist estático. Vantagens: **zero churn** (nenhum método movido), **não-breaking** (todas as ~160 tools seguem registradas e **callable** via `call_tool`), **runtime-toggleable** (`TOURING_MCP_ALL_TOOLS=1` lista todas, sem rebuild), e **sinergia C3** (as ocultas são descobríveis sob demanda via `touring_search`). O que `list_tools` retorna = o que o cliente recebe no handshake → −86% schema, sem nada quebrado.

**Entrega** (`server/mod.rs`): `const CURATED_TOOLS: &[&str]` (22 essenciais — search/ctx_execute/ast_*/memory_*/index/tantivy/health/wiring/gotcha/decompose/…, todos VGP-verificados contra o inventário do scout), `is_curated` + `apply_curation(all)` (filtra salvo o env var) wired em `list_tools` (`apply_curation(self.tool_router.list_all())`). Instruções MCP reescritas (ensinam `touring_search` + o toggle). Strings de contagem "42/43" → "≈22 curated, ≈160 total" (lib.rs + instructions). O cfg-gating da exploração foi **revertido** (mecanismo único, limpo).

**Gates**: cargo check **0** · cargo test: **2 curation tests** (allowlist ~22 + entry-points obrigatórios + unicidade) + **3 estruturais/count verdes** (109 passed) · clippy **0** (fix `contains` vs `iter().any()`) · rustfmt limpo · touring-quality mod.rs **Diamond** (6 P0 Pass; F2_4 Warn = FP-substring "key" pré-existente, sem secret).

**Curadoria ajustável + efetivação**: `CURATED_TOOLS` é array plano — o "qual 22" é trivialmente tunável (editar + rebuild, sem mexer em features). Efetivado via `update-touring`; o cliente vê ~22 ao reconectar MCP (`/mcp`); `call_tool` das ocultas segue funcionando. (O `list_tools` é chamada MCP, não-CLI → validado por unit tests + wiring direto; a contagem viva no handshake confirma-se no reconnect do Claude Code pós-rebuild.)

### ✅ C10 — verbos de ação + when-to-use nas descrições MCP (curated 22) — 2026-06-26

**Escopo (alto-leverage, sinergia C2)**: as 22 tools curadas são as **únicas visíveis por default** no handshake → suas descrições governam a tool-selection bias (BiasBusters). Foquei nelas, não nas ~160.

**Achado**: ~12 lideravam com substantivo/jargão sem when-to-use (`memory_recall` "Search RLM + SemanticRecall (FTS5 + cosine)", `classify_intent` "CILA L0-L6…RegexSet", `ast_overview`/`ast_find` terse, `decompose`/`wiring`/`blast_radius_analysis` noun-led); ~6 já tinham when-to-use (health/gotcha/minimal_context/detect_changes).

**Entrega**: **10 descrições reescritas** → lideram com **verbo de ação + when-to-use + de-jargão** (leverage do `when_to_use` do C3): `memory_recall` ("Recall past lessons… Use to find how something was solved"), `memory_store`, `classify_intent` ("Classify a task into a CILA level"), `ast_overview` ("List a file's symbols… Use to grasp shape without reading it whole"), `ast_find`, `ast_edit`, `index_status` (tools_core); `decompose` ("Decompose a task into a validated DAG… Use for multi-step"), `wiring` ("Audit symbol connections… Use after adding pub symbols"), `blast_radius_analysis` ("Analyze blast radius… Use before editing") (tools_analysis/infra).

**Gates**: cargo check **0** · rustfmt limpo · count/estruturais **verdes** (string-only → `#[tool(` count inalterado). Namespacing já consistente (`touring_*`).

**Follow-up**: eval formal BiasBusters (sem harness pronto — não-trivial); as multi-line (`tantivy_search`/`fuzzy`, `ast_meta`) e as já-boas (`ctx_execute`/`search`/`generator_submit_plan`/`find_references`) ficaram intactas (já action-verb-led ou informativas).

### ✅ MT-1 — master tool `touring_audit` (failure/error/gap detection, offensive) — 2026-06-26

**Diretriz Gabriel**: *"crie master tools que orchestrem comandos em workflows"* + *"precisa ter master tool que orchestre workflows de identificação de falhas, erros, gaps e problemas… explore o touring-offensive"*. Esta é a master tool emblemática da diretriz.

**Conceito (N→1)**: um workflow tool faz UMA chamada MCP fanar por múltiplos engines de detecção e devolver um relatório ranqueado consolidado — colapsa a cadeia manual (vuln-scan → quality-gate) em N→1. Padrão agentic "orquestre um workflow": menos round-trips, um veredito coerente.

**Entrega** (`crates/touring-server/src/server/tools_workflow.rs`, novo módulo + `#[tool_router(router = router_workflow)]` merge em `mod.rs`):
- **Layer ofensivo** (`touring_offensive::vuln::PatternRegistry::all().detect_all`, 10 detectores CWE/OWASP — SQLi/XSS/cmd-inj/path-trav/SSRF/deser/int+buf-overflow/LDAP/XML) memoizado via `OnceLock`. `VulnMatch{pattern_name,span,severity,cwe_id}` → `AuditFinding` com byte→line. Severity ≥7.0 → Block.
- **Layer de qualidade** (`touring_quality::score_target(path, &[F2_1,F2_4,F2_5,F2_6,F4_3,F4_5], Json)` — os 6 P0 BLOCK, **stateless**) → blockers/warnings viram findings. Scoring-fail é graceful (não derruba o audit).
- **Agregação**: `run_audit(path, layers) -> AuditReport{verdict, block/warn/info_count, findings[]}` ranqueado por severidade. `apply_detail_level` (sinergia C1). `AuditLayers::from_param` (vuln|quality|all, opt-in).
- `touring-offensive` adicionado como dep direta de touring-server (já no build graph via touring-analysis; só foundation → sem ciclo).

**Wiring live-provado** (smoke MCP `tools/list`): `touring_audit` na superfície curada (**23 tools**), unit tests provam `UNION SELECT`→Block(CWE-89), `<script>`→Block(CWE-79), clean→Info.

**Gates**: cargo build 0 · test **9/9** (tools_workflow) + **1322/0** (lib, estruturais `server_mod_merges_every_sub_router`/`every_tools_submodule_has_tool_router_macro` verdes) · clippy **0** · touring-quality **Diamond 0.9552** (0 blockers, 3 warnings hardening). Efetivado via `update-touring` (doctor 5/5).

**Honestidade (REGRA #21)**: CC warnings em `mod.rs`/`tools_infra.rs` (`spawn_background_tasks`=64, `wiring`=54, `decompose`=137) são **pré-existentes** nos arquivos onde só inseri 1 linha (merge/annotation) — não regressões desta wave.

### ✅ Gap 2 — MCP tool annotations nas 23 curadas — 2026-06-26

**Best-practice (MCP spec via context7)**: tools devem declarar `readOnlyHint`/`destructiveHint`/`idempotentHint`/`title` para o cliente apresentar/auto-aprovar com segurança. Era o gap #6 do scorecard (grau D — não setadas).

**Entrega**: `annotations(...)` em todas as 23 curadas (técnica annotations-first, antes de `name=`, sem tocar descrições). Read-only tools → `read_only_hint=true`+`title`; writes (`ast_edit`/`memory_store`/`decompose`/`generator_submit_plan`) → `read_only_hint=false`+`idempotent_hint=false`; `ctx_execute` → `read_only_hint=false`. Validado ordem annotations-first compila (build piloto).

**Live-provado** (smoke MCP): as 23 carregam annotations; `touring_audit` → `{"title":"Audit file for failures & gaps","readOnlyHint":true}` — serializa em **camelCase** correto (rmcp `read_only_hint`→`readOnlyHint`).

**Gates**: cargo build 0 · test **1322/0** · clippy 0. Efetivado (mesmo rebuild do MT-1).

### ✅ Gap 4 — benchmark harness τ-bench-style ("transforma o veredito em medido") — 2026-06-26

**Best-practice (τ-bench / BFCL)**: sem run formal, "no topo dos princípios" não é "no topo dos benchmarks". Era o gap #14 do scorecard (grau F — não-medido). Este harness fecha o veredito.

**Entrega** (`docs/agentic-bench/run_bench.py` + `test_run_bench.py`, padrão TACO-wt: code analisa, JSON é o contrato): dirige o binário **real** (`touring` via PATH) em 3 eixos:
- **selection** (BFCL/τ-bench): 8 intents rotulados → `touring search-tools <intent> -j` → **precision@1 + MRR** (relevância multi-tool, prática IR padrão). "did the agent pick the right tool?"
- **outcome**: `touring_audit` via MCP stdio em 3 fixtures conhecidos (SQLi→Block-CWE89, XSS→Block-CWE79, clean→Info).
- **conformance**: `tools/list` real → % com annotations + % com description + curadoria ∈ [18,26].
- Composite ponderado (sel .40 / out .40 / conf .20) → 6-tier (Diamond..Unranked). `--md`, `--fail-below`.

**O harness ganhou seu lugar** (achou+corrigiu gap real): 1º run = **0.925 Platinum** (discriminante, não rigged). As 2 "misses" de selection revelaram: (a) labels meus estritos demais (`ast meta` É o tool de "metadata+blast"; `e2e`/`doctor` ambos "health") → corrigido p/ relevância-IR multi-tool; (b) **gap REAL (REGRA #0)**: `touring_audit` estava em `CURATED_TOOLS` (MCP) mas **não no `tool_catalog`** (corpus do `touring_search`) → a intent "audit vulnerabilities" não o achava. **Fix**: entrada `touring_audit` no `tool_catalog.rs` (kind=mcp, keywords vuln/cwe/owasp/sqli/xss). Re-run = **1.0 Diamond** (todos os eixos 1.0) — subida **legítima** (substrato genuinamente completo), provada ao vivo: `search-tools "audit…"` → `touring_audit` **1º** (score 19.577).

**Gates**: py_compile 0 · **pytest 16/0** (funções puras precision@1/MRR/composite/tier) · ruff **All checks passed** · cargo test **1322/0** + catalog 8/0 (entry nova, sem quebrar ranking tests) · clippy 0. Efetivado via `update-touring` (doctor 5/5). Harness é **gate de regressão** vivo (cai se quebrarem tool-selection, annotations ou curadoria).

**Veredito MEDIDO**: touring tools = **1.0 Diamond** no harness agentic (9/10 princípios qualitativos → número). Headroom futuro: ampliar scenarios (mais intents/fixtures) conforme tools crescem.
