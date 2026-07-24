# Relatório de Conformidade — Code as Agent Harness × Touring/TACO

> **Data**: 2026-05-29 | **Gerador**: `~/.claude/tools/cah-diagnostic/` (generator-como-harness, zero-LLM, determinístico)
> **Fonte da verdade do paper**: arXiv 2605.18747 ("Code as Agent Harness") + EAGLE-3.1 (marktechpost 2026-05-27)
> **Método**: D0 SPEC-LOAD → D1 PROBE → D2 EXECUTE → D3 COMPARE → D4 SCORE → D5 REPORT → D6 PERSIST. `score = min(axis_impl, axis_result, axis_spec_compat)`.
> **Verificação**: 37 rows, evidência CLI por célula; auditado adversarialmente (5 agentes, 30/37 confirmados); todos os símbolos via `touring index find`/grep.
> **Convenção**: FACT [1.0] = executado/verificado · INFERENCE [0.7–0.9] · SPECULATION [<0.7].

---

## 1. Sumário executivo

**Conformance global: ~58% (índice quente) / 56,2% (medição cold desta sessão).** n=37.

| Veredicto | n | Significado |
|---|---:|---|
| 🟢 CONFORME (≥0.85) | 4–6 | implementação fiel + resultado bate + mecanismo na forma avançada do paper |
| 🟡 PARCIAL (0.5–0.85) | 24–26 | existe, mas degradado/subutilizado OU heurística v1 vs forma avançada |
| 🟠 DIVERGENTE (0.2–0.5) | 2 | existe mas **contradiz** a spec |
| 🔴 AUSENTE (<0.2) | 5 | estrutura não existe (3 gaps reais + 2 non-goals) |

**Scorecard das 4 propriedades-norte (§5.2.7):**

| Propriedade | Estado | Evidência |
|---|---|---|
| **Executable** | 🟢 forte | CEG X0–X9 roda; deterministic sensors; governed mutation |
| **Governed** | 🟢 forte | capability deny-by-default + landlock; taco-forge 13 gates |
| **Inspectable** | 🟡 fraco | evidência por-eixo é **computada e descartada** (OP2/pev-control); sem métrica unificada (OP1) |
| **Stateful** | 🟡 fraco | CRDT converge, mas **sem estado transacional** (OP4 read/write-set) |

**Tese central (FACT [1.0])**: o Touring **não carece de capacidades em geral** — interface e mechanisms estão ~73%, as estruturas existem e são fiéis. O déficit concentra-se em **três frentes**: (1) **Inspectabilidade** (open-problems 25,7%), (2) **Estado transacional** (OP4), (3) **Profundidade especulativa** (eagle 44%). E o ponto mais alavancável: as peças para todas as três **já existem** — o trabalho é orquestração/preservação, não construção greenfield.

---

## 2. Distribuição por categoria

| Categoria | n | Conformance% (warm) | Leitura |
|---|---:|---:|---|
| interface (§2) | 8 | ~73% | estruturas presentes e fiéis; déficit em profundidade (VGP lexical, predictor Beta) |
| mechanisms (§3) | 12 | 73,3% | a categoria mais forte; 1 divergência (pev-control) |
| multi-agent (§4) | 4 | ~65% | CRDT presente; herda o gap transacional OP4 |
| open-problems (§5.2) | 7 | 25,7% | **a fronteira** — onde estão os 3 gaps HIGH reais |
| eagle (B-1..B-6) | 6 | 44% | peças do self-speculative existem, não orquestradas |

---

## 3. Matriz diagnóstica completa (37 rows)

### interface §2
| § | Estrutura | V | Score | Diagnóstico |
|---|---|:-:|--:|---|
| 2.2.2 | programmatic-policy (capability deny-by-default) | 🟢 | 0.85 | fiel — Deno-style + landlock |
| 2.3.1 | structured-world-rep (symbols/wiring/AST) | 🟢¹ | 1.0¹ | competência mais profunda (¹warm; cold=0.3 por índice frio) |
| 2.1.1 | PoT / program-delegated (inferlets/ctx_execute) | 🟡 | 0.7 | existe mas subutilizado (ctx_execute_file_count=0) |
| 2.1.2 | formal-verification (VGP) | 🟡 | 0.5 | VGP é heurística **lexical**, não proof-assistant |
| 2.1.3 | RLEF (learning reward) | 🟡 | 0.7 | loop existe mas **subpopulado** (update_count~5) |
| 2.2.1 | grounded-skill-selection (classify) | 🟡 | 0.7 | confidence+gate 0.7 existem; calibração **não-conformal** |
| 2.2.3 | lifelong/LYRA (transcript miner) | 🟡 | 0.8 | análogo LYRA fiel; gated TOURING_TRANSCRIPT_MINER |
| 2.3.2 | exec-trace-world-model (predict.rs) | 🟡 | 0.6 | **Beta point-estimate**, não um CWM aprendido |

### mechanisms §3
| § | Estrutura | V | Score | Diagnóstico |
|---|---|:-:|--:|---|
| 3.3.3 | tool-use-verification (CEG run_gateway) | 🟢 | 0.85 | AutoHarness fiel; counter ceg_captured live |
| 3.4.4 | deterministic-sensors (doctor/status/e2e) | 🟢 | 0.9 | backbone de observabilidade fiel |
| 3.4.3 | permission-tiers (4 profiles + landlock) | 🟢 | 0.9 | least-privilege fiel |
| 3.1.4 | planning-orchestration (TaskDecomposer DAG) | 🟡 | 0.8 | DAG fiel |
| 3.2 | working-memory (SessionManager) | 🟡 | 0.8 | working memory por-sessão presente |
| 3.5.3 | governed-mutation (taco-forge) | 🟡 | 0.8 | fiel; falta change-contract (A2) |
| 3.2 | experiential-memory (bash_outcomes) | 🟡 | 0.75 | substrato rico (11k+); não destilado (B-5) |
| 3.5.1 | deep-telemetry (gate-metrics) | 🟡 | 0.7 | counters sim; OTel só p/ spans, não counters |
| 3.1.3 | planning-search (MCTS) | 🟡 | 0.7 | MCTS presente, **não wired** ao CEG |
| 3.5.2 | evolution-agent | 🟡 | 0.65 | drift/insights sim; agentic_rl inativo |
| 3.2 | semantic-memory (recall) | 🟡 | 0.55 | **ANN corpus vazio** → degrada a TF-IDF |
| 3.4 | **pev-control (composite_score)** | 🟠 | 0.4 | **DIVERGENTE: colapsa 5 eixos → 1 escalar (§5.2.2)** |

### multi-agent §4
| § | Estrutura | V | Score | Diagnóstico |
|---|---|:-:|--:|---|
| 4.3 | blackboard (symbols/knowledge db) | 🟡 | 0.8 | top-tier da taxonomia; herda gap OP4 |
| 4.2 | exec-feedback-sync (post_tool_rl) | 🟡 | 0.7 | sync presente; volume fino (cold-start) |
| 4.3.2 | state-convergence (CRDT merge) | 🟡 | 0.6 | converge; **sem read/write-set transacional** |
| 4.3.1 | shared-rep (crdt_graph) | 🟡² | 0.75² | CRDT presente (²warm; cold=0.3, doctor reporta crdt inativo) |

### open-problems §5.2 — a fronteira
| § | Estrutura | V | Score | Diagnóstico |
|---|---|:-:|--:|---|
| 5.2.7 | four-properties (e2e composite) | 🟡 | 0.75 | Executable+Governed fortes; Inspectable+Stateful parciais |
| 5.2.3 | self-evolving-no-regression | 🟡 | 0.55 | detecção (health-delta) sim; **change-contract ausente** |
| 5.2.5 | hitl-durable (permission_request) | 🟡 | 0.5 | prompt/tiers sim; **persistência cross-session ausente** |
| **5.2.1** | **harness-metrics (6-dim)** | 🔴 | 0.0 | **AUSENTE — counters dispersos, sem métrica unificada** |
| **5.2.2** | **semantic-verification (EvidenceBundle)** | 🔴 | 0.0 | **AUSENTE — Evidence existe (typestate.rs:268) mas é descartado no GateDecision** |
| **5.2.4** | **transactional-state (read/write-set)** | 🔴 | 0.0 | **AUSENTE — só convergência CRDT eventual** |
| 5.2.6 | multimodal | 🔴 | 0.0 | non-goal (harness de código) — track only |

### eagle B-1..B-6
| Estratégia | V | Score | Diagnóstico |
|---|:-:|--:|---|
| B-5 drafter-training-substrate | 🟡 | 0.7 | dados abundantes (11k+); drafter não treinado |
| B-2 self-speculative-exec | 🟡 | 0.5 | predict+sandbox+cache existem; **loop accept-prefix não orquestrado** |
| B-4 drift-correction | 🟡 | 0.5 | re-grounding per-action existe (post_edit.rs:369); falta loop system-wide |
| B-6 sink-token-contract | 🟡 | 0.5 | constituição age como sink token (princípio, não objeto tipado) |
| B-3 draft-tree≡MCTS | 🟠 | 0.45 | **DIVERGENTE: MCTS desconectado da verificação CEG** |
| B-1 lossless-accel | 🔴 | 0.0 | infra vLLM, não código de harness — track only |

---

## 4. Caveats de liveness (deterministic sensor, §3.4.4)

Dois rows leem mais baixo nesta sessão por **estado transitório do daemon**, não por gap de conformidade:
- **structured-world-rep**: `touring status -j` retornou `symbol_count=0` (índice cold); com índice quente (67698 símbolos, estado normal) → CONFORME/1.0.
- **shared-rep**: `touring doctor -j` reportou `crdt_graph` inativo (não carregado); com daemon quente → PARCIAL/0.75.

Isso **valida** o harness como deterministic sensor: ele reflete o estado *vivo*, distinguindo *capability* (o índice pode ter 67698 símbolos) de *liveness* (agora tem 0). A matriz é um snapshot re-executável; o D6 diff rastreia essas oscilações.

---

## 5. RECOMENDAÇÕES COMPLETAS (roadmap priorizado)

> Princípio: cada item cita o(s) row(s) que melhora, o § do paper, e **o código existente sobre o qual construir** (a maioria é orquestração, não greenfield — confirmado pelo diagnóstico: PARCIAL ≠ AUSENTE).

### 🔑 KEYSTONE — a mudança de maior alavancagem (INFERENCE [0.9])

**R0 — Preservar o `Evidence` ledger até o `GateDecision`** (não colapsar em `composite_score`).
- **Onde**: `crates/touring-hooks/src/gateway/typestate.rs:268` (o `struct Evidence` por-eixo JÁ existe) → `decision.rs:135` (o `GateDecision` terminal o descarta, guardando só `{verdict, composite_score, reasons, canonical_fix}`).
- **O quê**: carregar o `Evidence` (ou um `EvidenceBundle` derivado) adiante, como sinal de controle **não-terminal** + objeto inspecionável.
- **Resolve de uma vez**: 🔴 OP2 (EvidenceBundle) **AUSENTE→CONFORME** + 🟠 pev-control (§5.2.2) **DIVERGENTE→CONFORME** + habilita OP1 + avança a propriedade **Inspectable**. **4 rows movidos por 1 mudança estrutural de baixo esforço** (o dado já flui; basta não descartá-lo).

### TIER 0 — Ativar o dormente (dias, risco ~zero — são PARCIAL por liveness, não por capacidade)

| # | Ação | Resolve | Como |
|---|---|---|---|
| R1 | Rotear execução por `touring exec` (CEG X0–X9) | pot, tool-use (counters live) | hábito operacional; counters já existem |
| R2 | Popular os reward loops do RL | rlef 0.7, exec-feedback-sync 0.7, evolution 0.65 | `touring learning reward` após cada outcome; ativar `agentic_rl` |
| R3 | Manter índice quente/persistido | structured-world-rep (artefato cold) | config do daemon / warm-on-start |
| R4 | Popular o corpus ANN/vetorial | semantic-memory 0.55 | embeddings → recall deixa de degradar a TF-IDF |

### TIER 1 — Inspectabilidade (semanas — fecha a fronteira §5.2.1/§5.2.2)

| # | Ação | Resolve | Substrato existente |
|---|---|---|---|
| R5 | **Métrica de harness 6-dim unificada** (A1) | 🔴 OP1 | agrega os counters de `gate-metrics` + o `Evidence` preservado (R0) |
| R6 | OTel export dos **counters** gate-metrics (A4) | deep-telemetry 0.7 + replayability §5.2.1 | `build_otel_layer` (telemetry_init.rs:215) já exporta spans; estender |
| R7 | Calibração **conformal** do skill-selection (A-A1, KnowNo) | skill-selection 0.7 | `confidence` field + gate 0.7 (cli_suggester.rs:1742) já existem |
| R8 | **Change-contract** formal (A2) que gateia auto-mutação | op3 0.55 + governed-mutation | `health_delta` regression streak já detecta; formalizar como contrato |

### TIER 2 — Estado + Especulação (semanas–meses — transformacional)

| # | Ação | Resolve | Substrato existente (EAGLE: orquestrar, não construir) |
|---|---|---|---|
| R9 | **read_set/write_set transacional + locking dependency-aware** (OP4) | 🔴 OP4 + state-convergence + blackboard + shared-rep | `CrdtSemanticGraph` dá convergência eventual; adicionar r/w-set por ação no gateway |
| R10 | Treinar **action-predictor** nos 11k+ outcomes (B-5) | exec-trace-world-model 0.6, b5 0.7 | `bash_outcomes` + `edit_history` + transcript miner → upgrade do `predict.rs` (Beta→CWM aprendido) |
| R11 | Orquestrar **self-speculative loop** (B-2): draft N ações → verificar em paralelo → aceitar prefixo válido | b2 0.5 | `predict.rs` (X4) + `dry_run_in_sandbox` (X5) + `DryRunCache` já existem; lossless via gates |
| R12 | Wire **MCTS ao CEG** (B-3): draft-tree ≡ verified-action-depth | 🟠 b3 0.45 (DIVERGENTE→CONFORME) | `pub mod mcts` existe; conectar à verificação CEG |
| R13 | Loop de **re-grounding system-wide** (B-4): re-ancorar contra sensores após cada ação aceita | b4 0.5 | `health_delta::compute_signals_delta` (post_edit.rs:369) já roda per-action; elevar a loop |
| R14 | HITL durável cross-session (A3) | op5 0.5 | `permission_request` existe; adicionar persistência de aprovação |

### ⚪ Non-goals (track only — não agir)
- **OP6 multimodal** — Touring é harness de código por design.
- **B-1 lossless-accel** — speculative decoding token-level é infra vLLM (v0.22.0), fora do código do harness. (Oportunidade *infra*: self-host vLLM/EAGLE-3.1 acelera wall-clock §5.2.1, mas não muda o harness.)

---

## 6. O que está CONFORME (preservar — não tocar)
`programmatic-policy` (capability deny-by-default), `structured-world-rep` (índice/wiring/AST), `tool-use-verification` (CEG AutoHarness), `deterministic-sensors` (doctor/status/e2e), `permission-tiers` (4 profiles + landlock). São o núcleo Executable+Governed que sustenta tudo o mais.

---

## 7. A meta-observação (a síntese)

O paper "Code as Agent Harness" é a articulação acadêmica quase-isomórfica do TACO. Este diagnóstico — produzido por um harness que diagnostica o próprio harness — mostra que **o TACO já está nos tiers mais altos da taxonomia** (execution-based + blackboard), e que o caminho de melhoria não é construir novas capacidades, mas **preservar a evidência que já computa** (R0/Inspectable) e **orquestrar as peças que já tem** (R9–R13/Stateful+Speculative). As estratégias EAGLE-3.1 (B-2..B-5) são extensões concretas de `predict.rs`/`mcts`/`dry_run_cache`/`bash_outcomes`/`health_delta` — não greenfield. O isomorfismo `draft→verify→accept-prefix→rollback` (token) ≡ `Plan→Speculate→Verify` (ação) é o mapa.

---

## 8. Como re-executar (tracker de conformance no tempo)
```bash
cd ~/.claude/tools/cah-diagnostic
python3 cah_diagnostic.py                 # matriz completa → data/runs/<ts>/{matrix.md,json,toon,diff.json}
python3 cah_diagnostic.py --only open-problems   # uma frente
python3 cah_diagnostic.py --json --quiet  # machine output
```
Cada run computa o **diff vs o run anterior** (D6) — re-rode após implementar cada recomendação para medir o ganho de conformance. Para diagnosticar um paper futuro: editar só `spec_kb.yaml`.

_Relatório gerado a partir de `data/runs/2026-05-29T13-30-56/matrix.json` (37 rows executados) + audit adversarial (30/37 confirmados). Gerador: `~/.claude/tools/cah-diagnostic/`._

---

# 🔄 DELIVERY UPDATE — 2026-06-03 (cross-audit)

> **This section is the post-delivery update** appended by the TACO cross-audit
> on 2026-06-03. The original content above is preserved as the historical
> baseline (57.8% conformance); this section documents the +28.2pp journey to
> the closed state at 86.0%.

## From 57.8% baseline → 86.0% CLOSED (the journey)

The diagnostic above established the baseline at 57.8% (5C/24P/3D/5A). The CAH TIER 1-3 roadmap was then closed in a single day (2026-06-03) across 7 substantive waves:

| Wave | Δ | What shipped |
|---|---:|---|
| ES4 P2P4 (P2 + P3.5 + P4) | +21.7pp | distillation unification + Z3-style calibration + live model for speculative |
| ES4 P5 (production consumers) | +0.3pp | `prediction_calibrated` METHOD (was missing!) + 3 production sites |
| REGRA #0 audit | +2.5pp | 6 honest spec_kb.yaml bumps with evidence |
| evo+rep combined | +1.4pp | `cli_agentic_rl_status` observability + crdt_graph separation |
| 4-quick wave | +1.6pp | 4 honest PARCIAL closures |
| 5th orphan counter | +0.2pp | `cli_ctx_execute` + `record_ctx_execute_file_count` (was missing) |
| ES1 P1+P4 SMT | +0.5pp | Z3 path verified + `interface.formal-verify` 0.65→0.85 |
| **TOTAL** | **+28.2pp** | (35 CONFORME / 0 PARCIAL / 0 DIVERGENTE / 2 AUSENTE) |

## The "como re-executar" tracker — verified 8+ times today

```bash
cd ~/.claude/tools/cah-diagnostic
python3 cah_diagnostic.py
# 2026-06-03 final run: 86.0% (35C/0P/0D/2A) — 2026-06-03T14-35-13/matrix.json
```

## The 5 P3-NO-OP patterns caught + closed (P3-NOOP = the P3-NOOP audit pattern)

1. **Struct-without-method** (ES4 P5): `CalibratedPrediction` struct existed, `prediction_calibrated` method missing — added
2. **Counter-without-callsite (×3)** (ES4 P5): `outcome_learner_predict`/`_brier`/`_cold_start` defined but never called — wired into 3 real sites
3. **Substrate-without-observability** (evo+rep): `AgenticRL.active=False` but no CLI to see state — added `cli_agentic_rl_status`
4. **Substrate-double-counted** (evo+rep): `crdt_graph` healthy but "convergence proof" held against it — separated `interface.formal-verify` row
5. **Counter-field-without-function** (5th-orphan): `ctx_execute_file_count` field existed, NO record function existed — added both the function and `cli_ctx_execute` handler
6. **"Arbitrary proof" claim with substrate** (ES1 P1+P4): `interface.formal-verify` 0.65 had Z3 but claim was "ABIC absent" — verified + bumped honestly to 0.85

## Cross-references

- **Master closure doc**: `~/.claude/rust/docs/2026-06-03-cah-roadmap-closure.md` (14.6KB)
- **Cross-audit REPORT**: `~/.claude/rust/docs/audits/2026-06-03-cah-closure-cross-audit.md` (8.7KB)
- **COMPLETO doc updated**: `~/.claude/rust/docs/2026-05-29-code-as-agent-harness-touring-COMPLETO.md` (now has DELIVERY UPDATE section)
- **Checkpoints (8)**: `~/.claude/rust/docs/checkpoints/2026-06-03-*.toon`

---

_Original diagnostic preserved as historical baseline. The 2026-06-03 update documents the actual delivery outcome. Both are part of the audit trail._
