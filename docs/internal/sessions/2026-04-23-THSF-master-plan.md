# Touring Holonic Symbiosis Framework (THSF) — Plano Mestre

**Documento**: Plano executável completo para implementar simbiose bidirecional
entre o workspace Touring e todos os projetos em `/home/gabrielgadea/projects/`.

**Versão**: 1.0.0
**Data**: 2026-04-23
**Autor**: Gabriel Gadea (autoridade) + Claude Code (execução)
**Status**: DRAFT — aguardando aprovação para Fase 0

---

## 1. Objective

### 1.1 Motivação

Após 6 sessões de pesquisa e síntese, consolidou-se um paradigma concreto —
**Holonic Symbiosis Framework (HSF)** — que permite projetos autônomos se
descobrirem, negociarem capabilities, se acoplarem temporariamente e se
desacoplarem, sem broker central, sem daemon obrigatório, sem lock-in a LLMs
ou a Anthropic/MCP.

### 1.2 O que o plano entrega

1. **Baseline universal**: `.holon/manifest.toml` em todos os 28 holons
   (20 crates Touring + 8 projetos de código).
2. **Self-symbiosis do Touring**: o próprio Touring participa como holon
   que oferece suas 120+ capabilities CLI + 88 MCP tools como capabilities
   declarativas discoverable via filesystem.
3. **Escalonamento por combo**: cada um dos 7 combos arquiteturais é
   aplicado onde faz sentido real, sem forçar adoção uniforme.
4. **Reversibilidade total**: `rm -rf */.holon/` desfaz 100% — nenhum projeto
   importa código THSF em produção.
5. **Documento vivo**: este `.md` serve de referência operacional, atualizado
   conforme fases avançam.

### 1.3 Success criteria (measurable)

| Métrica | Meta | Verificação |
|---|---|---|
| Holons com manifest válido | 28/28 | `holon discover | wc -l` |
| Builds originais intactos | 100% | CI matrix em cada projeto |
| Capability trocas/dia (após Fase 2) | ≥ 10 | `holon stats --last 24h` |
| Redução duplicação de código cross-project | ≥ 15% | medido via `touring wiring chains` |
| Tempo de discovery de peers | < 200ms para 28 holons | benchmark em Fase 1.5 |
| Regressões introduzidas | 0 | test suites + `touring e2e -j` |

### 1.4 Out of scope

- Reescrever código existente dos projetos.
- Forçar qualquer projeto a depender do Touring em runtime de produção.
- Substituir ferramentas já em uso (pytest, cargo, npm continuam).
- Implementar todos os 7 combos em todos os 28 holons (seletivo por valor).

---

## 2. Scope — Holon Population Mapping

### 2.1 Touring workspace (20 crates em `/home/gabrielgadea/.claude/rust/crates/`)

| Crate | Função | Capabilities a OFERECER | Capabilities a CONSUMIR |
|---|---|---|---|
| **touring-ast** | Tree-sitter + syn semantic | `symbol-extraction`, `rust-semantic`, `ast-grep-polyglot` | — |
| **touring-ast-polyglot** | Multi-lang AST | `polyglot-parser` | `symbol-extraction` |
| **touring-analysis** | Quality metrics + Halstead + MI | `quality-gate`, `complexity-metrics` | `rust-semantic` |
| **touring-antt** | Domain-specific rerankers | `domain-reranker` | `quality-gate` |
| **touring-cognitive** | MCTS + Pheromone | `mcts-planner`, `graph-mcts` | `symbol-index`, `blast-radius` |
| **touring-core** | Shared primitives + embeddings | `embedding-service`, `u4-quantization` | — |
| **touring-cortex** | Hook runtime orchestrator | `hook-dispatch`, `circuit-breaker` | todas outras |
| **touring-generator** | Code generation pipeline (30 kinds) | `code-generation`, `vgp-verification`, `template-render` | `symbol-index`, `quality-gate` |
| **touring-hooks** | 153 hooks registry | `pre-edit-gate`, `post-edit-quality`, `health-delta` | `symbol-index` |
| **touring-index** | Symbol store + SCIP | `symbol-index`, `scip-export` | — |
| **touring-integration-tests** | E2E | `e2e-test-harness` | todas |
| **touring-learning** | LinUCB + RL | `linucb-tuner`, `reward-ingest` | — |
| **touring-loom-proofs** | Concurrency proofs | `concurrency-invariant` | — |
| **touring-offensive** | Red-team / stress | `stress-harness` | — |
| **touring-python** | PyO3 bindings | `python-bridge` | todas core |
| **touring-rkyv** | Zero-copy IPC | `rkyv-ipc`, `zero-copy-serde` | — |
| **touring-server** | CLI + daemon | `daemon-health`, `cli-dispatch` | todas |
| **touring-simd** | SIMD + GPU WGSL | `simd-backend`, `gpu-compute`, `u4-dot-product` | — |
| **touring-telemetry** | OTEL + metrics | `observability`, `trace-propagation`, `gate-metrics` | — |
| **touring-wasm** | WASM runtime + inferlets | `wasm-sandbox`, `inferlets` | — |
| **inferlets** | WASM plugin pool | `inferlet-registry` | `wasm-sandbox` |

**Total de capabilities Touring oferecíveis**: ~40 únicas.

### 2.2 Projetos em `/home/gabrielgadea/projects/` (8 com código)

| Projeto | Tamanho | Stack | Capabilities a OFERECER | Capabilities a CONSUMIR |
|---|---|---|---|---|
| **analise** | 112G | Python + Rust + Frontend | `evtea-model`, `sicro-parser`, `traffic-graph`, `hdm4-runner`, `monte-carlo-stochastic` | `simd-backend`, `symbol-index`, `memory-recall`, `quality-gate` |
| **transferegov_pipeline** | 67G | Python + JS | `transferegov-schema`, `etl-pipeline`, `compliance-validator` | `symbol-index`, `quality-gate`, `observability` |
| **claude-trading** | 37G | Python + Rust | `trading-strategy`, `market-simulator`, `backtester` | `mcts-planner`, `linucb-tuner`, `simd-backend` |
| **konverter** | 11G | Python + JS + Rust (portal+backend) | `file-conversion`, `portal-api`, `document-parser` | `symbol-index`, `quality-gate` |
| **claude-code-kazuba** | 690M | Python | `kazuba-agent`, `claude-orchestrator` | `memory-recall`, `hook-dispatch` |
| **kazuba-cargo** | 652M | Rust + Python (pyo3) | `kazuba-rust-core`, `kazuba-pyo3-bridge` | `symbol-index`, `rust-semantic` |
| **nautilus-reference** | 168M | Rust + Python | `nautilus-ref-port` | `simd-backend`, `quality-gate` |
| **tools/qgis-mcp** | 50M | Python | `qgis-tools` | `memory-recall` |

**Total de capabilities de projetos oferecíveis**: ~18 únicas.

### 2.3 Excluídos do scope inicial

- `/home/gabrielgadea/projects/data/` — diretório de dados (não código)
- `/home/gabrielgadea/projects/-home-gabrielgadea/` — diretório reservado
- `/home/gabrielgadea/projects/or-ptah-de-dabar/` — projeto experimental pequeno

Podem ser adicionados em Fase 8 se valor for identificado.

### 2.4 Resumo quantitativo

- **28 holons** na holarchy (20 crates + 8 projetos)
- **~58 capabilities únicas** no ecosystem (40 Touring + 18 projetos)
- **Total edges potenciais no grafo de symbiosis**: ~200 (cada consumer ×
  cada provider compatível)
- **Linguagens representadas**: Rust, Python, TypeScript, JavaScript
- **Domínios representados**: code intelligence (Touring), geo-engineering
  (analise), trading (claude-trading), government pipelines (transferegov),
  file conversion (konverter), AI orchestration (kazuba, claude-code-kazuba)

---

## 3. 7 Combos — Taxonomy Aplicada

### 3.1 Recapitulação dos combos

| Combo | Papers | Topologias | Aplicabilidade | Custo |
|---|---|---|---|---|
| **A** Stigmergic FS Holon | P1+P3 | T1+T4 | Universal [0.95] | XS |
| **B** OCapN Symbiosis | P3+P5 | T3+T4 | Research [0.60] | L |
| **C** WASM Woven Holarchy | P2+P3 | T5+T1 | Polyglot [0.85] | XL |
| **D** P2P Knowledge Holarchy | P5+P1 | T6+T4 | Multi-host [0.50] | L |
| **E** Typed Federation | P3+P6 | T2+T7 | Perf crítico [0.75] | L |
| **F** Hybrid FS+WASM+CRDT | all papers | T1+T4+T5 | Evolução [0.80] | M |
| **G** Layered Polyglot Stack | meta | meta | Spec pública [0.70] | M |

### 3.2 Mapa combo → holons

| Combo | Holons elegíveis | Justificativa |
|---|---|---|
| **A (baseline)** | **TODOS 28** | Universal; zero custo |
| **B (OCapN)** | 1-2 (spike research) | High learning curve; Guile/Racket |
| **C (WASM)** | konverter, claude-trading, touring-wasm, inferlets | Polyglot real + já há WASM infra |
| **D (libp2p)** | Deferred | Só após Gabriel ter multi-host |
| **E (Cap'n Proto)** | touring-server ⇄ analise, touring-simd ⇄ claude-trading | Perf crítico; schemas maduros |
| **F (Hybrid)** | touring-hooks ⇄ analise (knowledge sync) | Learning loop bidirectional |
| **G (Spec)** | Documento em `docs/thsf/` | Publicação final como padrão |

### 3.3 Princípio de aplicação

- **Combo A é obrigatório** para todos os 28 holons — baseline universal.
- **Combos B-G são opt-in** apenas quando valor marginal > custo marginal.
- **Nenhum combo substitui outro** — eles coexistem em camadas.

---

## 4. Per-Holon Assignment Matrix

Matriz completa indicando quais combos cada holon adota, por fase:

### 4.1 Touring workspace crates

| Crate | Fase 1 (A) | Fase 3 (E) | Fase 4 (C) | Fase 5 (F) |
|---|---|---|---|---|
| touring-ast | ✓ | — | — | — |
| touring-ast-polyglot | ✓ | — | ✓ consumer | — |
| touring-analysis | ✓ | — | — | ✓ health-delta |
| touring-antt | ✓ | — | — | — |
| touring-cognitive | ✓ | ✓ server | — | — |
| touring-core | ✓ | — | — | — |
| touring-cortex | ✓ | — | — | ✓ hook registry |
| touring-generator | ✓ | — | ✓ provider | ✓ generator-health |
| touring-hooks | ✓ | — | — | ✓ 153 hooks |
| touring-index | ✓ | ✓ server | — | — |
| touring-integration-tests | ✓ | — | — | — |
| touring-learning | ✓ | — | — | ✓ RL feedback |
| touring-loom-proofs | ✓ | — | — | — |
| touring-offensive | ✓ | — | — | — |
| touring-python | ✓ | — | — | — |
| touring-rkyv | ✓ | ✓ transport | — | — |
| touring-server | ✓ | ✓ **master** | — | — |
| touring-simd | ✓ | ✓ server | ✓ WGSL exports | — |
| touring-telemetry | ✓ | — | — | ✓ OTEL cross |
| touring-wasm | ✓ | — | ✓ **master** | — |
| inferlets | ✓ | — | ✓ registry | — |

### 4.2 Projetos

| Projeto | Fase 1 (A) | Fase 3 (E) | Fase 4 (C) | Fase 5 (F) | Notas |
|---|---|---|---|---|---|
| analise | ✓ | ✓ consumer | — | ✓ pair com touring-hooks | Piloto Fase 5 |
| transferegov_pipeline | ✓ | — | — | — | Manifest only |
| claude-trading | ✓ | ✓ consumer | ✓ consumer | — | Piloto Fase 4 |
| konverter | ✓ | — | ✓ consumer | — | Piloto Fase 4 |
| claude-code-kazuba | ✓ | — | — | ✓ memory sync | — |
| kazuba-cargo | ✓ | ✓ consumer | — | — | Rust+pyo3 |
| nautilus-reference | ✓ | — | — | — | Reference only |
| tools/qgis-mcp | ✓ | — | — | — | Manifest only |

---

## 5. Timeline Detalhada (Fases 0-8)

### Fase 0 — Foundations (1-2 dias, T-shirt: S)

**Goal**: `holon.py` instalado como CLI universal + schema canônico.

**Deliverables atômicos**:
- **D0.1** [S]: Empacotar `holon.py` (já escrito em sessão anterior) como
  script executável em `/home/gabrielgadea/.claude/tools/holon/holon`
  com shebang `#!/usr/bin/env python3`.
- **D0.2** [S]: `holon-manifest.schema.json` (JSON Schema Draft 2020-12)
  validando estrutura do manifest TOML. Referenciado por cada manifest
  via comment `# schema: ...`.
- **D0.3** [XS]: Adicionar `/home/gabrielgadea/.claude/tools/holon/` ao
  `$PATH` via `~/.bashrc` (uma linha).
- **D0.4** [XS]: Comando `holon init <dir>` scaffolda `.holon/manifest.toml`
  + `.holon/schemas/` vazios.
- **D0.5** [S]: Primeiro E2E test: `holon init /tmp/test1 && holon init
  /tmp/test2 && holon symbiosis /tmp` prova pipeline.

**Exit criteria**:
```bash
which holon                                     # retorna path
holon --help                                    # mostra subcomandos
holon init /tmp/demo && ls /tmp/demo/.holon/    # scaffolds visible
pytest /home/gabrielgadea/.claude/tools/holon/  # 15 tests PASS
```

**Dependências**: nenhuma (arranque).

---

### Fase 1 — COMBO A em toda a holarchy (1 semana, T-shirt: L)

**Goal**: 28 manifests TOML válidos + discovery funcional.

**Sub-fase 1.1 — Touring crates** (2 dias, paralelo):

Para cada um dos 20 crates em `/home/gabrielgadea/.claude/rust/crates/`:

```bash
cd /home/gabrielgadea/.claude/rust/crates/<crate>/
mkdir -p .holon/schemas
# Copy template manifest, customize name + offers + requires
```

Template por crate (exemplo `touring-ast`):

```toml
# .holon/manifest.toml
# schema: /home/gabrielgadea/.claude/tools/holon/holon-manifest.schema.json

[holon.identity]
name = "touring-ast"
version = "30.3.0"
description = "Tree-sitter + syn semantic analysis"

[holon.offers.symbol-extraction]
schema = "schemas/symbol-extraction.json"
adapter = "cli"
adapter_cmd = "touring ast overview"

[holon.offers.rust-semantic]
schema = "schemas/rust-semantic.json"
adapter = "cli"
adapter_cmd = "touring ast rust-semantic"

[holon.offers.ast-grep-polyglot]
schema = "schemas/ast-grep.json"
adapter = "cli"
adapter_cmd = "touring ast grep"
```

- **D1.1a** [M]: 20 manifests Touring crates criados.
- **D1.1b** [M]: 20 × 1-3 schemas JSON (60 schemas totais; template-gerados).

**Sub-fase 1.2 — Projetos** (2 dias, paralelo):

Para cada projeto:

```bash
cd /home/gabrielgadea/projects/<project>/
holon init .
# Customize manifest: offers + requires conforme seção 2.2
```

- **D1.2a** [M]: 8 manifests de projeto.
- **D1.2b** [S]: Schemas iniciais (~18 schemas).

**Sub-fase 1.3 — Validação global** (1 dia):

- **D1.3** [S]: Script `validate-holarchy.sh` roda em CI:
  ```bash
  holon discover /home/gabrielgadea | wc -l
  # Expect: 28
  holon doctor /home/gabrielgadea
  # Expect: 0 errors
  ```

**Sub-fase 1.4 — Discovery + scheduled symbiosis** (1 dia):

- **D1.4** [S]: systemd user timer rodando `holon symbiosis` 1×/dia:
  ```ini
  [Unit]
  Description=Holon Symbiosis Daily Cycle

  [Timer]
  OnCalendar=daily
  Persistent=true

  [Install]
  WantedBy=timers.target
  ```
- **D1.5** [XS]: Log em `/home/gabrielgadea/.local/state/holon/symbiosis.log`.

**Exit criteria**:
```bash
holon discover /home/gabrielgadea | grep -c manifest.toml  # 28
holon symbiosis /home/gabrielgadea                          # JSON report
# Expect: handshakes_accepted >= 5, rejected = 0
```

**Dependências**: Fase 0 completa.

**Riscos**:
- **R1.1** [MED, HIGH → **MED mitigado**]: Manifest drift entre versões do
  projeto. **Mitigação**: hook `post-commit` valida + `holon doctor` no CI.
- **R1.2** [LOW, LOW]: `rglob` lento em repos gigantes (analise = 112G).
  **Mitigação**: usar `.git/index` como hint; limitar profundidade para 5.

---

### Fase 2 — Touring Self-Enrichment (1 semana, T-shirt: M)

**Goal**: Touring é um holon que participa do próprio ecosystem.

- **D2.1** [S]: Manifest master em `/home/gabrielgadea/.claude/rust/.holon/`
  expondo 10+ capabilities agregadas:
  ```toml
  [holon.identity]
  name = "touring-master"
  version = "30.3.0"

  [holon.offers.symbol-index]
  adapter_cmd = "touring index find"
  [holon.offers.blast-radius]
  adapter_cmd = "touring ast blast"
  [holon.offers.simd-backend]
  adapter_cmd = "touring inferlets run"
  [holon.offers.mcts-planner]
  adapter_cmd = "touring cognitive mcts"
  # ... 6 more
  ```

- **D2.2** [M]: Adapter bridge `holon-touring-adapter.py`:
  ```python
  def invoke_touring_capability(cap_name: str, args: dict) -> dict:
      cmd = CAPABILITY_REGISTRY[cap_name]
      return subprocess.check_output(cmd + args_to_argv(args))
  ```

- **D2.3** [M]: CLI hook em `touring` que registra cada invocação no
  CRDT store de `.holon/state.db`, permitindo learning loop observer.

- **D2.4** [S]: Dashboard: `holon status --touring` mostra quais
  capabilities Touring já foram consumidas por quais projetos
  (counter via CRDT GSet).

**Exit criteria**:
```bash
holon discover /home/gabrielgadea | grep touring-master  # exists
holon invoke touring-master symbol-index '{"symbol":"HolonSymbiosis"}'
# Returns JSON
```

**Dependências**: Fase 1 completa (todos manifests existem).

---

### Fase 3 — COMBO E (Cap'n Proto Typed Federation) — 2 semanas, XL

**Goal**: Trocas perf-crítico entre Touring ↔ analise / claude-trading /
kazuba-cargo via Cap'n Proto com promise pipelining.

- **D3.1** [M]: Schema unificado `holon-core.capnp` em
  `/home/gabrielgadea/.claude/rust/crates/touring-rkyv/schemas/`:
  ```capnp
  @0xabcdef1234567890;

  interface Holon {
    listCapabilities @0 () -> (caps :List(Capability));
    invoke @1 (name :Text, args :Data) -> (result :Data);
  }

  struct Capability {
    name @0 :Text;
    schemaHash @1 :Text;
    version @2 :Text;
  }
  ```

- **D3.2** [L]: `touring-capnp-server` crate novo:
  - Expõe Cap'n Proto server em `/run/user/$UID/holon/touring.sock`
  - Implementa `Holon` interface via delegação para `HookRuntime`
  - Tests: roundtrip latency < 1ms

- **D3.3** [L]: Clientes Python:
  - `holon_capnp_client.py` para analise (consumers `symbol-index`)
  - `holon_capnp_client.py` para claude-trading (consumer `mcts-planner`)

- **D3.4** [S]: Benchmark `bench_symbiosis.py`:
  ```
  fs-baseline:  12.3 ms/call
  capnp:         0.4 ms/call (30× speedup)
  mcp (ref):     8.1 ms/call (baseline)
  ```

**Exit criteria**:
- Latência p50 consumption `symbol-index` via capnp < 1ms
- 0 regressões nos builds originais

**Dependências**: Fase 2 completa.

**Riscos**:
- **R3.1** [MED, HIGH]: Schemas divergem. **Mitigação**: schemas em único
  crate `touring-rkyv/schemas/`, CI valida hash em todos os consumers.

---

### Fase 4 — COMBO C (WASM Woven Holarchy) — 3-4 semanas, XL

**Goal**: Capabilities Touring compilam para WASM components, usáveis por
qualquer linguagem via WIT.

**Pre-req**: WASI 0.3 stable (Nov/2025 — confirmar disponibilidade).

- **D4.1** [M]: WIT interfaces em `/home/gabrielgadea/.claude/rust/crates/touring-wasm/wit/`:
  ```wit
  package holon:core@0.1.0;

  interface capabilities {
    list: func() -> list<string>;
    invoke: func(name: string, args: list<u8>) -> result<list<u8>, string>;
  }

  world holon-component {
    export capabilities;
  }
  ```

- **D4.2** [L]: Compilar 3 capabilities Touring para WASM:
  - `symbol-index.wasm` (via touring-index)
  - `blast-radius.wasm` (via touring-ast)
  - `quality-gate.wasm` (via touring-analysis)

- **D4.3** [L]: Pilot em claude-trading:
  ```bash
  cd /home/gabrielgadea/projects/claude-trading
  wac plug touring-symbol-index.wasm trading-plug.wasm -o composed.wasm
  wasmtime serve composed.wasm
  ```

- **D4.4** [M]: Feature flag `holon-wasm` opt-in em `.holon/manifest.toml`:
  ```toml
  [holon.requires.symbol-index]
  transport = "wasm"  # default: "cli"
  wasm_component = "path/to/symbol-index.wasm"
  ```

**Exit criteria**:
- claude-trading consome `symbol-index` via WASM sandbox (isolado)
- Build passa em ambiente sem Touring daemon rodando

**Dependências**: Fase 1 (manifests); WASI 0.3 stable.

**Riscos**:
- **R4.1** [MED, HIGH]: WASI 0.3 atrasa. **Mitigação**: Fase 4 é
  opcional; Fases 1-3 entregam valor sem WASM.

---

### Fase 5 — COMBO F (Hybrid FS + CRDT + WASM) — 2 semanas, L

**Goal**: Knowledge sync bidirecional entre Touring e analise como piloto.

- **D5.1** [M]: Wrapper opcional Automerge (via `automerge-py` bindings)
  mantendo API idêntica ao CRDTStore (LWW + GSet):
  ```python
  class AutomergeCRDTStore(CRDTStore):  # Liskov-compatible
      def __init__(self, db_path, actor_id):
          self._doc = Automerge.load(db_path) if db_path.exists() else Automerge.from({})
  ```

- **D5.2** [M]: Knowledge pipeline Touring ↔ analise:
  - Touring exporta `lessons` + `patterns` + `rl_rewards` para CRDT
  - analise importa + cruza com `domain-ontology-evtea.json`
  - Convergence: ambos enxergam união dos dois knowledge graphs
  - Reward loop: analise sinaliza "X pattern Touring ajudou N vezes"

- **D5.3** [S]: Dashboard `holon status --crdt --pair=touring-master,analise`:
  ```
  Touring → analise:  1,247 lessons shared
  analise → Touring:    384 domain patterns shared
  Conflicts auto-merged: 0  (true CRDT)
  ```

**Exit criteria**:
- `touring memory recall "evtea traffic"` retorna hits cruzados
  com ontologia real de analise

**Dependências**: Fase 2 completa; analise + Touring com manifests.

---

### Fase 6 — COMBO B (Spritely Goblins OCapN) research spike — 1 semana, M

**Goal**: Avaliar se OCapN/Goblins vale adoção em produção.

- **D6.1** [M]: Instalação Guile 3.0 + Goblins + OCapN:
  ```bash
  guix install guile guile-goblins  # if NixOS/Guix available
  # or: compile from source
  ```

- **D6.2** [M]: POC — capability Touring exposta como OCapN object:
  ```scheme
  (define (make-touring-symbol-index-vow)
    (spawn ^symbol-index))
  ```

- **D6.3** [S]: Report técnico go/no-go:
  - Pros: máxima filosofia OCaps; time-traveling debugger
  - Cons: ecosystem pequeno; Guile/Racket learning curve
  - Decisão: **arquivar** se não trouxer valor incremental claro

**Exit criteria**: documento `docs/thsf-combo-b-decision.md` com verdict.

**Dependências**: Fases 1-2 estáveis (não bloqueia se deferred).

---

### Fase 7 — COMBO D (libp2p) deferred — M (quando aplicável)

**Goal**: Spec pronta para quando Gabriel tiver multi-host real.

- **D7.1** [M]: Documento `docs/thsf-combo-d-multihost-spec.md`:
  - Kademlia DHT bootstrap config
  - Peer ID generation via ed25519
  - Content routing para `.holon/` metadata
  - GossipSub para broadcast de capability updates

- **D7.2** [XS]: Dockerfile `holon-bootstrap-node` que qualquer host roda.

**Ativação**: somente quando Gabriel tiver 2+ hosts concomitantes.

**Dependências**: nenhuma — é deferred spec.

---

### Fase 8 — COMBO G (Layered Polyglot Stack) spec pública — 2 semanas, L

**Goal**: Publicar THSF como spec pública com templates reutilizáveis.

- **D8.1** [L]: Doc canônico `docs/thsf/THSF-SPEC-v1.0.md`:
  - 4 camadas (Discovery, Handshake, Capability Exchange, Knowledge Sync)
  - Matriz de opções por camada (7 topologias × 4 camadas)
  - Compatibilidade matrix + versionamento semântico

- **D8.2** [S]: RFC layout em `docs/thsf/rfcs/`:
  - RFC-001: Manifest schema canônico
  - RFC-002: Capability ID + versioning
  - RFC-003: CRDT semantics + merge protocol
  - RFC-004: WIT interfaces standard

- **D8.3** [S]: Template repos em `/home/gabrielgadea/projects/templates/`:
  - `holon-rust-template/` — Rust project com `.holon/` pré-configurado
  - `holon-python-template/` — Python project idem
  - `holon-ts-template/` — TypeScript + Node idem

**Exit criteria**: doc público, `git clone <template>` funciona standalone.

**Dependências**: Fases 1-5 concluídas (prova o framework em produção).

---

## 6. Dependencies DAG

```
Fase 0 (Foundations)
    │
    ▼
Fase 1 (COMBO A em todos 28 holons)
    │
    ├──────────────┬──────────────┬──────────────┐
    ▼              ▼              ▼              ▼
Fase 2         Fase 3         Fase 6         Fase 7
(Self-enrich)  (CapnP)        (Goblins       (libp2p
    │           │                research)    deferred)
    │           │
    ├───────────┴───┬──────────┐
    ▼               ▼          ▼
Fase 4 (WASM)  Fase 5 (CRDT)   ▼
    │               │          │
    └───────┬───────┘          │
            ▼                  ▼
        Fase 8 (Spec pública)
```

**Regras de escalonamento**:
- Fase 0 → 1: **sequencial obrigatória**
- Fase 1 → 2: **sequencial obrigatória**
- Fase 2 → {3,4,5,6}: **paralelas opcionais**
- Fase 7: deferred indefinidamente
- Fase 8: depois de ≥ 3 das fases {3,4,5} concluídas

---

## 7. Validation Plan

### 7.1 Validação por fase

Cada fase tem seu próprio exit criteria (seção 5). Adicionalmente:

- **Smoke test universal**: `holon symbiosis /home/gabrielgadea` deve
  retornar `handshakes_rejected: 0` após qualquer fase.
- **Regression gate**: `cargo check --workspace` + `pytest` + `npm build`
  em cada projeto devem continuar verdes.
- **Idempotência gate**: rodar qualquer fase N vezes produz mesmo resultado
  (testado via `test_symbiosis_run_is_idempotent`).

### 7.2 Validação de deliverables atomicamente

Cada deliverable [Dx.y] é atômico e independentemente shippable:
- Tem success criteria próprio
- Pode ser revertido sem afetar outros (`rm -rf */.holon/` é nuclear reset)
- Tem teste automatizado (pytest, shell assertion, ou CI check)

### 7.3 Self-validation do plano

Verifiquei ao escrever:
1. ✓ Cada deliverable é atômico e independentemente shippable
2. ✓ Dependências são explícitas (seção 6 DAG) e acíclicas
3. ✓ Estimates em T-shirt sizing (XS/S/M/L/XL) consistentes
4. ✓ Riscos têm mitigações (seção 8)

---

## 8. Risks Register

### 8.1 Riscos de execução

| ID | Risco | Prob | Impacto | Severidade | Mitigação |
|---|---|---|---|---|---|
| R0.1 | `holon.py` tem bug em produção | LOW | MED | LOW | 15 tests pytest já validam |
| R1.1 | Manifests desatualizados vs código | HIGH | MED | **HIGH** | Git hook post-commit + CI validation |
| R1.2 | `rglob` lento em analise (112G) | LOW | LOW | LOW | Max depth 5 + `.gitignore` respect |
| R1.3 | Conflitos CRDT concorrência alta | LOW | MED | LOW | WAL mode + Automerge fallback |
| R2.1 | Bridge Touring CLI tem race condition | MED | MED | MED | Integration tests sob concorrência |
| R3.1 | Schemas Cap'n Proto divergem entre projetos | MED | HIGH | **HIGH** | Schemas em 1 lugar + hash check CI |
| R3.2 | Cap'n Proto Rust ≠ Python compat | LOW | HIGH | MED | roundtrip test em ambas libs |
| R4.1 | WASI 0.3 atrasa stable release | MED | HIGH | **MED** | Fase 4 é opcional; não bloqueia |
| R4.2 | WASM component size > 10MB | MED | MED | MED | dead-code-elim + feature flags mínimas |
| R5.1 | Automerge dep adiciona lock-in | MED | LOW | LOW | Interface abstrata; SQLite default |
| R6.1 | Goblins research = dead end | HIGH | LOW | LOW | Time-boxed 1 semana; verdict documentado |
| R7.1 | libp2p NA por hardware | HIGH | LOW | LOW | Deferred; spec standalone |
| R8.1 | Spec pública sem adoção externa | MED | LOW | LOW | Doc interno tem valor independente |

### 8.2 Riscos sistêmicos

| ID | Risco | Prob | Impacto | Severidade | Mitigação |
|---|---|---|---|---|---|
| RS.1 | Gabriel perde interesse antes Fase 2 | MED | HIGH | MED | Fase 1 entrega valor isolado |
| RS.2 | Build externo quebra por side-effect | LOW | HIGH | HIGH | Invariante: nunca tocar fora de `.holon/` |
| RS.3 | MCP ecosystem torna-se irresistível | LOW | HIGH | MED | THSF acomoda MCP como Camada 3 opcional |
| RS.4 | Lock-in acidental ao `holon.py` | LOW | MED | LOW | `rm -rf */.holon/` + nenhum import |
| RS.5 | Documentação desatualizada | HIGH | MED | HIGH | Este `.md` é living document; review trimestral |

---

## 9. Custo total estimado

| Fase | T-shirt | Esforço (dias) | Cumulativo |
|---|---|---|---|
| Fase 0 | S | 1-2 | 1-2 |
| Fase 1 | L | 5-7 | 7-9 |
| Fase 2 | M | 5 | 12-14 |
| Fase 3 | XL | 10-14 | 22-28 |
| Fase 4 | XL | 15-20 | 37-48 |
| Fase 5 | L | 10 | 47-58 |
| Fase 6 | M | 5 | 52-63 |
| Fase 7 | M | deferred | n/a |
| Fase 8 | L | 10 | 62-73 |

**Esforço total (Fases 0-6+8)**: **~60-75 dias-pessoa** de trabalho focado.

**Esforço para MVP (Fases 0-2)**: **~12-14 dias** — entrega valor substancial.

**Esforço para Fase 1 sozinha (baseline universal)**: **~1 semana** —
valor imediato palpável.

---

## 10. Appendices

### A. Schema canônico `.holon/manifest.toml`

```toml
# schema: /home/gabrielgadea/.claude/tools/holon/holon-manifest.schema.json

[holon.identity]
name = "<project-or-crate-name>"          # unique within holarchy
version = "X.Y.Z"                          # semver
description = "<one-line>"                 # optional
autonomy_guarantee = true                  # build passes without holon

[holon.offers.<capability-name>]
schema = "schemas/<capability>.json"       # JSON Schema
adapter = "cli" | "capnp" | "wasm"         # transport
adapter_cmd = "<executable>"                # for adapter=cli
capnp_socket = "$XDG_RUNTIME_DIR/..."       # for adapter=capnp
wasm_component = "path/to/component.wasm"   # for adapter=wasm
version = "X.Y.Z"                           # capability version

[holon.requires.<capability-name>]
optional = true | false                     # fallback allowed?
fallback = "<string>"                       # what to use if unavailable
min_version = "X.Y.Z"                       # minimum acceptable version

[holon.mediator]                            # optional
observability = "otlp" | "stdout"
log_path = "<abs-path>"
```

### B. JSON Schema canônico (resumo)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "holon-manifest.schema.json",
  "type": "object",
  "required": ["holon"],
  "properties": {
    "holon": {
      "type": "object",
      "required": ["identity"],
      "properties": {
        "identity": { "...": "..." },
        "offers": { "...": "..." },
        "requires": { "...": "..." }
      }
    }
  }
}
```

Schema completo será gerado em Fase 0 D0.2.

### C. Exemplos concretos de manifests (3 representativos)

#### C.1 Touring crate — `touring-index`

```toml
[holon.identity]
name = "touring-index"
version = "30.3.0"
description = "Symbol store + SCIP export for code intelligence"

[holon.offers.symbol-index]
schema = "schemas/symbol-index.json"
adapter = "cli"
adapter_cmd = "touring index find"

[holon.offers.scip-export]
schema = "schemas/scip.json"
adapter = "cli"
adapter_cmd = "touring scip export"

[holon.offers.prefix-search]
schema = "schemas/prefix-search.json"
adapter = "cli"
adapter_cmd = "touring index search"
```

#### C.2 Projeto — `analise` (kazuba-geo-engine)

```toml
[holon.identity]
name = "kazuba-geo-engine"
version = "1.0.0"
description = "EVTEA modeling for road concessions"
autonomy_guarantee = true

[holon.offers.monte-carlo-stochastic]
schema = "schemas/monte-carlo.json"
adapter = "cli"
adapter_cmd = "python -m kazuba_geo_engine.mef.monte_carlo"

[holon.offers.sicro-parser]
schema = "schemas/sicro.json"
adapter = "cli"
adapter_cmd = "python -m kazuba_geo_engine.mef.parsers.sicro"

[holon.offers.traffic-graph]
schema = "schemas/traffic-graph.json"
adapter = "cli"
adapter_cmd = "python -m kazuba_geo_engine.mef.traffic"

[holon.requires.simd-backend]
optional = true
fallback = "native"
min_version = "0.1.0"

[holon.requires.symbol-index]
optional = true

[holon.requires.quality-gate]
optional = true
```

#### C.3 Touring-master (Fase 2)

```toml
[holon.identity]
name = "touring-master"
version = "30.3.0"
description = "Unified Touring capabilities aggregator"

[holon.offers.symbol-index]
schema = "schemas/symbol-index.json"
adapter = "cli"
adapter_cmd = "touring index find"

[holon.offers.blast-radius]
schema = "schemas/blast-radius.json"
adapter = "cli"
adapter_cmd = "touring ast blast"

[holon.offers.simd-backend]
schema = "schemas/simd.json"
adapter = "cli"
adapter_cmd = "touring inferlets run"

[holon.offers.mcts-planner]
schema = "schemas/mcts.json"
adapter = "cli"
adapter_cmd = "touring mcts plan"

[holon.offers.linucb-tuner]
schema = "schemas/linucb.json"
adapter = "cli"
adapter_cmd = "touring learning reward"

[holon.offers.generator-pipeline]
schema = "schemas/generator.json"
adapter = "cli"
adapter_cmd = "touring generate plan-submit"

[holon.offers.health-delta]
schema = "schemas/health-delta.json"
adapter = "cli"
adapter_cmd = "touring health-delta status"

[holon.offers.memory-recall]
schema = "schemas/memory.json"
adapter = "cli"
adapter_cmd = "touring memory recall"

[holon.offers.wiring-audit]
schema = "schemas/wiring.json"
adapter = "cli"
adapter_cmd = "touring wiring audit"

[holon.offers.ast-semantic]
schema = "schemas/ast-semantic.json"
adapter = "cli"
adapter_cmd = "touring ast rust-semantic"
```

### D. Cronograma semanal (primeiras 4 semanas)

| Semana | Foco | Deliverables-chave |
|---|---|---|
| **1** | Fase 0 + Fase 1.1 | `holon` CLI + 20 manifests Touring |
| **2** | Fase 1.2 + 1.3 + 1.4 | 8 manifests projetos + CI + systemd |
| **3** | Fase 2 | Self-enrichment Touring + bridge |
| **4** | Fase 3 (início) | Cap'n Proto schemas + server |

### E. Comandos operacionais

```bash
# Discovery
holon discover /home/gabrielgadea

# Symbiosis cycle
holon symbiosis /home/gabrielgadea

# Validation
holon doctor /home/gabrielgadea

# Stats
holon stats --since=24h

# Specific capability invocation
holon invoke touring-master symbol-index '{"symbol":"HolonSymbiosis"}'

# Reset (nuclear)
find /home/gabrielgadea -type d -name .holon -exec rm -rf {} +

# Touring integration
touring status -j | jq '.holon_integration'  # pós-Fase 2
```

### F. Arquivos-chave deste plano

- **Plano mestre**: `/home/gabrielgadea/.claude/rust/docs/2026-04-23-THSF-master-plan.md` (este documento)
- **holon.py**: `/home/gabrielgadea/.claude/tools/holon/holon.py` (Fase 0)
- **Schema manifest**: `/home/gabrielgadea/.claude/tools/holon/holon-manifest.schema.json` (Fase 0)
- **Spec pública**: `/home/gabrielgadea/.claude/rust/docs/thsf/THSF-SPEC-v1.0.md` (Fase 8)

---

## 11. Aprovação e próximos passos

### 11.1 Checklist de aprovação para arranque

Antes de iniciar Fase 0, Gabriel deve confirmar:
- [ ] Leu seções 1-3 (objectives + scope + taxonomia)
- [ ] Está confortável com arquitetura filesystem-based (não MCP)
- [ ] Aceita esforço estimado (12-14 dias para MVP Fases 0-2)
- [ ] Autoriza criação de `.holon/` em todos os 28 holons
- [ ] Autoriza registrar cron/systemd-timer para symbiosis diária
- [ ] Designou slot para review semanal de progresso

### 11.2 Próximo passo imediato

**Comando único para iniciar**:
```bash
mkdir -p /home/gabrielgadea/.claude/tools/holon
# Próximo: copiar holon.py da sessão anterior + tests
```

**Aprovação**: aguardando Gabriel responder "GO Fase 0" ou modificações.

---

## 12. Meta — sobre este documento

- **Living document**: atualizado conforme fases avançam.
- **Versionamento**: este `.md` é versionado (v1.0.0 inicial).
- **Changelog**: seção a ser adicionada após primeira revisão.
- **Questões em aberto**:
  - Fase 4 WASI 0.3 stable disponibilidade (Nov/2025 esperado).
  - Fase 6 Goblins — valor real versus custo research.
  - Fase 8 publicação — pública no GitHub ou apenas interna?

**Aprovação esperada**: Gabriel Gadea.
**Última revisão**: 2026-04-23 — autor AI em sessão `Skill("Touring")`.
**Próxima revisão**: após Fase 1 completar.

---

*Fim do documento — Plano Mestre THSF v1.0.0*
