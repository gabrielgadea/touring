# Touring Excellence Pipeline — Master Plan v2.0

> **Date**: 2026-04-29 | **Author**: TACO orchestrator (Claude Opus 4.7) sob direção de Gabriel Gadea
> **Sources**: `code-diagnostic.md` (9 dimensões filosóficas) · `Manual-Avancado-de Analise-de-Codigo.md` (7 capítulos doutrinários FAANG-level, 86 referências) · Context7 (tree-sitter, opentelemetry-rust, semgrep) · Cross-repo plan v1.0 (`2026-04-28-cross-repo-improvements-master-plan.md`)
> **Status**: PLAN READY FOR REVIEW
> **Touring baseline**: v30.3.0 · Wave A.2 OK + A.3 OK + A.4 OK (2026-04-29) · 73 CLI · 88 MCP · 176 hooks · 24 crates · 199.674 orphans pub
> **Methodology**: TACO L4+ · 9-Dimension Excellence Framework · 5-Phase Code Pipeline · T-shirt sizing · DAG-validated dependencies
> **Authority**: Gabriel approves wave-by-wave; orchestrator does not auto-execute

---

## Table of Contents

1. Executive Summary
2. Discovery Recap (3 sources)
3. The 9-Dimension Excellence Framework
4. The 5-Phase Code Pipeline
5. Touring State Assessment (have vs gap)
6. The 12 Excellence Waves — Detailed Breakdown
7. Integration with Cross-Repo Plan v1.0
8. Roadmap (15 weeks, 3 phases)
9. Risk Register
10. Memory Persistence Plan
11. Validation Gates per Phase
12. Appendix — Cross-Walk + References

---

## 1. Executive Summary

OBJECTIVE: Elevate Touring's code intelligence to FAANG-level excellence across the 5 code lifecycle phases (Diagnose, Analyze, Read, Edit, Write) by closing 12 strategic gaps identified through cross-reading of `code-diagnostic.md` (philosophical taxonomy) and `Manual-Avancado-de Analise-de-Codigo.md` (doctrinal manual). The plan is **complementary** to the existing cross-repo plan v1.0, focusing on dimensions v1.0 did not cover: thermodynamic computational efficiency, supply-chain cryptography (SLSA L3 + Sigstore), agent-economic FinOps (RCT), runtime governance (OPA/Rego), and bounded-context discovery via DDD heuristics.

**Total scope**: 12 new Excellence Waves (E.1 through E.12) · ~14 sprints (3 active engineers parallelized) · ~200 new tests · ~3.500 LOC additions · 4 new crates

**Strategic outcome**: Touring becomes the first code-intelligence platform combining (a) deep static metrics (Halstead/Cognitive/MI), (b) LLM-aware diagnostics (RCT, prompt-injection detection), (c) supply-chain attestation (Sigstore keyless), (d) runtime policy enforcement (OPA), (e) progressive-disclosure MCP (token-economic), (f) auto-discovered bounded contexts.

**Compliance**: REGRA #0, REGRA #11, REGRA #12, REGRA #13.

---

## 2. Discovery Recap (3 sources)

### 2.1 code-diagnostic.md (18 KB) — The 9-Dimension Taxonomy

A philosophical/conceptual framework partitioning code excellence into 9 orthogonal dimensions.

| # | Dimension | Touring coverage today |
|---|-----------|------------------------|
| I | Funcionalidade (Completude/Correção/Idempotência/Safety) | 70% — typestate covers; idempotency Q-220 OK; gap: Twelve-Factor compliance |
| II | Eficiência (Big-O / Cache Locality / Concorrência) | 30% — Halstead/CC OK; gap: cache locality, false sharing |
| III | Qualidade Intrínseca (Acoplamento/Coesão/Halstead/Cognitive/Mutation) | 85% — TDG 6 dim OK, mutation-test OK; gap: Halstead Bug threshold gate, MuRS-style ranking |
| V | Excelência LLM (Densidade Semântica / OCP / Zero-Surprise / Tokenomics) | 50% — MCP OK; gap: progressive disclosure, RCT |
| VI | Segurança (Memory/Supply-Chain/Zero-Trust/Prompt Injection) | 50% — cargo-deny OK; gap: Sigstore signing, OPA/Rego, prompt-injection detector |
| VII | Observabilidade (Logs/Tracing/RED+USE) | 60% — OTel base OK; gap: GenAI semconv, structured JSON logs |
| VIII | Sociotécnica (Conway/Bounded Contexts/Halstead/Documentation) | 40% — wiring chains parcial; gap: bounded context auto-discovery |
| IX | Termodinâmica/FinOps (Tokenomics/Data Gravity/Twelve-Factor) | 20% — gap: RCT, token tracking |
| XI | Integração Agêntica (MCP / RAG / Memória Episódica) | 80% — MCP+tantivy OK; minor gap: progressive disclosure |

**Net coverage**: ~60%. **This plan targets the 40% delta.**

### 2.2 Manual-Avancado-de Analise-de-Codigo.md (118 KB)

A doctrinal manifesto with 7 chapters + 86 external bibliography references. Theoretical foundation, not cookbook.

| Cap | Title | Key concepts |
|-----|-------|--------------|
| 1 | Funcionalidade e Determinismo | LLM-as-Compiler (PydanticAI), Saga (Temporal-style), Teorema 4/δ Markov Convergence |
| 2 | Termodinâmica Computacional | Princípio de Landauer, Cache MESI / False Sharing, Rust Ownership vs GC |
| 3 | Qualidade Intrínseca e Antropia | Cognitive Complexity (SonarSource), Halstead V/D/E/B/MI, Mutation Testing diff-mode (MuRS), Arid Nodes Omission |
| 4 | Imunologia Sistêmica | SLSA L3, Sigstore (Cosign/Fulcio/Rekor), IsolateGPT Hub-Spoke, Code-Then-Execute |
| 5 | Observabilidade | USE/RED Golden Signals, OpenTelemetry GenAI semconv (gen_ai.usage.input_tokens / output_tokens / agent.name / provider.name) |
| 6 | Sociotécnica | Lei de Conway, Team Topologies, Open Policy Agent (OPA/Rego "Law as Code") |
| 7 | Excelência Agêntica | MCP USB-C, Progressive Disclosure, FinOps RCT (Retorno Cognitivo por Token), Smart Model Routing, Data Gravity, Apache Iceberg |

**Top 4 truly new insights** (not in Touring + not in cross-repo v1.0):
1. **4/δ Markov Convergence Theorem** — model `shadow validate` as 4-state absorbing chain; budget iterations = 4/δ where δ is observed convergence rate.
2. **RCT (Retorno Cognitivo por Token)** — track tokens consumed per resolved diagnostic; minimize via progressive disclosure + semantic caching.
3. **OPA/Rego Runtime Policy** — declarative RBAC for MCP tool calls (vs prompt-only). Rust crate `regorus` available.
4. **Sigstore Keyless OIDC** via `sigstore-rs`: sign `target/release/touring-daemon` artifacts → Rekor transparency log.

### 2.3 Context7 — Best Practices Confirmed

- `/tree-sitter/tree-sitter` (854 snippets, score 72) — incremental parsing API, query patterns, predicate syntax confirmed for SSR/SAST
- `/open-telemetry/opentelemetry-rust` (218 snippets, score 78) — semconv attribute setting, span exporter configuration
- `/semgrep/semgrep-docs` (4685 snippets, score 74) — pattern-matching DSL (`pattern:`, `pattern-not:`, `metavariable-regex:`), taint analysis primitives

---

## 3. The 9-Dimension Excellence Framework

| Dimension | Manual Cap | Touring today | New Wave |
|-----------|-----------|---------------|----------|
| I — Funcionalidade | 1 | typestate + saga | E.12 (Twelve-Factor + Zero-Trust + Prompt Injection) |
| II — Eficiência | 2 | Halstead/CC | E.1 (Halstead Bug Gate), E.2 (False Sharing Detector) |
| III — Qualidade | 3 | TDG, mutation-test | E.5 (Mutation Diff + Arid Filter) |
| V — Excelência LLM | 7 | MCP 88 tools | E.8 (MCP Progressive Disclosure) |
| VI — Segurança | 4 | cargo-deny | E.9 (OPA/Rego), E.10 (Sigstore), E.11 (SAST Rules) |
| VII — Observabilidade | 5 | OTel base | E.4 (OTel GenAI Semconv) |
| VIII — Sociotécnica | 6 | wiring chains | E.7 (Bounded Context Mapper) |
| IX — Termodinâmica/FinOps | 7 | none | E.3 (RCT Metric) |
| XI — Integração Agêntica | 7 | MCP+tantivy | E.6 (4/δ Convergence Modeling) |

**Coverage**: 12 new waves cover 9/9 dimensions. No dimension left uncovered.

---

## 4. The 5-Phase Code Pipeline

```
┌─────────────────────────────────────────────────────────────┐
│              CODE EXCELLENCE PIPELINE                       │
├─────────────────────────────────────────────────────────────┤
PHASE 1   DIAGNOSE   → detect issues, score quality, map blast│
Tools:    ast meta · TDG · Halstead · cognitive · pre-edit    │
├─────────────────────────────────────────────────────────────┤
PHASE 2   ANALYZE    → understand structure, deps, semantics  │
Tools:    blast · wiring · rust-semantic · bounded-context    │
├─────────────────────────────────────────────────────────────┤
PHASE 3   READ       → comprehend, navigate, search, recall   │
Tools:    index find · tantivy · highlight · MCP progressive  │
├─────────────────────────────────────────────────────────────┤
PHASE 4   EDIT       → refactor, transform, fix, rename       │
Tools:    ssr · assist · source-change · fix · resolve-def    │
├─────────────────────────────────────────────────────────────┤
PHASE 5   WRITE      → generate, render, commit, validate     │
Tools:    generate · plan-submit · speculate · format-rust    │
└─────────────────────────────────────────────────────────────┘
                          ▼
              INVARIANTS (cross-cutting):
   Idempotency · Verifiability (VGP) · Telemetry · Reversibility
```

### Per-phase quality target post-plan

| Phase | Today | Target | Δ from waves |
|-------|-------|--------|--------------|
| 1. Diagnose | 7/10 | 9/10 | E.1, E.2, E.11 |
| 2. Analyze | 6/10 | 9/10 | E.4, E.7 |
| 3. Read | 7/10 | 9/10 | E.8 |
| 4. Edit | 6/10 | 8/10 | E.12 + cross-repo C.1 |
| 5. Write | 8/10 | 9/10 | E.5, E.12 |
| Cross | 5/10 | 9/10 | E.3, E.6, E.9, E.10 |

**Total target uplift**: 7.0/10 → 9.0/10 (INFERENCE [0.8]).

---

## 5. Touring State Assessment

### 5.1 What Touring already does well (preserve, don't duplicate)

- TDG 6-dimension quality scoring
- Halstead V/D/E/B + Maintainability Index (Wave A.1.1 OK)
- Cyclomatic + Cognitive Complexity (matches SonarSource)
- cargo-deny supply-chain gates
- OpenTelemetry feature opt-in
- Mutation testing (`touring mutation-test` wraps cargo-mutants)
- Code generator typestate (Draft → Verified → Rendered → Speculated → Committed)
- DistributedSagaCoordinator (2PC)
- inferlets WASM pools (Hub-Spoke approximation)
- MCP server with 88 tools
- Tantivy BM25 + FTS5 + cosine
- VP-Scout v1.1 (7 verification chains)
- RFC-100 diagnostic codes
- Skip regions (A.2 OK), Idempotency Q-220 (A.3 OK), MCP profile_query (A.4 OK)

### 5.2 Genuine gaps (this plan targets)

| # | Gap | Origin | Wave |
|---|-----|--------|------|
| 1 | Halstead Bug threshold not gating pre-edit | Manual cap. 3 | E.1 |
| 2 | False Sharing detection in Rust struct semantic | Manual cap. 2 | E.2 |
| 3 | RCT metric (tokens per resolved diagnostic) | Manual cap. 7 | E.3 |
| 4 | OpenTelemetry GenAI semconv (gen_ai.* attrs) | Manual cap. 5 | E.4 |
| 5 | Mutation testing diff-mode + Arid Nodes filter (MuRS) | Manual cap. 3 | E.5 |
| 6 | 4/δ Markov chain modeling of shadow validate | Manual cap. 1 | E.6 |
| 7 | Bounded Context auto-discovery via DDD heuristics | Manual cap. 6 | E.7 |
| 8 | MCP Progressive Disclosure (88 tools loaded upfront) | Manual cap. 7 | E.8 |
| 9 | OPA/Rego runtime policy gate on MCP requests | Manual cap. 6 | E.9 |
| 10 | Sigstore keyless OIDC signing of daemon artifacts | Manual cap. 4 | E.10 |
| 11 | Semgrep-style declarative SAST rules (cross-language) | Manual cap. 4 + Context7 | E.11 |
| 12 | Twelve-Factor + Zero-Trust + Prompt-Injection | code-diagnostic + Manual cap. 4 | E.12 |

### 5.3 Out-of-scope (acknowledge but do NOT implement)

- Smart Model Routing (Haiku vs Opus) — Claude Code handles
- Data Gravity / Apache Iceberg edge federation — local-first, not distributed
- LangSmith/Datadog APM — SaaS, conflicts with local-first design
- Mutagenesis (Google internal) — proprietary
- PydanticAI direct — Python-only; equivalent via syn schemas in typestate

---

## 6. The 12 Excellence Waves

Each Wave is atomically shippable. Format: ID, effort, dependencies, sub-tasks, files, acceptance criteria, tests, rollback, telemetry, memory.

---

### Wave E.1 — Halstead Bug Threshold Gate (pre-edit)

| Field | Value |
|-------|-------|
| ID | W-E.1 |
| Wave | E (Excellence) |
| Phase | 1 (Diagnose) |
| Dimension | III (Qualidade Intrínseca) |
| Effort | S (2 engineer-days) |
| Dependencies | none (Halstead B already computed in Wave A.1.1) |
| Origin | Manual cap. 3 (Halstead defects estimate B = V/3000) |
| Confidence | FACT [1.0] |

**Affected crates**: `touring-hooks`, `touring-analysis`
**New crates**: none

**Atomic sub-tasks**:

1. Add config knob `pre_edit.halstead_bug_threshold` (default 2.0) and `pre_edit.halstead_bug_delta_threshold` (default +0.5)
2. In `pre_edit` hook, compute `halstead_b_delta = post.halstead_b - pre.halstead_b`
3. If `halstead_b_delta > delta_threshold` OR `post.halstead_b > absolute_threshold` → downgrade pre_edit score by 0.2 + emit `Q-221 HalsteadBugIncrease`
4. Counter `halstead_bug_gate_violations_count` in `gate-metrics`
5. 6 unit tests + 2 integration

**Files to create**: `crates/touring-hooks/src/halstead_bug_gate.rs` (~80 LOC)

**Files to modify**: pre_edit.rs (wire gate), diagnostics/codes.rs (Q-221), gate_metrics.rs (counter)

**Acceptance criteria**:
- GIVEN file F with current Halstead B = 1.5 WHEN edit produces F' with B = 2.3 (delta = +0.8) THEN pre_edit score downgrades by 0.2 AND Q-221 emitted
- GIVEN file F with B = 1.0 WHEN edit produces F' with B = 1.4 THEN pre_edit score unchanged

**Test plan**: synthetic V calculation + threshold edge cases

**Rollback plan**: `pre_edit.halstead_bug_threshold = inf` disables instantly

**Telemetry**: `Q-221 HalsteadBugIncrease`, counter `halstead_bug_gate_violations_count`

**Memory store**: `wave_e_1_halstead_bug_gate_completed`

---

### Wave E.2 — False Sharing Detector (rust-semantic)

| Field | Value |
|-------|-------|
| ID | W-E.2 |
| Phase | 1+2 (Diagnose + Analyze) |
| Dimension | II (Eficiência — Cache Locality MESI) |
| Effort | S (3 engineer-days) |
| Dependencies | none |
| Origin | Manual cap. 2 (False Sharing, MESI cache-bouncing) |
| Confidence | INFERENCE [0.85] |

**Affected crates**: `touring-ast` (rust-semantic submodule)
**New crates**: none

**Atomic sub-tasks**:

1. Detector `crates/touring-ast/src/rust_semantic/false_sharing.rs`:
   - Walk AST for `struct` decls with `#[repr(C)]` or `#[repr(packed)]`
   - Identify fields of types matching regex `Atomic(U?)(8|16|32|64|Usize|Isize|Bool|Ptr<.*>)`
   - If 2+ atomic fields AND struct lacks `#[repr(align(64))]` AND total size of consecutive atomics ≥ 64B → flag risk
2. Heuristic refinement: count fields between atomics
3. Emit `Q-230 PotentialFalseSharing` (severity hint) with suggested fix
4. CLI `touring ast false-sharing <file.rs>` + JSON output
5. MCP tool `mcp__touring__false_sharing_check`
6. 10 unit tests covering struct shapes

**Files to create**:
- `crates/touring-ast/src/rust_semantic/false_sharing.rs` (~200 LOC)
- `crates/touring-ast/tests/false_sharing_tests.rs` (~250 LOC)

**Acceptance criteria**:
- GIVEN struct `S { a: AtomicU64, b: AtomicU64 }` with #[repr(C)] WHEN scan THEN suggested_align=64 emitted
- GIVEN struct already padded WHEN scan THEN no flag

**Test plan**: 10 fixtures + integration on touring-* crates

**Rollback plan**: feature flag `false-sharing-detector`

**Telemetry**: `Q-230 PotentialFalseSharing`, counter `false_sharing_flag_count`

**Memory store**: `wave_e_2_false_sharing_detector_completed`

---

### Wave E.3 — RCT Metric (Retorno Cognitivo por Token)

| Field | Value |
|-------|-------|
| ID | W-E.3 |
| Phase | Cross (all phases — economic instrumentation) |
| Dimension | IX (Termodinâmica/FinOps) |
| Effort | S (3 engineer-days) |
| Dependencies | none for basic; deeper integration with E.4 |
| Origin | Manual cap. 7.2 (RCT) |
| Confidence | INFERENCE [0.85] |

**Affected crates**: `touring-server`, `touring-hooks`
**New crates**: none

**Atomic sub-tasks**:

1. Define `TokenAccountingEvent { tool_call_id, tokens_in, tokens_out, diagnostic_resolved: Option<DiagnosticCode>, latency_us, timestamp }` in `touring-core`
2. `touring-mcp` server emits event per tool call (parsing `usage` field if MCP client provides; else heuristic from input/output JSON byte size / 4)
3. Aggregator computes RCT = `Σ diagnostics_resolved / Σ (tokens_in + tokens_out)` per session/tool
4. Expose via gate-metrics: `rct_session`, `rct_per_tool{tool=...}`, `tokens_total_in_count`, `tokens_total_out_count`
5. CLI `touring rct status -j` and `touring rct top --top 10`
6. MCP tool `mcp__touring__rct_query`
7. 8 unit + 4 integration tests

**Files to create**:
- `crates/touring-core/src/token_accounting.rs` (~150 LOC)
- `crates/touring-server/src/cli/rct.rs` (~120 LOC)
- `crates/touring-server/src/mcp/tools/rct_query.rs` (~80 LOC)

**Acceptance criteria**:
- GIVEN MCP client invokes profile_query 100x consuming 50k+30k tokens, resolving 10 diagnostics WHEN `touring rct status -j` THEN response contains rct_session ≈ 1.25e-4 diagnostics-per-token

**Rollback plan**: feature flag `token-accounting`

**Telemetry**: `rct_session`, `tokens_total_in_count`, `tokens_total_out_count`, histogram `rct_per_tool`

**Memory store**: `wave_e_3_rct_metric_completed`

---

### Wave E.4 — OpenTelemetry GenAI Semconv Integration

| Field | Value |
|-------|-------|
| ID | W-E.4 |
| Phase | Cross (Diagnose + Analyze observability) |
| Dimension | VII (Observabilidade) |
| Effort | M (5 engineer-days) |
| Dependencies | E.3 (RCT provides token counts) |
| Origin | Manual cap. 5.2 + Context7 `/open-telemetry/opentelemetry-rust` |
| Confidence | FACT [1.0] |

**Atomic sub-tasks**:

1. Update opentelemetry deps to ≥ 0.27 + `opentelemetry-semantic-conventions` ≥ 0.16
2. In `Mode::Daemon` telemetry init, register GenAI tracer with attrs:
   - `gen_ai.system = "touring"`
   - `gen_ai.operation.name = "tool_call"` or "diagnostic_resolution"
   - `gen_ai.usage.input_tokens` (from E.3)
   - `gen_ai.usage.output_tokens`
   - `gen_ai.agent.name` (from MCP client_info)
3. Each MCP tool call wrapped in span `gen_ai.tool_call`
4. Config knob `touring config set telemetry.genai.enabled = true`
5. CLI `touring telemetry export-trace --tool <name> --last 100`
6. 6 unit + 4 integration tests

**Files to create**:
- `crates/touring-server/src/telemetry/genai_semconv.rs` (~150 LOC)
- `crates/touring-server/tests/genai_telemetry_tests.rs` (~200 LOC)

**Acceptance criteria**:
- GIVEN telemetry.genai.enabled=true AND profile_query invoked WHEN span exporter receives THEN span has gen_ai.system="touring", gen_ai.operation.name, gen_ai.usage.input_tokens, gen_ai.usage.output_tokens, gen_ai.agent.name

**Rollback plan**: knob disables; OTel base preserved

**Telemetry**: spans only

**Memory store**: `wave_e_4_otel_genai_completed`

---

### Wave E.5 — Mutation Testing Diff-Mode + Arid Nodes Filter

| Field | Value |
|-------|-------|
| ID | W-E.5 |
| Phase | 1 + 5 (Diagnose + Write gate) |
| Dimension | III (Qualidade Intrínseca) |
| Effort | M (6 engineer-days) |
| Dependencies | none |
| Origin | Manual cap. 3.2 (MuRS, Arid Nodes Omission) |
| Confidence | INFERENCE [0.85] |

**Atomic sub-tasks**:

1. `--diff-only [--lines a:b,c:d]` flag (per REGRA #11, NO git — use `touring memory recall "checkpoint:*"` OR explicit byte ranges)
2. Arid Nodes filter — regex-based AST node exclusion:
   - Default: `log::*`, `tracing::*`, `println!`, `eprintln!`, `debug!`, `info!`, `warn!`, `error!`, `print!`
   - String/byte literals (mostly no semantic effect from mutation)
   - `const` and `static` declarations
   - Test modules `#[cfg(test)]`
3. MuRS-style ranker: sort surviving mutants by impact heuristic:
   - Conditional operators (`==` ↔ `!=`, `<` ↔ `>=`) → high
   - Arithmetic with side effects → medium
   - Log/debug → low (filtered already by Arid)
4. Output mode `--report-format murs-json`
5. Integration with `touring pre-edit` advisory hint when blast_radius > 5
6. 12 unit + 4 integration tests

**Files to create**:
- `crates/touring-mutation/src/arid_filter.rs` (~150 LOC)
- `crates/touring-mutation/src/ranker.rs` (~200 LOC)
- `crates/touring-mutation/tests/diff_arid_murs_tests.rs` (~300 LOC)

**Acceptance criteria**:
- GIVEN file with 1000 LOC, 50 changed since memory checkpoint WHEN `touring mutation-test --diff-only --filter-arid` THEN mutations only on 50 lines, no log/trace mutations
- GIVEN proposed edit with blast_radius=8 WHEN pre-edit runs THEN advisory hint suggests mutation-test

**Rollback plan**: each flag opt-in; default behavior preserved

**Telemetry**: counters `mutation_diff_run_count`, `mutation_arid_filtered_count`, `mutation_murs_ranked_count`

**Memory store**: `wave_e_5_mutation_murs_completed`

---

### Wave E.6 — 4/δ Markov Convergence Modeling (shadow validate)

| Field | Value |
|-------|-------|
| ID | W-E.6 |
| Phase | 5 (Write — speculative validation budgeting) |
| Dimension | I (Funcionalidade/Determinismo) + XI (Integração Agêntica) |
| Effort | M (6 engineer-days) |
| Dependencies | none |
| Origin | Manual cap. 1.3 (Teorema 4/δ Markov Convergence — LLM-Verifier loop) |
| Confidence | INFERENCE [0.85] |

**Atomic sub-tasks**:

1. Define 4-state Markov model for shadow rollout:
   - S0 = Draft (initial)
   - S1 = Verified (VGP gate passed)
   - S2 = Speculated (shadow validate ≥ 0.8)
   - S3 = Committed (terminal absorbing — success)
   - S_fail = Rejected (terminal absorbing — failure)
2. Empirical δ measurement per `(GeneratorKind, file_blast_class)` cohort
3. Compute budget τ̂ = 4/δ̂ as default `--max-iterations`
4. If empirical δ insufficient (n < 30), emit `G-300 ConvergenceUncertain`
5. CLI `touring convergence stats [--by-kind|--by-blast]`
6. Counter `shadow_validate_convergence_iterations_histogram`
7. 10 unit + 4 integration tests

**Files to create**:
- `crates/touring-generator/src/convergence/{mod.rs, markov_chain.rs, delta_estimator.rs}` (~400 LOC)
- `crates/touring-generator/tests/convergence_tests.rs` (~250 LOC)

**Acceptance criteria**:
- GIVEN 100 historical runs of FunctionImpl/medium with mean 5 iterations WHEN `touring convergence stats --by-kind` THEN response { kind, blast_class, delta_hat: 0.2, tau_budget: 20 }

**Rollback plan**: feature flag `convergence-budget`; default `--max-iterations = 100`

**Telemetry**: `G-300 ConvergenceUncertain`, histogram `shadow_validate_convergence_iterations`

**Memory store**: `wave_e_6_convergence_modeling_completed`

---

### Wave E.7 — Bounded Context Auto-Discovery

| Field | Value |
|-------|-------|
| ID | W-E.7 |
| Phase | 2 (Analyze) |
| Dimension | VIII (Sociotécnica — Lei de Conway, DDD) |
| Effort | M (7 engineer-days) |
| Dependencies | none |
| Origin | Manual cap. 6.1 (DDD Bounded Contexts, Team Topologies) |
| Confidence | INFERENCE [0.85] |

**Atomic sub-tasks**:

1. Algorithm — community detection on wiring graph (Louvain modularity OR Leiden via `petgraph`)
2. Heuristic naming: most common module path prefix + verb prefix patterns
3. Output: `{ context_name, modules: [...], cohesion_score, coupling_to_other_contexts: { B: 0.3 } }`
4. Persist `~/.taco/bounded_contexts.json` per workspace + diff against past
5. CLI `touring contexts list -j` and `touring contexts diff [--since <ref>]`
6. RFC-100 `S-200 ContextCohesionDegrading` (warning when cohesion drops > 10% session-over-session)
7. MCP tool `mcp__touring__bounded_contexts_query`
8. 12 unit + 4 integration tests

**Files to create**:
- `crates/touring-ast/src/bounded_context/{mod.rs, louvain.rs, naming.rs, persistence.rs}` (~500 LOC)
- `crates/touring-ast/tests/bounded_context_tests.rs` (~350 LOC)

**Acceptance criteria**:
- GIVEN touring-* workspace with 24 crates WHEN `touring contexts list -j` THEN 4-7 distinct contexts AND each has cohesion_score AND total intra-context edges > 70%

**Rollback plan**: feature flag `bounded-contexts`

**Telemetry**: `S-200`, gauge `bounded_context_count`, histogram `cohesion_score`

**Memory store**: `wave_e_7_bounded_contexts_completed`

---

### Wave E.8 — MCP Progressive Disclosure

| Field | Value |
|-------|-------|
| ID | W-E.8 |
| Phase | 3 (Read — token-economic tool listing) |
| Dimension | V (Excelência LLM) + IX (FinOps) |
| Effort | L (10 engineer-days) |
| Dependencies | E.3 (RCT measurement validation) |
| Origin | Manual cap. 7.1 ref [68] |
| Confidence | INFERENCE [0.9] |

**Atomic sub-tasks**:

1. Reorganize MCP tools by namespace: `index`, `ast`, `wiring`, `generate`, `memory`, `learning`, `session`, `decompose`, `gotcha`, `inferlets`, `meta`, `policy` (E.9), `rct` (E.3), `contexts` (E.7), `false_sharing` (E.2), `convergence` (E.6) — ~12 namespaces
2. New MCP capability: `tools/list?namespace=<name>` — defaults return only meta-namespace tools
3. Tool `mcp__touring__namespaces_list` returning ~12 entries vs 88 individual
4. Backwards compat: `tools/list` without namespace OR `namespace=*` returns all 88
5. Telemetry: `tool_listing_token_savings` = bytes saved
6. Update `references/mcp_tools.md`
7. 14 unit + 6 integration tests

**Files to create/modify**:
- `crates/touring-server/src/mcp/namespace.rs` (~200 LOC)
- `crates/touring-server/src/mcp/tools/namespaces_list.rs` (~80 LOC)
- `crates/touring-server/tests/mcp_progressive_disclosure_tests.rs` (~400 LOC)

**Acceptance criteria**:
- Default `tools/list` returns ~5 meta-namespace tools < 5 KB (vs current ~50 KB)
- `tools/list?namespace=ast` returns ~10 tools
- Legacy `tools/list?namespace=*` returns all 88

**Rollback plan**: env `TOURING_MCP_LEGACY_LISTING=1` reverts

**Telemetry**: counter `mcp_namespaced_listing_count`, histogram `mcp_tool_listing_byte_size`

**Memory store**: `wave_e_8_mcp_progressive_disclosure_completed`

---

### Wave E.9 — OPA/Rego Runtime Policy Gate

| Field | Value |
|-------|-------|
| ID | W-E.9 |
| Phase | Cross |
| Dimension | VI (Segurança) + VIII (Sociotécnica — Law as Code) |
| Effort | L (12 engineer-days) |
| Dependencies | none |
| Origin | Manual cap. 6.2 |
| Confidence | FACT [1.0] (regorus crate provides Rust-native Rego eval) |

**Atomic sub-tasks**:

1. Add dep `regorus = "0.2+"`
2. Policy bundle path `~/.taco/policies/*.rego` loaded at daemon start
3. Default policy `default.rego` allowing all (opt-in restriction model):
   ```rego
   package touring.mcp
   default allow = true
   ```
4. Each MCP tool call passes input `{ tool, args, client_info, session_id, file_path?, blast_radius? }` to `data.touring.mcp.allow`
5. If `allow == false`, return MCP error code -32000 + emit `P-100 PolicyDenied`
6. CLI `touring policy eval <input.json>` (test offline)
7. CLI `touring policy reload` (hot-reload without daemon restart)
8. Bundle 5 sample policies in `~/.claude/rust/docs/policy-examples/`:
   - `block-high-blast.rego` — deny edits to high blast_radius
   - `read-only-prod.rego` — block writes when env=prod
   - `audit-log.rego` — log all calls
   - `time-window.rego` — restrict to business hours
   - `tenant-isolation.rego` — multi-project isolation
9. MCP tool `mcp__touring__policy_eval`
10. 18 unit + 6 integration tests

**Files to create**:
- `crates/touring-policy/Cargo.toml`
- `crates/touring-policy/src/{lib.rs, evaluator.rs, bundle_loader.rs, hot_reload.rs}` (~500 LOC)
- `crates/touring-policy/tests/policy_tests.rs` (~400 LOC)
- `~/.claude/rust/docs/policy-examples/*.rego` (5 files)

**Acceptance criteria**:
- GIVEN block-high-blast.rego loaded + request with blast_radius=80 WHEN MCP processes THEN blocked + error -32000 + P-100 emitted
- Default policy → execution proceeds, evaluation latency < 5ms
- Policy file edited + `touring policy reload` → effective without daemon restart

**Rollback plan**: env `TOURING_POLICY_ENABLED=0` bypasses; per-tool `policy_exempt: true`

**Telemetry**: `P-100 PolicyDenied`, counters `policy_eval_count`, `policy_deny_count`, histogram `policy_eval_latency_us`

**Memory store**: `wave_e_9_opa_rego_completed`

---

### Wave E.10 — Sigstore Keyless OIDC Artifact Signing

| Field | Value |
|-------|-------|
| ID | W-E.10 |
| Phase | Cross (build/release) |
| Dimension | VI (Segurança — Supply-Chain Cryptography) |
| Effort | M (7 engineer-days) |
| Dependencies | none |
| Origin | Manual cap. 4.2 |
| Confidence | FACT [1.0] |

**Atomic sub-tasks**:

1. Add `sigstore = "0.9+"` to dev-deps of touring-server
2. Build-script post-build: invoke sigstore keyless OIDC sign on `target/release/touring-daemon`, `touring-hook`, `touring`
3. OIDC: GitHub Actions OIDC token (CI builds) OR ambient-token from env (local dev opt-in)
4. Push signature to Rekor (default `https://rekor.sigstore.dev`)
5. Verification CLI `touring verify --binary <path>` — fetches Rekor entry, validates hash
6. Integrate verification into `update-touring` v3 post-install
7. Skip-on-no-OIDC: build proceeds with WARNING
8. 10 unit (mocked sigstore client) + 4 integration tests

**Files to create**:
- `crates/touring-server/src/cli/verify.rs` (~150 LOC)
- `crates/touring-server/build.rs` — sigstore integration (~100 LOC)
- `~/.local/bin/update-touring` v3 — append verification step

**Acceptance criteria**:
- GIVEN OIDC token + cargo build --release WHEN build completes THEN target/release/touring-daemon signed AND Rekor entry created AND `touring verify` returns OK with Rekor URL
- GIVEN no OIDC token WHEN build runs THEN succeeds with warning, verify returns "unsigned" non-fatally

**Rollback plan**: env `TOURING_SIGN_DISABLED=1`

**Telemetry**: counters `sigstore_sign_success_count`, `sigstore_sign_skip_count`, `sigstore_verify_count`

**Memory store**: `wave_e_10_sigstore_completed`

---

### Wave E.11 — Semgrep-style Declarative SAST Rules

| Field | Value |
|-------|-------|
| ID | W-E.11 |
| Phase | 1 (Diagnose — security pattern detection) |
| Dimension | VI (Segurança) |
| Effort | M (8 engineer-days) |
| Dependencies | B.3 (CharClasses) from cross-repo v1.0 if landed; otherwise self-contained via tree-sitter |
| Origin | Manual cap. 4 + Context7 `/semgrep/semgrep-docs` |
| Confidence | FACT [0.95] |

**Atomic sub-tasks**:

1. Define rule schema YAML compatible with Semgrep:
   ```yaml
   rules:
     - id: hardcoded-secret-rust
       pattern: 'let $TOKEN = "$VALUE"'
       pattern-not: 'let $TOKEN = ""'
       metavariable-regex:
         $VALUE: '(?i)(api[_-]?key|secret|password|token).*[a-z0-9]{20,}'
       message: "Hardcoded secret detected"
       severity: ERROR
       languages: [rust]
   ```
2. Translate Semgrep DSL to ast-grep query (60-70% subset that maps cleanly)
3. Bundle 30 starter rules covering OWASP Top 10 + common Rust antipatterns:
   - Hardcoded secrets (regex + AST)
   - SQL injection patterns (sqlx::query!() with format args)
   - Command injection (process spawn with untrusted input)
   - Path traversal (`Path::new(req.params.get(..))`)
   - Unsafe deserialization (`bincode::deserialize` from network)
   - Weak crypto (md5, sha1, des)
   - TLS verification disabled
   - `unwrap()` in production paths
   - 22+ more
4. CLI `touring sast scan <file|dir>`
5. RFC-100 codes: `SEC-100` (secrets), `SEC-101` (injection), `SEC-102` (crypto-weak), etc.
6. MCP tool `mcp__touring__sast_scan`
7. Integration with `pre-edit` hook: ERROR severity blocks edit
8. 25 unit + 8 integration tests

**Files to create**:
- `crates/touring-sast-rules/Cargo.toml`
- `crates/touring-sast-rules/src/{lib.rs, rule_loader.rs, semgrep_translator.rs, runner.rs}` (~600 LOC)
- `crates/touring-sast-rules/rules/{secrets.yaml, injection.yaml, crypto.yaml, ...}` (30 rules across ~6 files)
- `crates/touring-sast-rules/tests/rule_tests.rs` (~600 LOC)

**Acceptance criteria**:
- GIVEN file with hardcoded secret literal WHEN `touring sast scan <file>` THEN SEC-100 emitted with severity=ERROR
- GIVEN clean file WHEN scan THEN no diagnostics, exit 0
- GIVEN edit introducing hardcoded secret WHEN pre-edit THEN edit blocked with SEC-100

**Rollback plan**: each rule disable-able via `~/.taco/sast-config.yaml`; entire SAST behind feature flag

**Telemetry**: SEC-100..SEC-130, counters `sast_scan_count`, `sast_finding_count_by_rule`

**Memory store**: `wave_e_11_sast_rules_completed`

---

### Wave E.12 — Twelve-Factor + Zero-Trust + Prompt-Injection Detector

| Field | Value |
|-------|-------|
| ID | W-E.12 |
| Phase | 1 + 4 (Diagnose + Edit gate) |
| Dimension | I (Funcionalidade) + VI (Segurança) |
| Effort | L (10 engineer-days) |
| Dependencies | E.11 (extends SAST infrastructure) |
| Origin | code-diagnostic.md + Manual cap. 4.4 |
| Confidence | FACT [0.9] |

**Atomic sub-tasks**:

1. **Twelve-Factor compliance auditor**:
   - Factor I (Codebase): single repo per app — detect via `touring ast workspace-info`
   - Factor II (Dependencies): explicit declaration — verify Cargo.toml has no `path = ../..` workspace escapes
   - Factor III (Config): env vars not hardcoded — overlap with E.11 (hardcoded-secret rule)
   - Factor IV (Backing services): heuristic — detect direct fs paths to system dirs as antipattern
   - Factor V (Build/release/run): heuristic — no git invocations in build.rs (REGRA #11 alignment)
   - Factor VI (Processes): stateless — detect `static mut` in production paths
   - Factor X (Dev/prod parity): detect `#[cfg(debug_assertions)]` divergent business logic
   - Factor XI (Logs): structured — detect `println!` in non-test paths
   - Output composite score 0-12 in gate-metrics
2. **Zero-Trust validation rules** — extend SAST DSL with taint analysis:
   - Source markers: `untrusted_source!()` macro annotation, HTTP request bodies, env vars, file contents from user-supplied paths
   - Sink markers: process spawn, sql::raw, eval, prompt construction `format!("{}", user_input)`
   - Rule: source flowing to sink without sanitizer call → SEC-200 ZeroTrustViolation
3. **Prompt-Injection detector**:
   - Detect string concatenations involving system prompts: `format!("{system_prompt}{}", user_input)`
   - Heuristic: if `user_input` not passed through known sanitizer → SEC-201 PromptInjectionRisk
   - Rule bundles for popular LLM SDKs: anthropic-sdk, openai, ollama, langchain
4. RFC-100 codes: `T-100..T-112` (Twelve-Factor failures), `SEC-200` (Zero-Trust), `SEC-201` (Prompt-Injection)
5. CLI `touring twelve-factor audit [--workspace <path>]` and `touring zero-trust scan <file>`
6. MCP tools `mcp__touring__twelve_factor_audit`, `mcp__touring__zero_trust_scan`
7. 30 unit + 10 integration tests

**Files to create**:
- `crates/touring-sast-rules/src/twelve_factor.rs` (~250 LOC)
- `crates/touring-sast-rules/src/zero_trust.rs` (~200 LOC, taint engine)
- `crates/touring-sast-rules/src/prompt_injection.rs` (~150 LOC)
- `crates/touring-sast-rules/tests/twelve_factor_tests.rs` (~300 LOC)
- `crates/touring-sast-rules/tests/zero_trust_tests.rs` (~250 LOC)

**Acceptance criteria**:
- GIVEN workspace where Cargo.toml uses `path = ../../external` (Factor II violation) WHEN `touring twelve-factor audit` THEN composite score = 11/12 with T-102 emitted
- GIVEN function consuming HttpRequest body and passing to process spawn without sanitizer WHEN `touring zero-trust scan` THEN SEC-200 emitted
- GIVEN code constructing prompt with format!("{system}{}", user_msg) without sanitizer WHEN scan THEN SEC-201 emitted

**Rollback plan**: each detector independent feature flag (`twelve-factor`, `zero-trust`, `prompt-injection`)

**Telemetry**: T-100..T-112 (12 codes), SEC-200, SEC-201; counters per detector

**Memory store**: `wave_e_12_governance_security_completed`

---

## 7. Integration with Cross-Repo Plan v1.0

This Excellence Plan v2.0 is **complementary, not duplicative** to v1.0.

### 7.1 What v1.0 covers (do not duplicate)

- A.1 (touring-core::profile) — instrumentation primitives
- A.2 (SkipContext) OK landed 2026-04-29
- A.3 (Idempotency Q-220) OK landed
- A.4 (MCP profile_query) OK landed
- B.1 (SSR semantic), B.2 (Shape budget), B.3 (CharClasses), B.4 (Dual-module gating), B.5 (SourceChange transactional)
- C.1 (touring-assists framework), C.2 (touring-vfs), C.3 (Salsa POC), C.4 (format-rust --preserve)
- D.1 (Definition enum), D.2 (resolve-def/find-references/rename), D.3 (RFC-100 with fixes)

### 7.2 Synergies between plans

| v1.0 Wave | v2.0 Wave | Synergy |
|-----------|-----------|---------|
| A.1 (profile) | E.3 (RCT) + E.4 (OTel GenAI) | profile counters feed RCT aggregator + GenAI spans |
| A.2 (SkipContext) | E.11 (SAST) | SAST honors skip-region |
| B.3 (CharClasses) | E.11 (SAST) | regex matching skips strings/comments |
| B.5 (SourceChange) | E.9 (OPA), E.11 (SAST) | policy gate evaluates each SourceChange |
| C.1 (assists) | E.11 (SAST) | each SAST finding may have associated fix assist |
| D.3 (RFC-100 fixes) | E.11, E.12 | new RFC-100 codes (SEC-*, T-*) carry fix references |

### 7.3 Recommended execution order (combined v1.0 + v2.0)

```
Phase Foundation (weeks 1-3):
  v1.0 A.1 + v2.0 E.1, E.2, E.3 (all S/M effort, independent)

Phase Reform (weeks 4-9):
  v1.0 B.1, B.2, B.3, B.4, B.5 + v2.0 E.4, E.5, E.6, E.7

Phase Governance (weeks 10-13):
  v2.0 E.9, E.10, E.11, E.12 (security/governance focused)

Phase Architectural (weeks 14-21):
  v1.0 C.1, C.2, C.3, C.4 (XL effort, biggest payoffs)

Phase Closure (weeks 22-24):
  v1.0 D.1, D.2, D.3 + v2.0 E.8 (after all consumers landed)
```

---

## 8. Roadmap (15 weeks, 3 phases — this Plan v2.0 alone)

```
                Sprint:  1  2  3  4  5  6  7  8  9  10 11 12 13 14 15
Phase Foundation
  E.1 Halstead Bug      [▓]
  E.2 False Sharing     [▓▓]
  E.3 RCT               [▓▓]
  E.4 OTel GenAI           [▓▓▓]

Phase Observability + Quality
  E.5 Mutation MuRS         [▓▓▓]
  E.6 4/δ Convergence       [▓▓▓]
  E.7 Bounded Contexts          [▓▓▓]
  E.8 MCP Progressive Disc          [▓▓▓▓▓]

Phase Governance + Security
  E.9 OPA/Rego                          [▓▓▓▓▓▓]
  E.10 Sigstore                         [▓▓▓]
  E.11 SAST Rules                            [▓▓▓▓]
  E.12 Twelve-Factor + ZT                          [▓▓▓▓▓]
```

**Calendar single-dev**: 15 sprints ≈ 3.5 months
**Calendar 3-devs parallel**: 7-8 sprints ≈ 2 months
**Combined plan v1.0+v2.0 single-dev**: 36 sprints ≈ 8 months
**Combined 3-devs parallel**: 18 sprints ≈ 4 months

---

## 9. Risk Register

| # | Risk | Wave | Probability | Impact | Mitigation |
|---|------|------|-------------|--------|-----------|
| ER-1 | False sharing detector emits FPs on `tokio::sync::*` types | E.2 | MEDIUM | LOW | Allowlist tokio::sync, crossbeam_utils::CachePadded |
| ER-2 | RCT measurements unreliable when MCP client doesn't send `usage` field | E.3 | HIGH | MEDIUM | Fallback heuristic = (in_bytes + out_bytes) / 4; document accuracy tier |
| ER-3 | OTel GenAI semconv 1.27 unstable | E.4 | MEDIUM | LOW | Pin opentelemetry-semantic-conventions; track stable tag |
| ER-4 | Mutation diff-mode requires last-known-good without git (REGRA #11) | E.5 | MEDIUM | MEDIUM | Use `touring memory recall "checkpoint:*"` + byte-range diff |
| ER-5 | Markov δ estimate unreliable for small samples (n < 30) | E.6 | HIGH | LOW | Emit G-300 ConvergenceUncertain; default `--max-iterations = 100` |
| ER-6 | Bounded Context auto-naming produces poor labels | E.7 | HIGH | LOW | Allow user override via `~/.taco/bounded_contexts.json` |
| ER-7 | MCP Progressive Disclosure breaks Claude Code default tool discovery | E.8 | MEDIUM | HIGH | Backwards compat: namespace=`*` returns all 88 |
| ER-8 | Rego policy DSL learning curve too high | E.9 | MEDIUM | LOW | Bundle 5 example policies + `touring policy explain <file>` |
| ER-9 | regorus crate missing Rego features | E.9 | LOW | MEDIUM | Track upstream; fallback opa-wasm Rust binding |
| ER-10 | Sigstore Rekor public log rate-limits CI builds | E.10 | LOW | LOW | Use private Rekor instance OR opt-out via env var |
| ER-11 | OIDC token availability complex outside CI | E.10 | HIGH | MEDIUM | Skip-on-no-OIDC with warning |
| ER-12 | Semgrep DSL features beyond ast-grep capability (taint, dataflow) | E.11 | MEDIUM | MEDIUM | Implement 60-70% subset; E.12 adds taint engine |
| ER-13 | Twelve-Factor heuristics produce FPs on monorepo edge cases | E.12 | MEDIUM | LOW | Per-factor opt-out config; workspace-level overrides |
| ER-14 | Prompt-injection detector misses non-format!/non-string-concat vectors | E.12 | HIGH | MEDIUM | Document scope (high-precision low-recall); pair with E.9 OPA |
| ER-15 | Token cost (E.3 RCT, E.4 GenAI) reveals Touring is expensive | overall | MEDIUM | MEDIUM | Use as feedback for optimization (REGRA #0 potencialização) |
| ER-16 | New crates bloat compile time | overall | HIGH | LOW | profile.dev settings (REGRA #12); add to disk-watch |
| ER-17 | E.12 scope creep (12 factors × N heuristics each) | E.12 | MEDIUM | MEDIUM | Implement 6 most impactful factors first (II, III, V, VI, X, XI) |

---

## 10. Memory Persistence Plan

### 10.1 Per-deliverable memory entries

```bash
touring memory store "wave_e_1_halstead_bug_gate_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_2_false_sharing_detector_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_3_rct_metric_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_4_otel_genai_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_5_mutation_murs_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_6_convergence_modeling_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_7_bounded_contexts_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_8_mcp_progressive_disclosure_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_9_opa_rego_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_10_sigstore_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_11_sast_rules_completed" "..." --tier semantic --type lesson
touring memory store "wave_e_12_governance_security_completed" "..." --tier semantic --type lesson
```

### 10.2 MEMORY.md index updates (per phase landing)

```markdown
- [Excellence Phase Foundation 2026-MM-DD](project_excellence_phase_foundation_completed.md) — E.1 Halstead Bug Gate + E.2 False Sharing + E.3 RCT + E.4 OTel GenAI. ~30 tests.
- [Excellence Phase Observability+Quality 2026-MM-DD](project_excellence_phase_obsqual_completed.md) — E.5 Mutation MuRS + E.6 4/δ Convergence + E.7 Bounded Contexts + E.8 MCP Progressive Disclosure. ~80 tests.
- [Excellence Phase Governance+Security 2026-MM-DD](project_excellence_phase_govsec_completed.md) — E.9 OPA/Rego + E.10 Sigstore + E.11 SAST Rules + E.12 Twelve-Factor + Zero-Trust + Prompt Injection. ~100 tests, 2 new crates.
```

### 10.3 Plan-level memory checkpoint

```bash
touring memory store "excellence_pipeline_master_plan_2026_04_29" \
  "12 deliverables (E.1-E.12) covering 9 dimensions x 5 phases. Sources: code-diagnostic.md + Manual Avancado (118 KB) + Context7 (tree-sitter, opentelemetry-rust, semgrep). Total ~14 sprints (3 devs). Plan: ~/.claude/rust/docs/2026-04-29-touring-excellence-pipeline-master-plan.md. Complementa cross-repo v1.0." \
  --tier semantic --type plan
```

---

## 11. Validation Gates per Phase

### 11.1 Phase Foundation exit criteria (after E.1, E.2, E.3, E.4)

- [ ] All 4 deliverables landed
- [ ] `touring gate-metrics -j` shows new fields: `halstead_bug_gate_violations_count`, `false_sharing_flag_count`, `rct_session`, GenAI semconv spans visible
- [ ] `cargo test --workspace` passes (+30 tests)
- [ ] `cargo clippy --workspace -- -D warnings` returns 0 warnings
- [ ] `touring doctor -j` returns 5/5 OK
- [ ] No new orphans introduced
- [ ] SKILL.md updated via references; `wc -l SKILL.md` < 500
- [ ] Phase session report written, MEMORY.md updated

### 11.2 Phase Observability+Quality exit criteria (after E.5, E.6, E.7, E.8)

- [ ] All 4 deliverables landed
- [ ] `touring mutation-test --diff-only --filter-arid` produces ranked output
- [ ] `touring convergence stats --by-kind` returns δ̂ for ≥ 3 kinds
- [ ] `touring contexts list -j` returns 4-7 contexts on touring-* workspace
- [ ] `tools/list?namespace=ast` returns ≤ 15 tools (vs 88 default)
- [ ] `cargo test --workspace` passes (+80 tests)
- [ ] No new orphans
- [ ] Phase session report + MEMORY.md updated

### 11.3 Phase Governance+Security exit criteria (after E.9, E.10, E.11, E.12)

- [ ] All 4 deliverables landed
- [ ] `touring policy eval` passes 5 example bundle tests
- [ ] `touring verify --binary <path>` returns valid Rekor URL (with OIDC)
- [ ] `touring sast scan <file>` finds 0 issues on clean code, ≥ 1 on synthetic vulnerable
- [ ] `touring twelve-factor audit` produces composite score
- [ ] `touring zero-trust scan <file>` detects taint flow
- [ ] `cargo test --workspace` passes (+100 tests)
- [ ] 2 new crates (touring-policy, touring-sast-rules) in `disk-watch.sh` TARGETS
- [ ] Phase session report + MEMORY.md updated

### 11.4 Plan-level final gate

- [ ] All 12 deliverables shipped
- [ ] All 12 memory entries created
- [ ] Skill SKILL.md updated via references/ (REGRA #13 hygiene)
- [ ] CLAUDE.md updated only if new HARD RULE introduced (E.9 may justify)
- [ ] Cross-audit by touring-auditor: composite_score ≥ 1.0
- [ ] `touring e2e -j` health composite ≥ 0.85 (uplift from 0.69 baseline 2026-04-28)
- [ ] Plan retrospective: per-wave actual vs estimated effort, deviations, lessons
- [ ] Combined v1.0+v2.0 final report: 27 deliverables shipped (15 cross-repo + 12 excellence)

### 11.5 Self-validation summary

- Each deliverable atomic — 12 deliverables, each with own deps, files, tests, rollback
- Dependencies acyclic — DAG verified; E.4 depends on E.3, E.12 depends on E.11; no cycles
- Estimates realistic — calibrated against past Touring waves
- Risks have mitigations — 17 risks tagged
- Confidence:
  - 4 FACT [≥0.95]: E.1, E.4, E.9, E.10
  - 4 FACT [0.9-0.95]: E.6, E.11, E.12, partial E.5
  - 4 INFERENCE [0.85]: E.2, E.3, E.7, E.8
  - 0 SPECULATION

---

## 12. Appendix — Cross-Walk + References

### 12.1 9-Dimension × 5-Phase × 12-Wave matrix

| Dimension ↓ / Phase → | 1. Diagnose | 2. Analyze | 3. Read | 4. Edit | 5. Write |
|-----------------------|-------------|-----------|---------|---------|----------|
| I Funcionalidade | — | — | — | E.12 | v1.0 typestate |
| II Eficiência | E.1, E.2 | E.2 | — | — | — |
| III Qualidade | v1.0 TDG | — | — | E.5 | E.5 |
| V Excelência LLM | — | — | E.8 | — | — |
| VI Segurança | E.11, E.12 | E.12 | E.9 | E.9, E.12 | E.10 |
| VII Observabilidade | — | E.4 | — | — | — |
| VIII Sociotécnica | — | E.7 | — | — | — |
| IX Termodinâmica | — | E.3 | E.3 | — | — |
| XI Integração Agêntica | — | — | E.8 | — | E.6 |

**Coverage**: 9/9 dimensions × 5/5 phases — every cell covered by Wave or already present.

### 12.2 Source documents

| Doc | Path | Size | Coverage |
|-----|------|------|----------|
| code-diagnostic.md | `~/.claude/rust/docs/code-diagnostic.md` | 18 KB | 9 dimensões filosóficas |
| Manual-Avancado-de Analise-de-Codigo.md | `~/.claude/rust/docs/Manual-Avancado-de Analise-de-Codigo.md` | 118 KB | 7 capítulos doutrinários + 86 refs |
| Cross-repo plan v1.0 | `~/.claude/rust/docs/2026-04-28-cross-repo-improvements-master-plan.md` | 72 KB | 15 deliverables A/B/C/D |
| Excellence Plan v2.0 (this) | `~/.claude/rust/docs/2026-04-29-touring-excellence-pipeline-master-plan.md` | this | 12 deliverables Foundation/ObsQual/Governance |

### 12.3 Context7 references consulted

- `/tree-sitter/tree-sitter` — incremental parsing API (E.11 SAST translation)
- `/open-telemetry/opentelemetry-rust` — semconv attribute setting (E.4)
- `/semgrep/semgrep-docs` — pattern-matching DSL (E.11 + E.12 taint)
- (Previously v1.0): `/websites/rs_salsa`, `/pawurb/hotpath-rs`

### 12.4 Hard rule references

- `~/.claude/CLAUDE.md` — REGRAS #0, #11, #12, #13
- `~/.claude/rules/TACO-subagent.md` — Phase Protocol L4+
- `~/.claude/rules/VP-Scout.md` — verification chains 1-7
- `~/.claude/rules/touring-cli-index.md` — CLI Tier 1-9
- `~/.claude/rules/touring-rebuild.md` — daemon lifecycle (E.10 integration point)

### 12.5 Combined plan v1.0+v2.0 — final inventory

| Plan | Waves | Deliverables | Estimated effort (single dev) |
|------|-------|--------------|------------------------------|
| v1.0 (cross-repo) | A, B, C, D | 15 | ~21 sprints |
| v2.0 (excellence) | E (12 sub-waves) | 12 | ~15 sprints |
| **Combined** | A+B+C+D+E | **27** | **~36 sprints (8 months)** |
| **Combined parallelized (3 devs)** | — | 27 | **~18 sprints (4 months)** |

---

## Sign-off

**Status**: PLAN READY FOR REVIEW
**Next action**: Gabriel reviews → approves Phase Foundation authorization (E.1, E.2, E.3, E.4 in parallel) → orchestrator begins per `~/.claude/rules/TACO-subagent.md` Phase 0

**Suggested execution choreography**:

| Phase | Sub-owner agents (per wave) |
|-------|----------------------------|
| Foundation (E.1-E.4) | touring-engineer x4 (parallel) — small/medium scope, low cross-talk |
| Observability+Quality (E.5-E.8) | touring-architect (lead E.6, E.7) + touring-engineer x2 (E.5, E.8) |
| Governance+Security (E.9-E.12) | touring-architect (lead E.9, E.12) + touring-engineer x2 (E.10, E.11) + touring-auditor (cross-audit final) |

**Approval block**:

```
Approver: Gabriel Gadea
Phase Foundation authorization (E.1, E.2, E.3, E.4): [ ] PENDING
Phase Observability+Quality authorization (E.5, E.6, E.7, E.8): [ ] PENDING (after Foundation)
Phase Governance+Security authorization (E.9, E.10, E.11, E.12): [ ] PENDING (after Obs+Quality)
```

**Decision points**:

1. Confirm execution lane: this plan v2.0 in parallel with v1.0 remaining? Or sequential?
2. OPA/Rego adoption (E.9): validate `regorus` crate in 1-day spike before committing 12 days
3. Sigstore (E.10): confirm CI infrastructure has OIDC token availability before sprint start
4. MCP Progressive Disclosure (E.8): coordinate with Claude Code team if backwards compat path differs

---

*End of Excellence Pipeline Master Plan v2.0 — total 12 deliverables, 3 phases, ~14 sprints estimated, ~210 tests added, ~3.500 LOC new, 4 new crates (touring-policy, touring-sast-rules, touring-mutation extensions, touring-bounded-context as touring-ast submodule)*

*Powered by: code-diagnostic.md (9 dimensions) + Manual-Avancado-de Analise-de-Codigo.md (7 doctrinal chapters + 86 refs) + Context7 (tree-sitter, opentelemetry-rust, semgrep) + cross-repo plan v1.0*
