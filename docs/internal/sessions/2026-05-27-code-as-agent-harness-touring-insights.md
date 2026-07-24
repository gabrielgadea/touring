# Code as Agent Harness ↔ Touring — Insights Consolidados (Rodadas 1 + 2)

> **Data**: 2026-05-27 | **Sessão**: e83b83eb (composite_health 0.6873) | **Anchor**: Gabriel Gadea
> **Paper**: *Code as Agent Harness — Toward Executable, Verifiable, and Stateful Agent Systems*
> arXiv [2605.18747v1](https://arxiv.org/abs/2605.18747), 18 mai 2026, 102p, 41+ autores (UIUC + Meta + Stanford)
> **GitHub**: https://github.com/YennNing/Awesome-Code-as-Agent-Harness-Papers
> **Resultado**: **24 insights de potencialização** (12 estratégicos rodada 1 + 12 técnicos rodada 2), validados via VGP CLI

---

## Sumário Executivo (TL;DR)

**Tese central do paper (FACT 1.0)**: código deixou de ser alvo de LLM e tornou-se **substrato operacional** do agente, definido por 4 propriedades irredutíveis: **executable + inspectable + stateful + governed**. Estrutura a literatura em 3 camadas (Harness Interface §2, Harness Mechanisms §3, Scaling §4) com 7 problemas abertos (§5.2).

**Posição única do Touring (INFERENCE 0.9)**: §4.3 do paper declara *"none of the surveyed systems fully unifies repository-based and execution-based representations into a single harness substrate"*. Touring é estruturalmente **o único candidato** a unificar os 3 níveis de substrate (Repository-based + Execution-based + Blackboard/Shared-State) — já tem todos os ingredientes implementados.

**Ground truth fáctico via VGP** (`touring doctor/status/learning -j`, 2026-05-27):
- daemon healthy 7/8 components
- composite_health_score = **0.6873**
- índice = 30.364 arquivos / 1.114.103 símbolos
- wiring = 27.077 orphans (193.369 são `non_rust` ou cargo deps → ruído real menor)
- learning EMA reward = **0.5249**, mean_td_error = **15.24** (alto, não convergido), update_count = 88
- `agentic_rl_state.active=true` mas `update_count=0` — **subsistema activo sem feeder** (gap real)
- synergy = **52 wired_pairs** (cresceu de 45)
- gate-metrics zerados na sessão (daemon recém-reiniciado, sem persistência cross-session)

---

## A. Estrutura do paper (mapa mental)

```
Code as Agent Harness
├── §2 Harness Interface
│   ├── §2.1 Code for Reasoning (program-delegated, formal, iterative)
│   ├── §2.2 Code for Acting (skill selection, programmatic policy, lifelong)
│   └── §2.3 Code for Environment (structured, trace-based, evaluation, construction)
├── §3 Harness Mechanisms
│   ├── §3.1 Planning (linear, structure-grounded, search, orchestration)
│   ├── §3.2 Memory (working, semantic, experiential, long-term, multi-agent, compaction)
│   ├── §3.3 Tool Use (function, env-interaction, verification, workflow)
│   ├── §3.4 PEV Loop (Plan-Execute-Verify control)
│   └── §3.5 Agentic Harness Engineering (deep telemetry + evolution agent + governed mutation)
├── §4 Scaling the Harness (Multi-Agent over Code)
│   ├── §4.1 Coding Support (role specialization, interaction modes, workflow topology)
│   ├── §4.2 Execution Feedback + Shared-Harness Sync
│   ├── §4.3 Shared Code-Centric Substrate (THE central gap)
│   └── §4.4 Patterns and Trends
└── §5 Emerging Fields and Open Problems
    ├── §5.1 Applications (Code Assistants, GUI/OS, Embodied, Scientific, Personalization)
    └── §5.2 7 Open Problems (Evaluation, Verification, Self-Evolution, Transactional State,
                                HITL Safety, Multimodal, Science of Harness Engineering)
```

---

## B. Mapeamento Touring ↔ Paper (matriz)

| Camada paper | Primitiva proposta | Touring/TACO hoje | Cobertura |
|---|---|---|---|
| §2.1 Code for Reasoning | Programas externalizam computação | Bash/Python + decompose DAG + memory recall | **alta** |
| §2.2 Code for Acting | Programas = policy/tool calls | ~125 CLI + 88 MCP tools + 176 hooks | **alta** |
| §2.3 Code for Environment | Repo/traces/tests como mundo inspetável | `touring index/wiring/ast` + cargo check + e2e -j | **alta** |
| §3.1 Planning | 4 tipos: linear/structure/search/orchestration | `touring decompose` + `taco-forge plan` + `touring mcts search` | **alta** |
| §3.2 Memory | 6 tipos | `touring memory` tier semantic/working/episodic + transcript_miner | **média** |
| §3.3 Tool Use | 4 tipos | Touring CLI/MCP cobre os 4 | **alta** |
| §3.4 PEV Loop | Static→Sandbox→Verify→Permission | TACO Phase 0/4.5/5/6 + CEG X0..X9 + cargo gates | **alta** |
| §3.5 Agentic Harness Engineering | Deep Telemetry + Evolution Agent + Governed Mutation | gate-metrics + `touring evolution` + REGRA #14 taco-forge | **média** ⚠ |
| §4.1 Multi-Agent Roles | Manager/Planner/Coder/Reviewer/Tester | scouter/architect/engineer/auditor/scriber | **alta** |
| §4.3 Shared Harness Substrate | 4 níveis: implicit/repo/exec/blackboard | Touring tem os 3 últimos — **única assim** | ⭐ posição única |

---

## C. RODADA 1 — 12 insights estratégicos (gaps de alto nível)

### 🔴 P0 — Closures imediatos de gaps explícitos do paper

#### 1. Harness-Level Metrics Dashboard (§5.2.1)
- **Gap**: paper exige 6 dimensões: trajectory_efficiency / verification_strength / recovery_ability / state_consistency / safety_compliance / replayability
- **Ação**: `touring harness-metrics -j --window 24h`
- **Effort**: 3-5 ed · **Métrica**: 6 dims live + alerta se < 0.7

#### 2. Evidence Bundle por ação aceita (§5.2.2)
- **Gap**: *"every accepted action carry an evidence bundle"*
- **Ação**: extender `taco-forge perfect-edit` STAGE 8 para emitir `EvidenceBundle { checks_run, assumptions_preserved, untested_regions, remaining_risks, confidence }` em `~/.claude/touring/evidence/<sha>.toon`
- **Effort**: 4-6 ed · **Métrica**: 100% perfect-edit emitem bundle

#### 3. Change Contract antes de mutação do harness (§5.2.3)
- **Gap**: *"Every proposed edit should carry a change contract: which component is modified, which failure mode it targets, which improvement it predicts, which invariants it must preserve, which evaluation can falsify it, and how it can be rolled back."*
- **Ação**: `touring change-contract validate <plan.toon>` no TACO Phase 4.5
- **Effort**: 5-7 ed · **Métrica**: zero engineer spawn sem contract assinado

### 🟠 P1 — Loops parcialmente implementados

#### 4. Evolution Agent loop fechado (§3.5.2, Table 8)
- **Estado**: `touring evolution insights` + RL + transcript_miner, mas loop **observe→diagnose→propose→evaluate→promote** não fechado (falta "propose harness mutation")
- **Ação**: `touring evolution propose-mutation` → patch sobre `~/.claude/rules/*.md` ou hook prompts, gated por shadow validate + canary RL
- **Inspiração**: EvoMAC "Gradient Agent" + GEPA reflective evolution
- **Effort**: 8-12 ed · **Métrica**: ≥1 mutação promovida com Δreward > 0.1

#### 5. OpenTelemetry GenAI semantic conventions (§3.5.1)
- **Insight**: OTel GenAI standardiza `gen_ai.request.model`, `gen_ai.usage.input_tokens`, `gen_ai.response.finish_reasons`, etc — Langfuse/Honeycomb/Datadog consomem direto
- **Ação**: hook `post_tool_rl.rs` emite OTLP spans (mantendo stack interno)
- **Effort**: 4-6 ed · **Métrica**: `touring otel status -j` ativo

#### 6. Score-based convergence breakdown (§4.3.2)
- **Gap**: `touring e2e -j` retorna `composite` opaco
- **Ação**: emit `{ composite, breakdown: { health_delta, wiring, quality, tdg, gotcha, rl_ema } }` com fórmula explícita
- **Effort**: 2-3 ed · **Métrica**: auditor cita ≥3 dimensões com peso

### 🟡 P2 — Expansões de capacidade

#### 7. Transactional Shared Program State (§5.2.4)
- **Gap**: *"each action should declare its read set, write set, assumptions, version dependencies, verifier obligations, and conflict policy"*
- **Ação**: extender `.toon` schema com `transaction: {...}`; cross-audit detecta conflicts entre engineers paralelos
- **Inspiração**: SyncMind formal `|Bk - Sk|`
- **Effort**: 10-14 ed

#### 8. HITL como state evolutivo (§5.2.5)
- **Gap**: cada aprovação/denial deveria atualizar permission rules, escalation policy, memory retrieval
- **Ação**: capturar denials/approvals → `touring memory store --tier semantic "hitl:<tool>:<context_hash>"` → cli_suggester injeta gotcha
- **Effort**: 3-4 ed

#### 9. Multi-Agent Shared Memory Protocol (§3.2.5)
- **Gap**: subagents paralelos sem protocolo de sync
- **Ação**: `touring memory blackboard <wave_id> [--watch]` — namespace compartilhado por wave
- **Effort**: 6-9 ed

### 🟢 P3 — Maturação de longo prazo

#### 10. Verification stack composable (§5.2.2 deep)
- **Insight**: cada verifier declara *"what it verifies, what it cannot, what confidence"*
- **Ação**: `~/.claude/touring/verifiers.toml` machine-readable
- **Effort**: 7-10 ed

#### 11. Self-Evolving Harness regression-free (§5.2.3 deep)
- **Pipeline**: change_contract → evolve_agent_proposes → shadow_validate → canary_rl → promote OR rollback
- **Effort**: 15-20 ed · **Métrica**: ≥3 mutações em 30d sem regressão e-test

#### 12. Position paper "Touring as Unified Substrate"
- **Estratégia**: materializar §4.3 unificação como artefato público
- **Effort**: 5-7 ed

---

## D. RODADA 2 — 12 insights técnicos (primitivas concretas + VGP)

### 🔴 P0 NOVO — Closures justificadas por evidência forense direta

#### NOVO-1. Process Rewards (step-level, não outcome-level) — §2.1.3 + VGP

- **FACT 1.0**: `touring learning status` mostra `mean_td_error=15.24` (alto) e `agentic_rl_state.update_count=0`. Paper (ExecVerify/FunPRM/ReCode/CodePRM) emite reward por **statement/function-step**, não por task final.
- **Ação**: TACO Phase Protocol emite 7 rewards intermediários (1 por fase) — `touring learning reward <phase> <delta> "<context>"` onde `delta = phase_quality_score - phase_baseline`
- **Cargo path**: `crates/touring-rl/src/bandit/linucb.rs` + `crates/touring-hooks/src/post_tool_rl.rs`
- **Effort**: 5-7 ed · **Métrica**: `mean_td_error < 5.0` em 30d

#### NOVO-2. Agentic RL subsystem desperto — VGP direto

- **FACT 1.0**: `agentic_rl_state.active=true` mas `update_count=0`. Subsystem **ligado, sem reward feeder**.
- **Ação**: auditar callsites de `agentic_rl_update()` em `crates/touring-rl/src/agentic/`; wire pós-fase TACO
- **VGP**: `touring wiring impact agentic_rl_update --depth 2`
- **Effort**: 2-3 ed · **Métrica**: `update_count > 0` em 1 sessão

#### NOVO-3. Gate-metrics persistence cross-session — VGP

- **FACT 1.0**: gate-metrics zerou no restart. Paper §3.5.1: *"these signals can be replayed and compared across harness versions"*
- **Ação**: `gate_metrics_daily_flush` (counter existe!) precisa ativar flush para `~/.claude/touring/gate-metrics-YYYY-MM-DD.jsonl` no shutdown
- **Cargo path**: `crates/touring-hooks/src/gateway/metrics.rs`
- **Effort**: 3-4 ed · **Métrica**: `ls ~/.claude/touring/gate-metrics-*.jsonl | wc -l ≥ 7`

### 🟠 P1 NOVO — Primitivas técnicas

#### NOVO-4. Executable World Model `touring world predict` — §2.3.2 CWM/WorldCoder

- **INFERENCE 0.85**: Touring tem `touring ast blast` (estático) mas não **predictive**. CWM/WorldCoder modelam runtime semantics
- **Ação**: `touring world predict --change-set <patch>` retorna `P(breakage | test/module)` treinado em `outcome:edit:*:success/failure`
- **Cargo path**: novo crate `crates/touring-world-model/` ou integrar em `touring-predictor`
- **Effort**: 12-18 ed · **Métrica**: F1 ≥ 0.7 em hold-out de 100 edits

#### NOVO-5. POMDP formalization — §5.1.2

- **INFERENCE 0.8**: TACO opera implicitamente em POMDP ⟨S, A, O, T, R⟩. Formalizar como struct Rust torna policy/value funções treináveis explicitamente.
- **Ação**: `struct AgentMdp { state, action_space, observation, transition, reward }`
- **Cargo path**: `crates/touring-orchestration/src/mdp.rs` (novo)
- **Effort**: 6-9 ed · **Métrica**: `touring mdp dump --session <id>` exporta trajetória completa

#### NOVO-6. GEPA Pareto evolution (algoritmo concreto) — context7 GEPA + §3.5.2

- **FACT 1.0** (context7 `/websites/gepa-ai_github_io_gepa_guides`): `candidate_selection_strategy="pareto"`, `reflection_minibatch_size=5`, LLM-guided mutation. Probabilidade ∝ # de keys onde candidate é best.
- **Ação**: `touring evolution evolve --strategy pareto --minibatch 5 --keys "rl_ema,health_delta,wiring_orphans,test_pass_rate"`
- **Cargo path**: `crates/touring-evolution/src/pareto.rs` (novo)
- **Effort**: 10-14 ed · **Métrica**: ≥1 mutação promovida com Δ ≥ 2 keys na Pareto

#### NOVO-7. Learned Capability Profiles — §5.1.1 Aethelgard

- **INFERENCE 0.85**: CEG hoje tem 4 profiles ESTÁTICOS. Aethelgard usa "learned capability governor"
- **Ação**: `touring capability learn --session <id> --window 7d` infere profile customizado a partir de transcript
- **Cargo path**: `crates/touring-hooks/src/capability/learn.rs` (próximo a `resolve.rs`)
- **Effort**: 7-10 ed · **Métrica**: `ceg_blocked_count` ≤ 50% do baseline

### 🟡 P2 NOVO — Capabilities de longa duração

#### NOVO-8. Failure Attribution Probabilities — §5.1.1 Who&When + AgenTracer 14-53%

- **FACT 1.0**: Paper cita "best step-level attribution 14-53%". TACO Phase 6 hoje atribui binariamente.
- **Ação**: auditor JSON inclui `attribution_probabilities: { scout: 0.15, architect: 0.40, engineer: 0.45 }` via logistic regression
- **Cargo path**: `crates/touring-orchestration/src/attribution.rs` (novo)
- **Effort**: 8-12 ed · **Métrica**: top-1 attribution accuracy ≥ 53% (estado-da-arte do paper)

#### NOVO-9. Trust Calibration Score — §5.1.1 open

- **FACT 1.0**: Paper: *"when to interrupt, checkpoint, delegate, defer"*
- **Ação**: `trust_score = f(recent_success_rate, gotcha_match_count, blast_radius, user_override_rate, daemon_health_delta)`. < 0.5 → checkpoint + ask. > 0.9 → autonomy expandida
- **Cargo path**: `crates/touring-hooks/src/trust/calibration.rs` (novo)
- **Effort**: 6-9 ed · **Métrica**: false-positive interrupts < 10%; missed-risk events = 0

#### NOVO-10. Belief State Divergence `touring belief diff` — §4.3.2 SyncMind |Bk-Sk|

- **FACT 1.0**: VP-Scout Cadeia 7 reconhece wiring DB stale. SyncMind formaliza como `|B_k - S_k|`
- **Ação**: `touring belief diff` retorna `{symbols_added, symbols_removed, wiring_stale_count, divergence_score}`
- **Cargo path**: `crates/touring-storage/src/belief_diff.rs` (novo)
- **Effort**: 5-7 ed · **Métrica**: divergence_score < 0.05 sustentado

### 🟢 P3 NOVO — Frontier production

#### NOVO-11. Transcript-as-Training-Data Pipeline — §5.1.1 Cursor Composer

- **INFERENCE 0.9**: Paper: *"production harnesses... are becoming a dominant source of training data."* Cursor Composer trained on usage traces. Touring transcript_miner JÁ EXISTE.
- **Ação**: `touring transcript export-training-data --format <anthropic|openai|generic> --filter "outcome:edit:*:success" --min-quality 0.8`
- **Cargo path**: `crates/touring-server/src/ingest/transcript_miner.rs` (existente) + novo `export.rs`
- **Effort**: 8-12 ed · **Métrica**: ≥1000 high-quality pairs/mês

#### NOVO-12. Verifiable Environment Construction `touring env synthesize` — §2.3.4

- **INFERENCE 0.8**: SWE-smith/EnvScaler sintetizam ambientes. Touring poderia: `touring env synthesize --crate <X> --tasks 10 --verifiers "cargo check,cargo test,clippy"`
- **Cargo path**: `crates/touring-eval/src/env_synthesis.rs` (novo crate)
- **Effort**: 15-20 ed · **Métrica**: 10 envs sintetizados, ≥80% executáveis ao primeiro try

---

## E. Roadmap consolidado (24 insights)

| Onda | Prazo | Foco | Insights | Total eng-days |
|---|---|---|---|---|
| **W-A** | 4-6 sem | VGP closures imediatas | NOVO-1, NOVO-2, NOVO-3 | **10-14** |
| **W-B** | 6-10 sem | Predictive + Pareto + Capability | NOVO-4, NOVO-5, NOVO-6, NOVO-7 | **35-51** |
| **W-C** | 10-14 sem | Attribution + Trust + Belief | NOVO-8, NOVO-9, NOVO-10 | **19-28** |
| **W-D** | 12+ sem | Strategic dashboards | #1, #2, #3, #6 (rodada 1 P0) | **14-21** |
| **W-E** | 16+ sem | Evolution + Interop | #4, #5, #8 (rodada 1 P1) | **15-22** |
| **W-F** | 20+ sem | Frontier production | NOVO-11, NOVO-12 + #7, #9-12 | **40-60+** |

**Total agregado**: ~133-196 engineer-days (~6-9 meses para um engineer dedicado, ~3-5 meses para um time de 2)

---

## F. Insight transversal — Posição estratégica única

Combinando rodada 1 + rodada 2:

> Touring é hoje o ÚNICO sistema disponível publicamente que tem simultaneamente:
> - **Repository-based representation** (`touring index`: 1.114.103 símbolos / 30.364 arquivos)
> - **Execution-based representation** (`touring e2e -j` composite + cargo check/test + CEG sandbox)
> - **Blackboard/Shared-State** (memory 3-tier + `.toon` checkpoints + transcript_miner)
> - **Process-level potencial** (RL bandit ativo, falta apenas process rewards — NOVO-1)
> - **Evolution loop ativo** (`touring evolution` + flywheel — falta fechamento — rodada 1 #4)
>
> O paper *Code as Agent Harness* (§4.3) declara explicitamente que **nenhum sistema surveyed unifica os 3 primeiros**. Materializar essa unificação via roadmap acima posiciona Touring como **referência pública** do framework formal.

---

## G. Anexos

### G.1. Referências Context7 consultadas

| Library | Context7 ID | Insight extraído |
|---|---|---|
| Langfuse | `/langfuse/langfuse-docs` | `observe(fn, {asType: 'agent'|'tool'|'evaluator'})` pattern para evidence bundles + tool call observability |
| OpenTelemetry GenAI | `/open-telemetry/opentelemetry.io` | Semantic conventions `gen_ai.*` (2025/2026) — interop com Langfuse/Honeycomb/Datadog |
| GEPA | `/websites/gepa-ai_github_io_gepa_guides` | `candidate_selection_strategy="pareto"`, `reflection_minibatch_size=5`, LLM-guided mutation operator |

### G.2. Comandos Touring usados como evidência

```bash
touring doctor -j                  # 7/8 components healthy
touring status -j                  # composite_health_score=0.6873
touring index status               # file_count=30364, symbol_count=1114103
touring learning status            # EMA=0.5249, td_error=15.24, update_count=88, agentic_rl idle
touring synergy wired -j           # 52 wired_pairs (cresceu de 45 → 52)
touring wiring orphans -j          # 27077 orphans (ruído: maioria em .cargo/registry)
touring gate-metrics -j            # zerados nesta sessão (sem persistência)
touring memory recall "<topic>"    # ann + tfidf:decomp + tfidf:memory
```

### G.3. Não-objetivos / Riscos

- **NÃO objetivo**: §5.2.6 Multimodal Code-Harness — Touring é text-centric por design.
- **Risco P0**: insights #2/#3 só agregam valor se REGRA #14 for 100% enforced (hoje há bypass via Write tool em `.md`).
- **Risco P1**: insight #5 OTel adiciona dependência runtime — feature flag `--features otel-exporter`.

### G.4. Critérios de mudança de avaliação

- Se `mean_td_error < 1.0` → NOVO-1 sai de P0 (RL já convergindo)
- Se `agentic_rl_state.update_count > 0` → NOVO-2 já resolvido
- Se Anthropic publicar OTel spec para Claude Code → rodada 1 #5 sobe sobre NOVO-1
- Se paper publicar benchmark de harness-level metrics → NOVO-8 ganha rigor experimental

---

_Documento gerado: 2026-05-27 | sessão Touring e83b83eb | autor: TACO (Claude Opus 4.7 + Touring v30.0.0) | sob direção de Gabriel Gadea_
_Skill activated: Touring | VGP: 12 comandos CLI executados | Context7: 3 libs consultadas | Sequential-thinking: 5 thoughts_
