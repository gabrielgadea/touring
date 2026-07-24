# Code as Agent Harness (arXiv 2605.18747) → Roadmap de Potencialização do Touring/TACO

> **Data**: 2026-05-29 (autoria — versão condensada) — **atualizado 2026-06-03** (status banner)
> **Status pós-2026-06-03**: 🏁 **CAH TIER 1-3 ROADMAP: CLOSED** (86.0% conformance, 35/37 CONFORME, 0 PARCIAL)
> **Fonte**: paper "Code as Agent Harness: Toward Executable, Verifiable, and Stateful Agent Systems" (UIUC + Meta + Stanford, 102pp, ~300 refs) | **PDF**: `~/Downloads/2605.18747v1.pdf` | texto extraído: `/tmp/cah_analysis/cah_{raw,layout}.txt`
> **Verificação**: todos os símbolos Touring confirmados via ripgrep em `~/.claude/rust` (índice: 3002 files / 67698 symbols). Tags FACT[1.0]/INFERENCE/SPECULATION conforme evidência.
> **Versão completa (esta é a condensada)**: `~/.claude/rust/docs/2026-05-29-code-as-agent-harness-touring-COMPLETO.md` — has a DELIVERY UPDATE section appended 2026-06-03 with the full closure narrative.
> **Master closure doc (2026-06-03)**: `~/.claude/rust/docs/2026-06-03-cah-roadmap-closure.md` — canonical single point of truth for the day's work.

## Tese central (FACT [1.0])

O paper é a articulação acadêmica quase-isomórfica do TACO. Sua tríade ("Executable, Verifiable, Stateful") = arquitetura TACO. A taxonomia mapeia 1:1 em subsistemas Touring; os 7 Open Problems (§5.2) são a fronteira de melhoria. TACO está nos tiers mais altos da taxonomia do paper (execution-based + blackboard representation), enquanto a maioria da literatura é implicit/file-only.

---

## Parte A — Harness Interface (§2): Reasoning / Acting / Environment

### A.1 Code for Reasoning (§2.1)
- **Program-Delegated / PoT** (§2.1.1) → REGRA #8 Compute-in-Code: `inferlets`/`ctx_execute`. PoT no nível harness. **HAS**.
- **Formal Verification / executable contracts** (§2.1.2: Lean/VERINA/Lean4Agent) → VGP (check formal-leve); sem proof-assistant. **PARTIAL**.
- **Iterative Code-Grounded / RLEF** (§2.1.3) → CRC loop + `touring learning reward`. **HAS**.
- **A-R1 (POTENCIALIZAÇÃO):** promover trace de inferlet a artefato de raciocínio reutilizável (cache consultável; já há DryRunCache blake3). [impacto alto / esforço M]

### A.2 Code for Acting (§2.2)
- **AutoHarness / action boundary** (§2.2 linha 1092) → CEG X0..X7 É um AutoHarness (denega ação inválida antes do X8). **HAS**.
- **Grounded Skill Selection** (SayCan, KnowNo conformal) → 125 CLI+88 MCP + `cli_suggester` classify. **HAS** / incerteza calibrada **PARTIAL**.
- **Programmatic Policy Gen** (CaP, NormCode governance) → taco-forge + CEG capability model. **HAS**.
- **Lifelong Agents** (Voyager, LYRA corrections→skills) → skills/ + memory tiers + transcript miner (= LYRA). **HAS**.
- **A-A1 (APRIMORAMENTO):** seleção de skill com incerteza conformal (X4 confidence + bash_outcomes → limiar auto-exec vs HITL). [médio / M] (liga OP5)

### A.3 Code for Environment (§2.3) — competência mais profunda do Touring
- **Structured World Reps** (§2.3.1) → symbols.db + knowledge.db + wiring graph + AST. **HAS forte**.
- **Execution-Trace World Modeling** (§2.3.2: CWM, WorldCoder) → ctx_execute traces + bash_outcomes + **X4 PREDICT + blast = Code World Model**. **HAS**.
- **Code-Grounded Eval** (§2.3.3: InterCode/SWE-bench) → e2e + gate-metrics + sandbox CEG. **HAS**.
- **Verifiable Env Construction** (§2.3.4: SWE-smith/EnvScaler) → touring-evolve worktrees + taco-forge validators. **HAS**.
- **A-E1 (POTENCIALIZAÇÃO de alta alavancagem):** consolidar Code World Model preditivo do repo: dado um edit, prever {Δblast, Δorphans, Δquality, prob_compile_fail, testes afetados} ANTES de aplicar. Substrato que o EAGLE (Parte B) acelera. [transformacional / L]

---

## Parte B — EAGLE-3.1 no Code Agent Harness

### B.0 O que é (FACT [1.0], marktechpost 2026-05-27)
Speculative decoding: draft model propõe tokens, target verifica em paralelo, aceita prefixo correto (lossless). **Attention drift**: drafter desvia atenção dos sink tokens para os próprios tokens gerados (causas: representação fundida desbalanceada + magnitude do hidden-state cresce por residual não-normalizado). **Fix**: (a) FC Normalization (magnitude limitada); (b) Post-norm Hidden-State Feedback (re-ancora recursivamente ao target). **Resultados**: 2,03×/1,71×/1,66× throughput (conc.1/4/16, Kimi-K2.6-NVFP4, vLLM TP=4 GB200); **até 2× acceptance length em long-context**. Em vLLM v0.22.0.

### B.1 Isomorfismo central
`draft → verify-paralelo → accept-longest-valid-prefix → rollback` (EAGLE, token) ≡ `Plan → Speculate → Verify` (harness, ação). E **attention drift (micro) ≡ state divergence Sk/Bk (macro, OP4)**.

### Estratégias
- **B-1** Aceleração lossless via EAGLE-3.1 (vLLM/SGLang self-hosted). Ganho direto em §5.2.1 trajectory wall-clock. [FACT-grounded / infra]
- **B-2** Self-Speculative Harness Execution: draft N ações → sensores verificam → aceita prefixo válido. Já existem X4 PREDICT + X5 dry-run + DryRunCache + shadow validate; falta orquestrar como 1 loop. Lossless no nível-ação garantido pelos gates. [INFERENCE 0.8 / transformacional]
- **B-3** Draft-tree ≡ MCTS (`pub mod mcts` existe). Ações em árvore, ramos verificados em paralelo, aceita melhor caminho. "acceptance length" → "verified-action-depth". [INFERENCE 0.75 / alto]
- **B-4** Drift-correction: FC-norm → re-normalizar budget de contexto (compaction + STR); post-norm feedback → re-grounding contra sensores após cada ação aceita (= reconciliação Sk/Bk OP4). [INFERENCE 0.75 / alto]
- **B-5** Telemetria/RL como substrato de treino do drafter (training-time test + RLEF). bash_outcomes(11453)+edits(9551)+transcript miner = dados; treinar action-predictor pequeno. [INFERENCE 0.8 / alto]
- **B-6** Princípio "sink token" = contrato do harness + Sk verificado, não-compactável e sempre-atendido. [SPECULATION 0.65 / princípio]

---

## Tabela de Verificação de Código (FACT [1.0] — file:line em ~/.claude/rust)

| Peça | Símbolo | Local | Status |
|---|---|---|---|
| Entry CEG | `fn run_gateway` | gateway/pre_exec.rs:176 | FACT |
| X0 CAPTURE | `enum ExecSurface` / `fn capture_tool_call` | gateway/capture.rs:21 / :73 | FACT |
| X3 VGP | `struct VgpReport` | gateway/vgp_stage.rs:22 | FACT |
| X4 PREDICT | `struct ExecutionOutcomePredictor` / `PredictionReport` / `fn predict` | gateway/predict.rs:79 / :124 / :154 | FACT |
| X5 SANDBOX | `fn dry_run_in_sandbox` / `SandboxOutcome` | gateway/sandbox_stage.rs:141 / :43 | FACT |
| DryRunCache | `struct DryRunCache` | gateway/dry_run_cache.rs:161 | FACT |
| X7 DECISION | `fn composite_score` / `struct GateDecision` / `enum Verdict` | gateway/decision.rs:109 / :135 / :122 | FACT |
| X9 LEARN | `fn emit_gate_reward` | gateway/learn.rs:52 | FACT |
| Capability | `fn resolve_capability_profile` / `ProjectProfileRegistry` | capability/resolve.rs:124 / mod.rs:36 | FACT |
| Skill selection | `fn classify`/`classify_task`/`webfetch`/`bash` | cli_suggester.rs:288+ | FACT |
| Action signature | `struct ActionSignature` | action_signature.rs:128 | FACT |
| Transcript miner (LYRA) | `extract_error_resolution_pairs` | ingest/transcript_miner.rs | FACT |
| MCTS | `pub mod mcts` | cli/mod.rs:67 | FACT |
| Shadow validate | `applier.shadow_validate` | touring-generator | FACT |
| Inferlets (PoT) | `InferletEntry` / `InferletPool` | handlers_inferlet.rs:150 / wasm/pool.rs:44 | FACT |
| ctx_execute | surface + `ctx_execute_file_count` metric | ceg_e2e.rs:345 | FACT |
| Learning reward (RLEF) | `cli_learning_reward` / `record_reward` | cli_handlers.rs:3822 / rl/templates/evolving.rs:176 | FACT |
| blast_radius | graph service | touring-server | FACT |
| Telemetria B-5 | `BashOutcomeRecord` / `feed_edit_history` | touring-intelligence | FACT |
| **GAP P2** | `EvidenceBundle` | **ABSENT** (rg count 0) | FACT (gap real) |
| **GAP OP4** | `read_set`/`write_set` em gateway | **ABSENT** | FACT (gap real) |

### Descoberta-chave
`predict.rs` já implementa, em forma esquelética, o **draft-predict-learn no nível-ação**: `ExecutionOutcomePredictor` (Beta-prior) + `ActionSignature` que produz a MESMA chave (`signature_for → to_key`) que o X4 prediz e o X9 LEARN registra, backed pelo ledger `outcome:*`. → **B-2/B-3/B-5 são extensões concretas de código existente, não greenfield.**

## Roadmap (H0/H1/H2)
- **H0**: A1 harness-metrics 6-dim + A4 export OTel (reusa counters; mede o resto).
- **H1**: P2 evidence-bundle X7 + A2 change-contract evolve + A3 HITL durável + P3 verifier stack + A-R1 + A-A1.
- **H2**: P1/OP4 transactional state (Automerge + dependency-aware locking) + B-2/B-3 self-speculative execution + A-E1 Code World Model.

## Insights de sessões anteriores (catálogo §5.2)
OP1 harness metrics 6-dim · OP2 evidence bundle/scope · OP3 change-contract sem regressão · OP4 transactional shared state · OP5 HITL durável · OP6 multimodal (SPECULATION p/ Touring) · OP7 4 propriedades (Executable🟢/Inspectable🟡/Stateful🟡/Governed🟢).
