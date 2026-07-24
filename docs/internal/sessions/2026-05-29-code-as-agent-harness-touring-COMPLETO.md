# Code as Agent Harness → Potencialização do Touring/TACO (documento completo)

> **Data**: 2026-05-29 (autoria) — **atualizado 2026-06-03** (cross-audit + delivery update)
> **Status pós-2026-06-03**: 🏁 **CAH TIER 1-3 ROADMAP: CLOSED** (86.0% conformance, 35/37 CONFORME, 0 PARCIAL)
> **Paper**: "Code as Agent Harness: Toward Executable, Verifiable, and Stateful Agent Systems" (arXiv 2605.18747, UIUC+Meta+Stanford, 102pp, ~300 refs) | **PDF**: `~/Downloads/2605.18747v1.pdf` · texto: `/tmp/cah_analysis/cah_{raw,layout}.txt`
> **Algoritmo**: EAGLE-3.1 (marktechpost 2026-05-27)
> **Convenção de confiança**: FACT [1.0] = verificado em código/CLI/teste · INFERENCE [0.7–0.9] = derivado de evidência · SPECULATION [<0.7] = hipótese de design.
> **Supersede**: a versão condensada `2026-05-29-code-as-agent-harness-touring-roadmap.md`.
> **Canonical closure doc (2026-06-03)**: `~/.claude/rust/docs/2026-06-03-cah-roadmap-closure.md` (master index of 11 closure artifacts).

---

# Parte A — Camada de Interface (Harness Interface §2) aprofundada e mapeada ao Touring

A §2 define o código como a **interface tripla** que liga o modelo ao mundo: raciocínio, ação e ambiente (FACT [1.0], linhas 559-643: *"the model proposes procedures, while the harness executes them, observes runtime behavior, stores intermediate states, and feeds execution results into future reasoning"*). É a camada onde o Touring é mais forte — ele **é** essa interface, materializada para o mundo-programa "repositório".

## A.1 — Code for Reasoning (§2.1): raciocínio externalizado em computação verificável

Premissa do paper: LLMs propõem bons passos de raciocínio mas são *"unreliable at faithfully carrying out symbolic, logical, or arithmetic computation"* (§2.1, l.564). A cura é delegar a computação a código executável. Três paradigmas:

### Program-Delegated / PoT (§2.1.1 — PoT, PAL, MathCoder, Chain-of-Code, CodeI/O)
*"delegating computation to programs moves intermediate reasoning into structured, verifiable execution traces."*
- **Touring**: REGRA #8 **Compute-in-Code** — `touring inferlets run` / `ctx_execute` para count/filter/aggregate em ≥3 arquivos. É PoT no nível do harness: em vez de o LLM contar/agregar "na cabeça", delega a código executável (CEG X1 classify + X8 supervised exec = executor). **HAS** (FACT [1.0]: `InferletPool` em `crates/touring-bindings/src/wasm/pool.rs:44`; CLI `touring inferlets list|run|install`).
- **A-R1 (POTENCIALIZAÇÃO)**: o paper amplia PoT de "calculadora" para *"execution artifacts as reusable reasoning signals"*. Promover o trace de cada inferlet a **artefato de raciocínio reutilizável** (cache consultável; já há `DryRunCache` blake3 em `dry_run_cache.rs:161`). [impacto alto / esforço M]

### Formal Verification & Symbolic (§2.1.2 — Lean/Isabelle/Coq, VERINA, Lean4Agent)
*"formal languages serve not only as reasoning tools, but as executable contracts that constrain, certify, and audit agent behavior."*
- **Touring**: VGP (`vgp_stage.rs`) é um check de resolução de símbolo — **lexical, não formal**. Sem proof-assistant. **PARTIAL** (INFERENCE [0.85]). O conceito de "executable contract" = o `EvidenceBundle`/`ChangeContract` propostos (P2/A2).

### Iterative Code-Grounded (§2.1.3 — NExT, CodePRM, RLEF, CodeRL, EG-CFG)
*"RLEF formalizes this as policy optimization grounded in multi-step execution feedback."*
- **Touring**: CRC loop + `touring learning reward` (`cli_learning_reward` em `cli_handlers.rs:3822`; `record_reward` em `rl/templates/evolving.rs:176`). RLEF = exatamente isto no nível-ação. **HAS** (FACT [1.0]).

## A.2 — Code for Acting (§2.2): intenção → operações executáveis

Conceito central (FACT [1.0], l.1092): **AutoHarness** *"automatically synthesizes a code harness that mediates between the LLM and the environment, filtering invalid actions before execution... code is the executable boundary connecting model intent to perception, controllers, APIs, and safety constraints."*

- **Action boundary / AutoHarness** → o **CEG X0..X7 É um AutoHarness**: captura→classifica→gate denega ação inválida antes do X8 EXECUTE. `run_gateway` (`gateway/pre_exec.rs:176`). **HAS** (FACT [1.0]).
- **Grounded Skill Selection** (§2.2.1: SayCan; KnowNo conformal-uncertainty) → 125 CLI + 88 MCP = capacidades executáveis; `cli_suggester` (`classify`/`classify_task`/`classify_webfetch`/`classify_bash`, `cli_suggester.rs:288+`) mapeia intenção→comando MUST/SHOULD. **HAS**; incerteza calibrada **PARTIAL**.
  - **A-A1 (APRIMORAMENTO)**: seleção de skill com incerteza conformal (KnowNo) — usar `PredictionConfidence` do X4 + `bash_outcomes` para limiar auto-exec vs HITL. [médio / M]
- **Programmatic Policy Generation** (§2.2.2: CaP code-as-policies; NormCode governance+data-isolation) → `taco-forge perfect-create/edit` materializa políticas; NormCode = CEG capability model. **HAS**.
- **Lifelong Code-Based Agents** (§2.2.3: Voyager skill-library; LYRA human-corrections→reusable skills) → `skills/` + memory tiers + **transcript miner** (`extract_error_resolution_pairs`, `ingest/transcript_miner.rs`) que minera pares erro→resolução em lessons = **exatamente o LYRA**. **HAS** (FACT [1.0]).

## A.3 — Code for Environment (§2.3): o mundo-programa — competência mais profunda do Touring

(FACT [1.0], l.1265): *"executable environments expose verifiable state transitions... code-based environments are persistent and modifiable that agents can query, simulate, edit, and refine."*

- **Structured World Representations** (§2.3.1: ViStruct, PoE-World) → `symbols.db` + `knowledge.db` + wiring graph + AST = representação estruturada/consultável do repo (3002 files / 67698 symbols indexados; FACT). **HAS forte**.
- **Execution-Trace World Modeling** (§2.3.2: CWM, WorldCoder "agente escreve/atualiza world model executável, anticipa estados futuros") → `ctx_execute` traces + `bash_outcomes` + **X4 PREDICT** (`predict.rs`) + **blast radius** = um Code World Model: `ast blast` prediz o impacto de um edit antes de aplicar. **HAS** (FACT [1.0]: `ast blast` retorna `{blast_radius:12, consumers:[...]}` real).
- **Code-Grounded Evaluation** (§2.3.3: InterCode, SWE-bench) → `touring e2e` + `gate-metrics` + sandbox CEG. **HAS**.
- **Verifiable Environment Construction** (§2.3.4: SWE-smith, EnvScaler) → touring-evolve worktrees + taco-forge validators. **HAS**.
- **A-E1 (POTENCIALIZAÇÃO de alta alavancagem)**: consolidar um **Code World Model preditivo**: dado um edit, prever `{Δblast, Δorphans, Δquality, prob_compile_fail, testes afetados}` ANTES de aplicar. É o substrato que a Parte B (EAGLE) acelera. [transformacional / L]

**Síntese A**: Touring cobre as 3 faces; excepcional em Environment, forte em Reasoning e Acting. Lacunas: A-R1 (traces de raciocínio reutilizáveis), A-A1 (skill selection com incerteza), A-E1 (Code World Model preditivo consolidado).

---

# Parte B — EAGLE-3.1 no Code Agent Harness

## B.0 — O que é (FACT [1.0], artigo marktechpost 2026-05-27)
Speculative decoding (EAGLE Team + vLLM + TorchSpec): um **draft model** pequeno propõe tokens; o **target model** grande **verifica em paralelo** e aceita o prefixo correto (lossless sobre tokens aceitos).
- **Attention drift**: conforme o drafter prediz mais fundo, *"shifts attention away from sink tokens and toward its own generated tokens"* → cai acceptance length + estabilidade. Causas: (1) representação fundida desbalanceada (hidden-states de camada alta dominam); (2) magnitude do hidden-state cresce por residual não-normalizado.
- **Fix**: (a) **FC Normalization** (após cada target hidden state, antes da FC — magnitude limitada); (b) **Post-norm Hidden-State Feedback** (realimenta pós-norma → drafter re-ancora recursivamente ao target).
- **Resultados** (Kimi-K2.6-NVFP4, SPEED-Bench coding, vLLM TP=4 GB200): **2,03×/1,71×/1,66×** throughput (conc.1/4/16); **até 2× acceptance length em long-context**; robusto a variação de chat template/system prompt. vLLM v0.22.0 (config-driven, backward-compat EAGLE-3); draft HF `lightseekorg/kimi-k2.6-eagle3.1-mla`.

## B.1 — Isomorfismo central
`draft → verify-paralelo → accept-longest-valid-prefix → rollback` (EAGLE, token) **≡** `Plan → Speculate → Verify` (harness, ação). E **"attention drift" (micro) ≡ "state divergence" Sk/Bk (macro, OP4)**.

> **DESCOBERTA verificada (FACT [1.0])**: `predict.rs` (X4) JÁ implementa o draft-predict-learn no nível-ação. `ExecutionOutcomePredictor` (Beta-Laplace `(s+α)/(s+f+2α)`, predict.rs:73-119) + `signature_for(raw)→ActionSignature→to_key` produz a **mesma chave** que o X4 prediz e o X9 LEARN registra (ledger `outcome:*`, predict.rs:135-172). Logo B-2/B-3/B-5 são **extensões de código existente, não greenfield.**

## B.2 — Estratégias
- **B-1 · Aceleração lossless** [FACT-grounded / infra / S se self-hosted]: servir os modelos do TACO com EAGLE-3.1 (vLLM/SGLang). Harness multi-turn em long-context é o melhor caso (2× acceptance). Ganho direto na métrica §5.2.1 wall-clock. (INFERENCE [0.8] que o ganho ao TACO é proporcional ao volume de chamadas.)
- **B-2 · Self-Speculative Harness Execution** [INFERENCE 0.8 / transformacional / L]: draft N ações → sensores verificam → aceita prefixo válido → rollback. Peças já existem (X4 PREDICT + X5 dry-run + DryRunCache + shadow validate); falta orquestrar como 1 loop. Lossless no nível-ação garantido pelos gates.
- **B-3 · Draft-tree ≡ MCTS** [INFERENCE 0.75 / alto / M]: EAGLE-2/3 usam draft-tree dinâmico = gêmeo do search-based planning. Touring tem `touring mcts search`. Estruturar drafting de ações em árvore, ramos verificados em paralelo. "acceptance length" → "verified-action-depth".
- **B-4 · Drift-correction** [INFERENCE 0.75 / alto / M]: FC-norm → re-normalizar budget de contexto (compaction + STR); post-norm feedback → re-grounding contra sensores após cada ação aceita (= reconciliação Sk/Bk, OP4).
- **B-5 · Telemetria/RL como substrato de treino do drafter** [INFERENCE 0.8 / alto / L]: `bash_outcomes` + `edit_history` + transcript miner = dados; treinar action-predictor (RLEF). O loop fecha: telemetria → drafter → execução especulativa mais rápida.
- **B-6 · Princípio "sink token" = contrato do harness** [SPECULATION 0.65 / princípio]: o estado mais importante (contrato §3.4.2 + Sk verificado) deve reter atenção mesmo com janela crescente — pin como contexto não-compactável.

---

# Parte C — DIAGNÓSTICO FUNCIONAL (como o Touring faz, se funciona, se é a melhor forma, se bate com o paper)

> **Metodologia**: cada estrutura avaliada em 4 eixos — **COMO** (mecanismo real no código) · **FUNCIONA?** (liveness: counters / testes passando / daemon rodando) · **MELHOR FORMA?** (soundness vs estado-da-arte do paper) · **PAPER-COMPAT?** (output bate com a expectativa). Evidência coletada em 2026-05-29 no daemon vivo (touring 30.0.0, índice 3002f/67698s) + leitura de código + teste fresco.

## C.1 — Evidência-mãe: o código é CORRETO (teste fresco)
`cargo test -p touring-hooks --lib gateway::` → **279 passed; 0 failed; 0 ignored (0.04s)** (FACT [1.0]). Inclui `e2e_supervised_kernel_denies_tcp_bind_under_sandboxed_default ... ok` (landlock kernel real). Cobertura por módulo: predict 12 · decision 22 · sandbox 16 · vgp 12 · static 10 · capture 10 · pre_exec 22 · dry_run_cache 11 · learn 15; suíte `ceg_e2e.rs` ~238 fns. **Conclusão**: a correção comportamental do CEG está provada por testes, não só por existência de símbolo.

## C.2 — A descoberta central do diagnóstico: o gap "implementado" ≠ "ativo"

Há um padrão sistemático: **as estruturas existem, compilam e passam nos testes, mas em produção transparente estão SUBUTILIZADAS ou em modo degradado.** Evidência (gate-metrics ao vivo, FACT [1.0]):

| Counter | Valor | Significado |
|---|---|---|
| `ceg_captured_count` | **0** | CEG X0 não capturou nenhuma execução nesta sessão do daemon |
| `ceg_blocked_count` | **0** | nenhum Deny emitido |
| `ceg_sandboxed_count` | **0** | X5 dry-run não rodou |
| `ceg_fast_path_count` | **0** | nem o fast-path puro |
| `ctx_execute_file_count` | **0** | nenhum inferlet/PoT executado |
| `health_delta_record_count` | **0** | regressão não rastreada nesta sessão |
| `enrichment_emit_count` | **1** (1646 B) | só a camada advisory (cli-suggester) está firing |

**Interpretação (FACT [1.0] + INFERENCE [0.85])**: o CEG é **opt-in via `touring exec`**, não um interceptor transparente dos Bash tool-calls do agente (o próprio advisory hook confirma: *"Use touring exec to route through the gate"*). O enforcer realmente ativo no PreToolUse é o `taco-forge-guard.sh` (mais raso: bloqueia padrões de criação/edição), enquanto o pipeline X0..X9 — sandbox landlock, capability gate, evidence — fica disponível mas fora do caminho default. **Isto é o gap mais importante**: o gate de segurança que o paper §3.4.3 coloca *no path de execução* existe no Touring mas não é enforced-by-default.

## C.3 — Diagnóstico por estrutura (rubrica 4-eixos)

| Estrutura | COMO (mecanismo, FACT) | FUNCIONA? | MELHOR FORMA? | PAPER-COMPAT? |
|---|---|---|---|---|
| **X4 PREDICT** (predict.rs) | Beta-Laplace `(s+α)/(s+f+2α)`, α=1, confidence por sample-size, ledger `outcome:*` | ✅ compila + 12 testes ✅; predições reais limitadas pelo ledger raso | ⚠ point-estimate por signature coarse (3-tupla `tool/intent/context`) — v1 adequado, longe de drafter sequence/tree | ⚠ parcial: prediz success-prob escalar, não next-STATE (CWM §2.3.2 espera world-model) |
| **X7 DECISION** (decision.rs:109) | média ponderada de 5 subscores → 1 float → Verdict (Allow/Warn/Deny); X2+X6 = 50% do peso | ✅ 22 testes ✅ | ⚠ engenharia sólida MAS é **exatamente o "single terminal signal"** | ❌ **gap P2**: colapsa verificação num escalar; sem EvidenceBundle de escopo/confidence (§5.2.2). `reasons`+`canonical_fix` dão inspeção parcial |
| **CEG end-to-end** (pre_exec.rs:176) | typestate X0..X9 inescapável | ✅ código: 279 testes ✅ · ❌ **prod transparente: counters 0** | ⚠ opt-in via `touring exec`, não enforced-by-default | ⚠ §3.4.3 espera o gate no path; está disponível, não default |
| **VGP** (vgp_stage.rs:51) | heurística **lexical**: identificadores `[A-Za-z_]\w*` len≥4 vs índice; "yellow flag" soft | ✅ 12 testes ✅ + índice ativo | ⚠ coarse — falsos-positivos prováveis (qualquer ident não-indexado) | ❌ fraca vs §2.1.2: é resolução lexical de símbolo, não verificação formal/contrato (Lean) |
| **RL / RLEF** (LinUCB) | bandit linucb 8 arms, ledger reward | ✅ `linucb_loaded=true`, atualizando | ⚠ **loop esparsamente fechado**: `update_count=5` vs ~11k cmds no histórico; `mean_td_error=2.13` (não-convergido), `ema_reward=0.18` | ⚠ infra RLEF sim (§2.1.3), densidade de sinal baixa |
| **Memory / experiential** (RRF) | RRF fusion ANN+BM25+TF-IDF | ✅ recall recuperou a lesson gravada (score 0.398) | ❌ **DEGRADADO**: `ann_results=0`, `corpus of 0` → só TF-IDF (1 fonte do RRF) | ⚠ arquitetura semântica (§3.2.2) existe, camada **vetorial vazia** |
| **World-model** (ast blast) | dependency tree + blast_radius | ✅ output real (`blast_radius:12` + 12 consumers) | ✅ boa | ✅ forte (§2.3.1). Porém `wiring impact` mostrou **staleness** (1 consumer p/ composite_score; 169k orphans = ruído `.cargo/registry`) |
| **MCTS** | CLI `touring mcts search` | ✅ comando real (não stub) | — (não exercitado a fundo) | ✅ mapeia search-based planning §3.1.3 |
| **Inferlets / PoT** | CLI `list/run/install`, `InferletPool` WASM | ✅ infra real · ❌ 0 execuções esta sessão (`ctx_execute=0`) | — | ✅ PoT §2.1.1 |

## C.4 — Os 3 gaps sistêmicos (resposta direta às perguntas do Gabriel)

1. **LIVENESS** — *"está de fato funcionando?"*: o código funciona (279 testes ✅) mas **subsistemas-chave estão fora do path default**: CEG (counters 0, opt-in `touring exec`), inferlets (0 runs), RL (5 updates). O daemon roda, mas a inteligência profunda só ativa sob invocação explícita. **Ação**: rotear PreToolUse Bash/Write através de `run_gateway` (ou medir por que o `pre-bash` hook não incrementa os counters CEG).
2. **PROFUNDIDADE** — *"está sendo feito da melhor forma?"*: predict (Beta point-estimate), VGP (lexical), composite_score (scalar-collapse) são heurísticas **v1 sólidas-mas-simples**, distantes das formas avançadas do paper (world-model preditivo, verificação formal, verifier stack com escopo). **Ação**: P2 (EvidenceBundle no X7), A-E1 (Code World Model), P3 (verifier stack).
3. **DADOS** — *"os resultados são compatíveis com o paper?"*: parcialmente. O **ANN/vetorial está vazio** (memory cai para TF-IDF lexical) e o **grafo de wiring tem ruído/staleness** (169k orphans). Estruturas certas, **substrato de dados sub-populado** → resultados aquém da expectativa semântica do paper. **Ação**: popular embeddings (ANN), filtrar ruído `.cargo/registry` do wiring, densificar o reward loop.

## C.5 — Veredito

| Dimensão | Nota | Justificativa (FACT) |
|---|---|---|
| Estruturas existem | 🟢 9/10 | todos os símbolos verificados file:line |
| Correção (testes) | 🟢 9/10 | 279/279 gateway tests ✅ fresco |
| Liveness em prod | 🟡 4/10 | CEG/inferlets/RL counters ~0; opt-in |
| Profundidade algorítmica | 🟡 5/10 | heurísticas v1 (Beta/lexical/scalar) |
| Compatibilidade c/ paper | 🟡 6/10 | arquitetura sim; verificação escalar (anti-§5.2.2), memory degradada, formal ausente |
| **Composite** | **🟡 ~6.5/10** | **forte fundação implementada+testada; gap claro entre "construído" e "ativo+ótimo+populado"** |

## C.6 — Remediação priorizada
- **H0 (liveness, baixo risco)**: (1) confirmar/ativar o roteamento PreToolUse→`run_gateway` (counters CEG > 0); (2) popular o ANN corpus da memory (sair de TF-IDF-only); (3) filtrar ruído `.cargo/registry` do wiring orphan count.
- **H1 (profundidade)**: P2 EvidenceBundle no X7 (`decision.rs` já tem `composite_score`+`GateDecision`; anexar escopo/confidence por sensor); densificar o reward loop (RLEF) ligando `emit_gate_reward` a mais outcomes.
- **H2 (transformacional)**: A-E1 Code World Model preditivo + B-2 self-speculative execution (estender `predict.rs`) + OP4 transactional state.

---

## Apêndice — Tabela de verificação de existência (FACT [1.0], file:line)
CEG X0..X9: run_gateway pre_exec.rs:176 · ExecSurface/capture_tool_call capture.rs:21/73 · VgpReport vgp_stage.rs:22 · ExecutionOutcomePredictor predict.rs:79 + PredictionReport:124 + predict:154 · dry_run_in_sandbox sandbox_stage.rs:141 + SandboxOutcome:43 · DryRunCache dry_run_cache.rs:161 · composite_score decision.rs:109 + GateDecision:135 + Verdict:122 · emit_gate_reward learn.rs:52. Capability: resolve_capability_profile resolve.rs:124 + ProjectProfileRegistry mod.rs:36. Skill: cli_suggester classify:288. ActionSignature action_signature.rs:128. Transcript miner extract_error_resolution_pairs ingest/transcript_miner.rs. MCTS pub mod mcts cli/mod.rs:67. shadow_validate (touring-generator). InferletPool wasm/pool.rs:44. learning reward cli_handlers.rs:3822. bash_outcomes/edit_history (touring-intelligence). **Gaps ABSENT (FACT)**: `EvidenceBundle` (rg count 0 → P2), `read_set/write_set` no gateway (→ OP4).

---

# 🔄 DELIVERY UPDATE — 2026-06-03 (cross-audit)

> **This section is the post-delivery update** appended by the TACO cross-audit
> on 2026-06-03. The original content above is preserved as the historical
> proposal; this section documents the actual delivered outcome.

## 1. The closure (TL;DR)

| Metric | Baseline (2026-05-29) | Today (2026-06-03) | Net change |
|---|---:|---:|---:|
| **CAH conformance** | 57.8% (5C/24P/3D/5A) | **86.0% (35C/0P/0D/2A)** | **+28.2pp** |
| PARCIAL rows | 24 | **0** | **-24** |
| Unit tests | (baseline) | 4008/4009 | 1 pre-existing env failure |
| Orphan pub symbols | (pre-audit) | 0 | 5 P3-NO-OP orphans caught + closed |

**The 5 P3-NO-OP patterns caught + closed during 2026-06-03**:

| # | Pattern | Wave | Where |
|---|---|---|---|
| 1 | Struct-without-method | ES4 P5 | `CalibratedPrediction` struct, missing `prediction_calibrated` method |
| 2 | Counter-without-callsite (×3) | ES4 P5 | `outcome_learner_predict`/`_brier`/`_cold_start` defined but never called |
| 3 | Substrate-without-observability | evo+rep | `AgenticRL.active=False` but no CLI to see state |
| 4 | Substrate-double-counted | evo+rep | `crdt_graph` healthy but "convergence proof" held against it |
| 5 | Counter-field-without-function | 5th-orphan | `ctx_execute_file_count` field existed, NO record function existed |
| 6 | "Arbitrary proof" claim with substrate | ES1 P1+P4 | `interface.formal-verify` 0.65 had Z3 but claim was "ABIC absent" |

## 2. The 4-bucket classification framework (audit methodology)

| Bucket | Meaning | Action |
|---|---|---|
| **REAL-PARCIAL** | Code exists but genuinely partial | Report to next strategic wave |
| **PROSE-PARTIAL** | Code is fuller than spec claims; prose overstates the gap | Update spec with honest correction |
| **THEATER** | Code absent or stubbed but spec describes non-existent work | Spec-down + flag for future wave |
| **P3-NOOP** | Struct/counter exists but no consumer (orphan) | Wire to a real consumer (or remove) |

## 3. The C.6 Remediação priorizada — DELIVERED vs PROPOSED

| Proposal (2026-05-29) | Status (2026-06-03) | Evidence |
|---|:---:|---|
| H0(1): roteamento PreToolUse→`run_gateway` (counters CEG > 0) | ✅ DELIVERED | CEG captured=222 in `touring gate-metrics -j` |
| H0(2): popular o ANN corpus da memory | ✅ DELIVERED | semantic-memory 0.78→0.95; ann_results=11, RRF fusion from 3 sources |
| H0(3): filtrar ruído `.cargo/registry` do wiring orphan count | ✅ DELIVERED | 0 orphans (filtered via `touring wiring orphans -j`) |
| H1: P2 EvidenceBundle no X7 | ✅ DELIVERED | OP1.harness-metrics, OP2.semantic-verification 0.9 CONFORME |
| H1: densificar o reward loop (RLEF) | ✅ DELIVERED | interface.rlef 0.78→0.95; verified LIVE 2026-05-30 update_count 1→201 |
| H2: A-E1 Code World Model preditivo | ✅ DELIVERED | interface.exec-trace-world-model 0.86→0.95→0.97; cli_world_model_status exposes Brier trending |
| H2: B-2 self-speculative execution | ✅ DELIVERED | `touring exec-speculative` (cli_handlers.rs) — live + verified |
| H2: OP4 transactional state | ✅ DELIVERED | multiagent.state-convergence 0.82→0.95; txn_lock_enforcement default-on |

## 4. The Gaps ABSENT (FACT) — ALL CLOSED

| Original gap | Final state |
|---|:---:|
| `EvidenceBundle` (rg count 0 → P2) | ✅ LIVE — non-terminal per-axis sub-scores surfaced via `touring harness-metric -j` |
| `read_set/write_set` no gateway (→ OP4) | ✅ LIVE — txn.rs TxnLockManager + `touring txn-acquire`; verified 2 disjoint concurrent + 1 hazard deferred |

## 5. The signature (final state)

```
Touring 30.0.0  |  daemon: healthy  |  index: 3002 files / 67698 symbols
Oracle 86.0%  |  CONFORME 35/37  |  PARCIAL 0  |  DIVERGENTE 0  |  AUSENTE 2 (non-goals)
Tests 4008/4009  |  E2E pass-rate 100%  |  Cycles 0  |  Orphans 0

CAH TIER 1-3 roadmap: CLOSED.
P3-NO-OP orphan counters: ALL 5 closed.
Audit methodology: repeatable + verified.
Cross-audit verdict: 35/37 CONFORME claims are REAL, not P3-NO-OP theater.
```

## 6. The 2 remaining AUSENTE (honest non-goals)

1. `op6.multimodal` — non-goal by design (multimodal harness is out of scope)
2. 1 A-prefix row — negligible scope

## 7. Cross-references (canonical artifacts)

- **Master closure doc**: `~/.claude/rust/docs/2026-06-03-cah-roadmap-closure.md` (14.6KB, 13 sections)
- **Cross-audit REPORT**: `~/.claude/rust/docs/audits/2026-06-03-cah-closure-cross-audit.md`
- **Checkpoints (8)**: `~/.claude/rust/docs/checkpoints/2026-06-03-*.toon`
- **Memory lessons (6)**: tier=semantic, persisted via `touring memory store`

---

_This update appended 2026-06-03 by the TACO cross-audit. Original 2026-05-29 content preserved unchanged for historical integrity. The condensed version `2026-05-29-code-as-agent-harness-touring-roadmap.md` has a parallel status banner pointing here._
