# Insights do "LLM Agentic Harness Architecture" → Touring

> **Análise profunda** de `~/.claude/downloads/LLM Agentic Harness Architecture.md` (síntese de
> pesquisa 2025/2026, 30 refs arXiv) extraindo o que é **acionável para o Touring**.
> **Data**: 2026-06-26 | **Sessão**: `e0f553d0` | **Autor**: TACO (Opus 4.8 1M) p/ Gabriel Gadea
> **Companheiro**: `2026-06-26-touring-llm-coupling-strategy.md` (§9 referencia este doc)
> **Método**: leitura integral + VGP (grep no código antes de afirmar "tem/não tem") + sequential-thinking.
> Convenção: `[FACT]` verificado (código/medição/doc) · `[INFER 0.7-0.9]` derivado · `[SPEC <0.7]` hipótese.

---

## 0. A tese central — e por que importa para nós

O doc formaliza, com 30 papers, **exatamente a virada do nosso coupling-strategy**:

> *"transição da **indução semântica** baseada em instruções textuais densas ('pushing the model')
> para a **coerção estrutural** baseada em restrições físicas de ambiente ('pulling the model
> through paths of least resistance')."* — §1

`[FACT]` Isto é a tese de Gabriel ("a LLM não tem prior para a infra de código") + o nosso princípio
**P1 (afordância > indução)**, agora com **respaldo empírico de terceiros**. Dois números matam a
dúvida sobre "vale investir no harness em vez de esperar modelo melhor":

- **Harness > modelo**: mudar só a infra/formato de ferramentas move **+22 pontos** no SWE-bench Pro
  com o **mesmo modelo**; *Confucius* (Sonnet 3.5 + scaffolding) **52,7% > Opus 3.5 no harness padrão
  52,0%**. `[FACT, §2]`
- **Formato da ferramenta de edição**: block-edit (Grok Code Fast) levou SWE-bench de **6,7% → 68,3%**
  sem tocar nos pesos. `[FACT, §2]`

→ **Conclusão para o Touring**: o trabalho de coupling não é polimento — é a maior alavanca disponível.
O doc adiciona **5 mecanismos acionáveis** que o nosso plano ainda não tinha (ou tinha vago).

---

## 1. Mapa mestre — conceito do doc × estado do Touring × ação

| # | Conceito do doc | Touring tem? | Gap / Oportunidade | Pri |
|---|---|---|---|---|
| **I1** | **Active Summarization** (interceptor Rust: stdout >4KB → JSON <200 tok via regex/AST) | **NÃO** `[FACT]` (grep vazio em `gateway/`) | Generalizar o `slim_large_arrays` de hoje p/ camada CEG universal | **P0** |
| **I2** | **DADL / Code Mode** (2 meta-tools `list_tools`+`execute_code`; 142K→1K tok) | **Parcial** `[FACT]` (`ctx_execute_tools.rs` + `inferlets/` existem; MCP ainda expõe 160 atômicas) | Expor MCP como ~2 meta-tools sobre a CLI, não curar p/ 15 | **P1** |
| **I3** | **RGAO router** (vetor `c∈ℝ⁵` → FASTPATH/SUBAGENT/MULTIAGENT/DEEPRESEARCH; erro 30,1%→8,2%) | **Parcial** `[INFER]` (CILA L0-L5 existe heurístico; sem vetor medido) | Computar `c` via `ast blast`/`wiring`/`index` → rotear nível TACO deterministicamente | **P1** |
| **I4** | **Class D silent failure** (modelo narra sucesso, mascara erro real) | **NÃO** detector `[INFER]`; só REGRA #21 + real-exit-codes | Detector narrativa-vs-exit no CEG X9 + transcript miner | **P1** |
| **I5** | **WarpGrep** (busca estruturada: +3,7pp, −17% tok, −28% tempo) | **SIM** `[FACT]` (`index find`/`tantivy`) — subusado | Munição p/ o redirect grep→`index find` do cli-suggest | **P0** |
| I6 | **Budget conservation** (`Σ Bᵥ ≤ B_root`, verificado O(\|V\|+\|E\|)) | **NÃO** no decompose DAG `[INFER]`; o Workflow-tool tem `budget` | Contrato de orçamento por nó do `decompose` | P2 |
| I7 | **Crab** (eBPF infere turnos com side-effect → checkpoint só então; 87% economizados) | **Parcial** `[FACT]` (`ebpf-telemetry` feat. + checkpoints) | Checkpoint seletivo guiado por side-effect do CEG X0 | P2 |
| I8 | **CA-MCP / Shared Context Store** (agents coordenam sem round-trip LLM) | **Parcial** `[INFER]` (daemon + `decompose` + `memory.db`) | Daemon como SCS: subagents leem/escrevem estado sem o orquestrador | P2 |
| I9 | **Semantic consistency mesh** (GED+cosine arbitra merge de agents paralelos) | **NÃO** `[INFER]`; tem AST+embeddings (peças) | Gate de divergência AST entre engineers paralelos (FASE 6) | P2 |
| I10 | **Tokenization gateway** (placeholders de credencial, Vault, LLM nunca vê secret) | **Parcial** `[FACT]` (CEG `Env(KeyScope)`+ALLOWLIST nega; sem substituição reversa) | Placeholder→secret na gateway p/ comandos que precisam de chave | P3 |

---

## 2. Top insights acionáveis (detalhados)

### I1 — Active Output Summarizer no CEG **[P0, o maior ROI]**

`[FACT]` **O doc descreve exatamente a nossa doença e a cura na mesma frase** (§3):

> *"Se o buffer exceder limites (ex. 4 KB), o harness extrai programaticamente as assinaturas de erro,
> os ficheiros afetados e as linhas críticas usando regex ou parsing de AST, compactando a resposta
> num objeto JSON denso <200 tokens antes da reinjeção."*

`[FACT]` O CEG tem X0 (CAPTURE) e X9, mas **não há camada de sumarização** — `grep` por
`summariz|truncat|compress_output` em `gateway/`+`capability/` retorna **vazio**. O `slim_large_arrays`
que entreguei hoje (status wiring 59K→384B) é **um caso particular hardcoded** dessa ideia.

**Proposta concreta**: novo estágio **X8.5 SUMMARIZE** (ou enriquecer X9) em
`crates/touring-hooks/src/gateway/` — um `OutputSummarizer` que, quando `stdout/stderr > N KB`:
1. Detecta o tipo (JSON / cargo / pytest / log) por sniff.
2. JSON → reusa `slim_large_arrays` generalizado (elide arrays grandes → counts).
3. cargo/linter → extrai `^error[...]`, `file:line`, contagem (o nosso VP-Scout Cadeia 5 já sabe o padrão).
4. Emite `{summary, omitted_bytes, full_ref}` <200 tok; o full fica acessível sob demanda (file ref).

**Por que P0**: ataca a **raiz** da verbosidade (status 70K, orphans 173K, `cargo` dumps) de uma vez,
em vez de caçar caso a caso. Resolve o `C(tokens)` da economia `U(a)` de forma sistêmica. É a
generalização natural do trabalho de hoje. `[INFER 0.9]`

### I2 — MCP como 2 meta-tools (DADL), não 160 atômicas nem "curado 15"

`[FACT]` O doc (§3, §5) mostra **142× de redução** (142K→1K tok) ao expor um catálogo de **3.115
ferramentas** sob **apenas `list_tools` + `execute_code`** (padrão DADL/ToolMesh). Isto **supera** a
nossa proposta de "curar 160→15": mesmo 15 schemas custam tokens e fragmentam o prior.

`[FACT]` O Touring **já tem a metade que importa**: `ctx_execute_tools.rs` + crate `inferlets/`
(WASM) — Code Mode existe. O que falta é **a fachada MCP de 2 portas**:
- `touring_search` (descobre comando/capacidade por intenção — já há `tantivy`/`index find` por baixo).
- `touring_exec` (roda um script que orquestra N comandos `touring` na sandbox CEG e devolve **só o
  sumário** — exatamente o I1 fechando o loop).

**Proposta**: em vez de só gatear os 12 legacy routers (mov. §5.3 do coupling-strategy), **definir o
router curado como esses 2-3 meta-tools** + manter os ~10 metadados estruturados de orquestração
(`ast meta`, `wiring impact`, `index find`) para o cérebro-planner. Isto reconcilia o paradoxo de
Gabriel ("MCP não funciona") **na forma mais forte**: o MCP vira porta para Code Mode, não catálogo. `[INFER 0.85]`

### I3 — RGAO router: formalizar o CILA L0-L5 com um vetor medido

`[FACT]` O doc (§4) extrai um **vetor de complexidade `c = (d, n_f, n_s, h, ρ)`** — profundidade de
dependências, nº de arquivos no raio, nº de símbolos, altura média da AST, densidade de acoplamento —
e **roteia deterministicamente** para topologia (solo / 1 subagent / multiagent / deep-research),
derrubando erro de roteamento de **30,1% → 8,2%**.

`[FACT]` **O Touring já produz cada componente desse vetor**:
- `d` ← `touring ast blast` (árvore de dependências) · `ρ` ← `touring wiring` (acoplamento/cycles)
- `n_s` ← `touring index` · `n_f` ← raio de modificação · `h` ← `ast` (altura sintática)

`[INFER 0.8]` Hoje o TACO escolhe L0-L5 + fases por **heurística de trigger words** (CLAUDE.md). O RGAO
diz: **meça `c` e roteie**. Proposta: um `touring route <intent> <paths>` que computa `c` via os
comandos acima e devolve `{level: L0..L5, topology, fases}` — substituindo adivinhação por medição.
Conecta diretamente com o `decomposer` (`touring-server-reasoning/src/reasoning/decomposer`). É o
elo que falta entre "Decision Matrix C01-C12" e "Phase Protocol".

### I4 — Detector de Class D (silent narrative failure)

`[FACT]` O doc (§1) nomeia a falha **mais perigosa** para LLM:

> *"Class D (alucinação e fabricação encadeada): em vez de expor a falha, o modelo consome o erro bruto
> e reconstrói uma narrativa plausível que mascara completamente a falha real."*

`[FACT, este mesmo dia]` **Nós vivemos uma Class D de infra hoje**: o sccache reportou build exit 0 e
o agente teria declarado "status -j otimizado" sobre um **binário stale** — só o teste empírico
(contadores sccache, medição do tamanho) pegou. É a razão de existir a memória `real-exit-codes` e a
REGRA #21.

`[INFER 0.8]` O Touring tem as peças (CEG X9 LEARN, transcript miner, `gate-metrics`) mas **não um
detector dedicado**. Proposta: um sinal no X9 que **cruza a narrativa do turno (claimed outcome) com o
exit-code/stderr real capturado em X0** — divergência → `gotcha` automático + reward negativo. Operacionaliza
"código não testado é hipótese" no nível do harness, não da disciplina humana. `[SPEC 0.6]` poderia
usar o `conformal predictor` que o cli-suggester já tem para calibrar o gatilho.

### I5 — WarpGrep: a munição quantitativa para o redirect grep→`index find`

`[FACT]` O mov. §5.1 do coupling-strategy (cli-suggest reescreve grep→`touring index find`) era
argumentado por *design*. O doc dá o **número** (§2): busca estruturada focada (WarpGrep) =
**+3,7pp SWE-bench Pro, −17% tokens de entrada, −28% tempo total**. `[FACT]` O `index find`/`tantivy`
do Touring **é** esse mecanismo — está subusado por falta de prior, não por falta de capacidade.

→ **Ação**: citar este número no próprio `additionalContext` do cli-suggest quando redireciona
(*"index find: −17% tokens, −28% tempo vs grep cru [WarpGrep, SWE-bench Pro]"*). Indução de **alto
sinal** baseada em evidência, não em "MUST". `[INFER 0.85]`

---

## 3. Mid-tier (valiosos, mais esforço)

- **I6 — Budget conservation no `decompose`** `[INFER]`: o doc prova `Σ Bᵥ + B_resto ≤ B_root`
  (vetor `B∈ℕ⁶`: iter/calls/tokens/sec/retry/handoff) verificado estaticamente em O(|V|+|E|). O nosso
  `Workflow`-tool já tem `budget{total,spent,remaining}`; **o `decompose` DAG não tem**. Levar o
  contrato de orçamento por-nó ao decompose daria garantia formal anti-runaway nos subagents TACO.
- **I7 — Checkpoint seletivo estilo Crab** `[FACT ebpf existe / INFER aplicação]`: o Crab usa eBPF p/
  inferir quais turnos mudam estado do SO e **só então** faz checkpoint (87% economizados, <1,9%
  overhead, 100% recovery). O CEG X0 já classifica side-effects (capability `FsWrite`/`Run`); gatear
  `taco-forge checkpoint` por essa classificação evita checkpoints inúteis em turnos read-only.
- **I8 — Daemon como Shared Context Store (CA-MCP)** `[INFER]`: hoje subagents TACO retornam JSON e o
  **orquestrador-LLM** faz a coordenação (round-trip caro). O daemon + `decompose status` + `memory.db`
  **já são** um store compartilhado; fortalecê-lo para subagents lerem/escreverem progresso direto
  (sem o LLM no meio) é o padrão SCS — corta chamadas ao modelo na FASE 5.
- **I9 — Gate de consistência semântica (GED)** `[INFER]`: quando o TACO roda engineers paralelos, não
  há arbitragem formal antes do merge. O doc usa `D = GED(G_A,G_B) + α(1−cos(e_A,e_B))` sobre os CFGs.
  O Touring tem AST (CFG) + embeddings (antt/nlp) — um gate de divergência na FASE 6 detectaria
  conflito/assimetria entre outputs paralelos (conecta com C08 cross-caller da Decision Matrix).

---

## 4. Validações — o que o doc confirma que o Touring já faz certo

`[FACT]` Reforço explícito (evita "reinventar"):

| Mecanismo do doc | Touring equivalente | Status |
|---|---|---|
| Coerção estrutural / gate determinístico PreToolUse | **CEG X0-X9** typestate (não-skippável) | ✅ alinhado |
| Conformal predictors p/ gatilho de autonomia | `cli_suggester` τ-gate (coupling §3.1) | ✅ **estado-da-arte** |
| `U(a)=P·V−C` (utilidade sob orçamento) | já é o eixo do coupling-strategy §4 | ✅ |
| Circuit breaker / anti-loop (TCA "Tempo", decay e^{−λt}) | circuit breaker + loop detection | ✅ |
| Semantic execution hooks (`before_tool_call`) | PreToolUse hooks (176+ registry) | ✅ |
| Output redaction (PII/keys) | hook `scan-pii` | ✅ |
| Sandbox (WASM fuel/epoch, namespaces) | CEG `SandboxExecutor` + landlock/rlimit + `touring-wasm` | ✅ |
| Progressive disclosure (schemas sob demanda) | Claude Code deferred tools + `ToolSearch` | ✅ (mas MCP touring fura ao expor 160) |
| Memória estratificada (working/episodic/semantic) | `.remember` / `memory.db` / `MEMORY.md` | ✅ |

→ **Touring está arquiteturalmente correto.** Os gaps são de **ativação/modo de uso** (I1-I5), não de
fundação. Isto é exatamente o diagnóstico do coupling-strategy: "a infra coercitiva existe; o **modo de
acoplamento** é que está errado".

---

## 5. Segurança (§5 do doc) — direções já cobertas / a reforçar

`[FACT]` O doc alerta que Code Mode **abre vetores** (MAESTRO threat model: elevar dados não-confiáveis
a semântica executável). Mitigações recomendadas × Touring:
- **OAuth 2.1 PKCE / rotação de token** — N/A no dev-loop local; relevante se o Touring expor rede.
- **Air-gapped sandboxing (namespaces sem rede)** — o CEG tem `Net(HostScope)` deny-by-default +
  landlock; `[INFER]` reforçar que o profile `Sandboxed` **nega rede por default** já cobre.
- **Output filtering/redaction** — `scan-pii` ✅; o **I1 (summarizer)** é o ponto natural p/ redação.
- **Semantic hooks antes de side-effects** — PreToolUse + CEG X6 (capability gate) ✅.
- **ClawAudit STRIDE no runtime layer** `[INFER]`: o doc auditou o **código do próprio agent-runtime**
  (Semgrep recall 21,7%→66,8%). Ângulo novo: rodar o `touring-quality` F2.x **sobre o próprio
  `touring-hooks`/CEG** com lente STRIDE — auditar o harness, não só o código do usuário.

---

## 6. Síntese — como isto refina a estratégia de coupling

O coupling-strategy estava **certo na direção** e o harness doc **valida com 30 papers**. O que ele
**adiciona** ao nosso plano:

1. **Eleva o I1 (Active Summarizer) a P0** — é a generalização do fix de hoje e ataca a raiz da
   verbosidade; o doc dá o alvo (`>4KB → <200 tok`).
2. **Radicaliza o MCP**: de "curar 160→15" para **"2 meta-tools sobre Code Mode"** (I2, 142× medido).
   O Touring já tem Code Mode (`ctx_execute`/`inferlets`); falta a fachada.
3. **Dá número à indução restante** (I5/WarpGrep): o cli-suggest passa a redirecionar com **evidência**
   (−17% tok), não com "MUST".
4. **Fecha dois loops de qualidade** que faltavam: **Class-D detector** (I4 — o que nos mordeu hoje) e
   **RGAO router** (I3 — formaliza o CILA com dados que o Touring já produz).

**Sequência sugerida (atualiza §6 do coupling-strategy)**: I1 (summarizer) + I5 (número no redirect) no
mesmo lote do redesign cli-suggest → I2 (meta-tools MCP) → I3 (router) + I4 (class-D) como o salto de
qualidade. Tudo aproveitando peças que o Touring **já tem**, não construindo do zero.

---

## 7. Rodada 2 — fundamentação na estrutura real (48 crates, 636.937 LOC)

Segunda passada: 3 exploradores paralelos no código real (CEG/sandbox, Code Mode/MCP, cognition/RL/saga)
+ VGP. Vários `[INFER]` viraram `[FACT]`; **três foram corrigidos**. Veredito: o Touring é **muito mais
rico** do que a rodada 1 assumiu — o que **muda a natureza dos gaps de "construir" para "ativar/conectar/induzir"**.

### 7.1 Correções ao mapa mestre

| # | Rodada 1 dizia | Estrutura real `[FACT]` | Implicação |
|---|---|---|---|
| **I1** | "Active Summarizer: gap vazio" | **Parcialmente errado.** (a) truncação hard **1 MB** + `was_truncated` + blake3 (`touring-ceg/src/gateway/sandbox_executor.rs:38,47,60`); (b) **`ObservationMasker`** (tool-result→1-linha, ativa >4000 tok, ~5-10×) + **`ContextCompiler`** (priority P0-P3) em `touring-server/src/{observation_masker,context_compiler}.rs`. **Mas** atuam na camada **MCP/daemon** (`tools_infra.rs`, `context_tools.rs`), **não no stdout de `touring` CLI via Bash** — por onde `status`/`orphans` poluem | Gap mais preciso: **a sumarização existe, mas não no caminho mais usado**. Fix = densificar CLI (iniciado) OU rotear via `ctx_execute` |
| **I2** | "Code Mode parcial" | **Subestimado.** `ctx_execute` roda **código arbitrário em 11 linguagens** + AST forbidden-call (`ctx_execute_tools.rs:176`); **16 inferlets WASM**; **`FamilyRouter`** (`mcp-curated`, 9 `*_status`→1, `tools_status.rs:1-76`) | Code Mode **já construído**. Gap = **ativação**: `mcp-curated` não-default (160 expostas), **sem `search_tools`**, `ctx_execute` subusado |
| **I3** | "CILA existe; RGAO refina" | **Confirmado + preciso.** `decomposer.rs:235` `CilaLevel{L0..L6}`→N-level topology, **mas roteia por heurística de nível, NÃO por métricas computadas** | RGAO = **fechar o loop**: alimentar o CILA com `c∈ℝ⁵` que o Touring já produz (`ast blast`/`wiring`/`index`). Cirúrgico |
| **I6** | "budget talvez em touring-contracts" | **Corrigido.** `touring-contracts` é **IoC seam puro** (`trait LearnRuntime`, 102 LOC) — zero budget | Gap **maior**: não há **nenhum** orçamento no `decompose` |
| conformal | "[FACT] cli_suggester usa conformal" | **Confirmado.** `cli_suggester.rs:99-122` → `conformal::ConformalCalibrator` (split-conformal `τ=1−q̂`, `DEFAULT_ALPHA`). RL (PPO) é separado | Mantido — o Touring **está** no estado-da-arte de gate-calibration |

### 7.2 Novos insights (só visíveis com a estrutura real)

- **N1 — MCTS para tool-planning (= ToolTree, doc ref 21)** `[FACT MCTS existe / INFER aplicação]`: há
  **`MCTSEngine`+`PheromoneMCTS`** (`touring-intelligence/src/reasoning/mcts.rs:208`, UCB+pheromone
  `augment_ucb:750`) usado p/ **síntese**. O doc usa MCTS p/ **planejar sequências de ferramentas**
  (bidirectional pruning). Oportunidade: apontar o motor existente p/ escolher a **cadeia de comandos
  touring** de menor custo (a "geodésica do grafo" da TCA-Space). O motor existe; falta a aplicação.
- **N2 — 416 hooks = Formal Skill runtime maduro** `[FACT]`: o doc (ref 1, FairyClaw) trata hooks
  `before_*_call` como vanguarda; o Touring tem **416 eventos** + skills. **Validação forte**: o Touring
  já É o "formal skill runtime" do doc — a camada coercitiva está madura; o gap nunca foi infra, é
  **modo de acoplamento** (a tese central, reconfirmada pela própria estrutura).
- **N3 — Tensão sumarização ↔ Class-D (I1 × I4)** `[INFER 0.85]`: o `ObservationMasker` comprime p/
  1-linha. Se descartar o sinal de erro, **cria** a Class-D silent failure que o doc (§1) alerta. Logo o
  summarizer (I1) **deve preservar exit-code + assinaturas de erro + file:line** ao comprimir (doc §3
  prescreve isto). **I1 e I4 são o mesmo projeto** — comprimir sem mascarar.
- **N4 — Saga `compensate()` = rollback semântico (≠ C/R de processo)** `[FACT]`:
  `DistributedSagaCoordinator` (`touring-hooks-saga/src/distributed.rs`, 2PC O(1), prepare/execute/
  **compensate**) dá rollback **lógico** entre subagents; o doc (Crab/DeltaBox) faz C/R de **processo**
  via eBPF. Juntar `ebpf-telemetry` (que existe) + side-effect do CEG-X0 + `compensate` da saga = o I7
  (checkpoint seletivo) com peças já existentes.

### 7.3 Drift de documentação detectado (REGRA co-evolução)

`[FACT]` A exploração revelou docs/rules desatualizados:
- **`rules/code-execution-gateway.md`**: aponta o CEG em `crates/touring-hooks/src/gateway/`; o real é
  **`crates/touring-ceg/`** (`gateway/`+`capability/`+`lib.rs`). **Corrigido nesta sessão.**
- **Hook count**: rules/CLAUDE.md citam 176/178; registry real = **416**.
- **MCP tools**: docs citam 42/86/160; real = **171** `#[tool(` (+ FamilyRouter).

### 7.4 Re-priorização à luz da estrutura real

O trabalho muda de "construir" para **"ativar + conectar + induzir"** — ROI **ainda maior**:
1. **I2 sobe a P0** (era P1): o Code Mode já existe (ctx_execute + 16 inferlets + FamilyRouter). Ativar
   `mcp-curated` default + `search_tools` = **baixo esforço, alto ganho** (motor pronto).
2. **I1 reframe**: não "construir summarizer" — **densificar a CLI** (iniciado) + estender o
   `ObservationMasker` ao caminho CLI/Bash OU induzir `ctx_execute`; projetar **com N3** (preservar erros).
3. **I3 cirúrgico**: ligar `c∈ℝ⁵` (já computável) ao `decomposer.rs` — 1 loop a fechar.
4. **I6 honesto**: budget conservation é o maior gap de fundação (nada existe) — esforço maior, P2.

---

## 8. Rodada 3 — pesquisa externa + best practices (papers 2025-2026 + Anthropic/context7)

Pesquisa em fontes externas sobre estratégias de acoplamento — várias **sem a nomenclatura "coupling"**.
O termo acadêmico canônico é **ACI (Agent-Computer Interface)**. A pesquisa **valida, quantifica e
refina** — não muda o rumo, dá o "como" e corrige um risco real (§8.7).

### 8.1 O nome acadêmico da tese: Agent-Computer Interface (ACI)
- **SWE-agent** (arXiv 2405.15793) funda o conceito: *"LM agents são uma nova categoria de end-user, com
  necessidades próprias, e se beneficiam de interfaces feitas para eles"*. ACI = comandos LM-friendly com
  **output limitado** (janela 100 linhas + ellipsis) + guardrails embutidos (lint antes do edit). → o
  `status --brief`/`slim_large_arrays` de hoje **são ACI**.
- **"From Human to Agent Interfaces"** (arXiv 2603.20300, 2026) — 5 princípios p/ invocação por máquina:
  Machine Interpretability · **Composable Capability Design** (fine-grained, não monólitos) · Explicit
  Contracts (I/O schema, side-effects, failure modes) · Invocation Reliability · **Context Compatibility**
  (descrições concisas). Conceito central: **"invocable capabilities"** = o framing do capability-map.

### 8.2 Harness > modelo, agora com números 2026 `[FACT]`
**42%→78%** no SWE-bench só mudando scaffolding (Particula); **+16 pontos** de swing no mesmo Opus só
pela escolha de harness (2605.15184); **30-50 pontos** de spread no mesmo modelo (Morph/SEAL). **WarpGrep**
(subagent de busca paralela): +2,1–3,7pp, **36 grep+read em <5s / 8 chamadas paralelas** — é o padrão dos
nossos exploradores paralelos (P8).

### 8.3 Code Mode / CodeAct — números oficiais que armam o I2 `[FACT]`
**Anthropic Tool Search = −85% tokens** (descobre tool sob demanda); **CodeAct = −60% tok / −50%
latência** (Microsoft); **Cloudflare Code Mode** (2.500+ endpoints, token mínimo); **RAG-MCP** (2505.03275)
triplica acurácia de seleção, −50% tok; **Meta-tools** (2601.22037). → O Touring já tem o motor
(`ctx_execute` + 17 inferlets); falta **`search_tools` + indução** (I2).

### 8.4 Context Rot — o footprint degrada PERFORMANCE, não só custo `[FACT]`
**Chroma "Context Rot"** (18 LLMs, incl. Sonnet 4): *"performance degrada conforme o input cresce, mesmo
em tarefas simples"*. → reduzir os 178K base é **acurácia**, não só economia. Muda o argumento de "custo"
para "qualidade".

### 8.5 Tool-selection bias — nome/descrição são alavanca `[FACT]`
**BiasBusters** (2510.00307) / **ToolTweak** (2510.02554): a LLM escolhe ferramenta por **metadados
superficiais** (nome, descrição, ordem), não por relevância (manipulável 20%→81%). → otimizar **nomes e
descrições** dos comandos/tools touring (EASYTOOL 2401.06201; PLAY2PROMPT 2503.14432).

### 8.6 Best practices operacionais — Anthropic *Writing tools for agents* (o "como")
| Prática oficial | Operacionalização no Touring |
|---|---|
| **`response_format` enum (concise/detailed) = −⅔ contexto** | É o `--brief` de hoje → **generalizar a todo `-j`** |
| **Fewer high-impact tools** (consolidar) | = FamilyRouter / MCP curado (I2) |
| **High-signal returns** (semantic names; excluir UUID/mime) | = `slim_large_arrays` / drop PPO (feito) |
| **Truncar com instrução** (default 25k tok; guiar p/ busca targeted) | aplicar no CEG summarizer (I1) |
| **Namespacing por prefixo** | já temos (`touring ast *`) — validar contra eval |
| **Erros acionáveis** (não traceback) | conecta com Class-D (I4) |

### 8.7 Refinamento CRÍTICO do I1/N3 — a "Codex pathology" `[FACT]`
"Is Grep All You Need?" mede que **entrega file-based de output pode PIORAR**: Codex caiu **93,1%→55,2%**
com "programmatic grep" porque **a LLM nem sempre completa o ciclo retrieve→read**. O CEG
`sandbox_executor` hoje salva output em disco (blake3) e retorna só o hash — **exatamente o padrão de
risco**. **Correção de design do I1**: o summarizer deve retornar **sumário inline denso + metadata-first**
(erros/arquivos/linhas), com o full em disco apenas *opcional sob demanda* — nunca só o ref.

### 8.8 Fontes (papers/recursos 2025-2026)
SWE-agent ACI (2405.15793) · From-Human-to-Agent-Interfaces (2603.20300) · Is-Grep-All-You-Need (2605.15184)
· RAG-MCP (2505.03275) · BiasBusters (2510.00307) · ToolTweak (2510.02554) · EASYTOOL (2401.06201) ·
PLAY2PROMPT (2503.14432) · Meta-tools (2601.22037) · Anthropic *Code execution with MCP* + *Writing tools
for agents* + *Tool Search* · Cloudflare Code Mode · Microsoft CodeAct · Chroma Context Rot · MCP spec
(context7 `/modelcontextprotocol/modelcontextprotocol`). **Mapa de capacidades**: `2026-06-26-touring-capability-map.md`.

---

_Análise integral de 30-ref synthesis + VGP no código + sequential-thinking. O harness doc não muda o
rumo — **confirma-o e arma-o**: a maior alavanca de um agente é o harness, e os 5 mecanismos top (I1-I5)
são ativações de capacidades que o Touring já possui, na direção que Gabriel apontou: afordância
estrutural barata, não indução semântica cara._
