---
name: taco-planning
description: Create exponentially excellent implementation plans using Touring CLI intelligence + a deterministic Python toolkit. Toolkit — ground_truth_collector (Stage-1 unified Touring sweep), dimension_scorer (9 dims, schema/symbols/blast-aware), dimension_amplifier (concrete actions to lift each dim <7), gap_detector (undefined symbols, vague claims, missing evidence/blast), confidence_tagger (auto FACT/INFERENCE/SPECULATION on every claim), plan_validator (4-stage Pln2 structure + tag coverage), plan_scaffolder (Jinja2 Pln2 skeleton from intent + ground truth), dag_builder (Mermaid + parallel/sequential phase extraction), mcts_wrapper (multi-path planning via touring mcts search). Enforces 9 quality dimensions (precision, scalability, performance, functionality, quality, detail, integration, dependencies, potentiation) with mandatory VGP-verified ground truth. Use when — creating plans, writing blueprints, designing architectures, planning implementations, when keywords plan / plano / blueprint / roadmap / Pln1 / Pln2 / Pln^N appear. Produces Pln2-grade plans grounded in real codebase state. Complements TACO-wt — taco-planning AUTHORS plans; TACO-wt OPERATES them.
---

# taco-planning — Touring-Grounded Plan Excellence

> **Origin**: Wave 2026-05-01 (agentic paradigm shift) created the prose-only
> skill; Wave 2026-05-23 added the 10-script toolkit, distilled from
> `pln2_generator` (regex-only dimension scoring, P0-P3 gaps, blake2b),
> `vgp` (Kahn sort, cycle detection, learning JSONL, schema cache),
> `aco` (7-phase orchestration, pre/post hooks, MCTS).
>
> **Pairs with**: `TACO-wt` (wave operation) — taco-planning **authors** the
> plan, TACO-wt **executes** it.

---

## Quick-start in 4 commands

```bash
# 1. Collect ground truth (Stage 1) — unified Touring sweep
python3 ground_truth_collector.py --intent "implement async write-back cache" \
                                  --output data/ground_truth.json

# 2. Scaffold a Pln2 skeleton from intent + ground truth (Jinja2 templates)
python3 plan_scaffolder.py --intent "implement async write-back cache" \
                          --ground-truth data/ground_truth.json \
                          --out plans/2026-05-24-async-cache.md

# 3. Score across 9 dimensions; amplify anything <7
python3 dimension_scorer.py plans/2026-05-24-async-cache.md --emit
python3 dimension_amplifier.py plans/2026-05-24-async-cache.md \
                              --threshold 7 --emit

# 4. Final gates — gap detector + plan validator + confidence tagger
python3 gap_detector.py plans/2026-05-24-async-cache.md --fail-on=P0
python3 plan_validator.py plans/2026-05-24-async-cache.md --strict -j
python3 confidence_tagger.py plans/2026-05-24-async-cache.md --autofill
```

Every script emits JSON to stdout and writes a side-car artifact to `data/`.

---

## The 4 mandatory stages

Each plan passes through 4 stages. Skip none.

### Stage 1 — GROUND TRUTH (before writing a single plan line)

Run a unified Touring sweep — never plan from assumptions. Implemented by
`ground_truth_collector.py`, which parallelizes:

```bash
touring doctor -j                       # System health
touring status -j                       # Unified dashboard
touring e2e --depth standard -j         # Baseline composite score
touring wiring audit -j                 # Orphans + module health
touring evolution drift -j              # Degrading metrics
touring memory recall "<task_keywords>" # Past lessons
touring gotcha match <target_files>     # Known pitfalls
touring index find <each_target_symbol> # VGP — symbol exists?
```

The collector emits a single `ground_truth.json` envelope — every later script
reads from it. Tag every plan claim with confidence (see `confidence_tagger.py`):

- **FACT [1.0]** — verified via a Touring command (the JSON evidence is embedded)
- **INFERENCE [0.7-0.9]** — derived from Touring data (which command? what step?)
- **SPECULATION [<0.7]** — hypothesis needing verification (what would prove it?)

### Stage 2 — 9-DIMENSION ANALYSIS

Score the draft plan on each of the 9 canonical dimensions. Target: all ≥ 8.

| # | Dimension | What `dimension_scorer.py` measures | Min | Amplifier strategy |
|---|-----------|--------------------------------------|----:|---------------------|
| **a** | **Precision** | exact `file:LINE`, verified symbol signatures | 8 | run `touring ast find` for every cited symbol; embed signature |
| **b** | **Scalability** | extensible patterns vs one-offs | 7 | reference factory/trait/registry instead of bespoke struct |
| **c** | **Performance** | benchmarks + complexity (P50/P99/Big-O) | 7 | add target latency + worst-case complexity for hot paths |
| **d** | **Functionality** | maximize capabilities exposed (orphans wired) | 8 | inspect `touring wiring orphans -j` for symbols to integrate |
| **e** | **Quality** | error handling, tests named, 0 unwrap | 8 | every code change has a named test + an error branch |
| **f** | **Detail** | JSON schemas + edge cases + exact code | 8 | every contract has input + output schemas; edges enumerated |
| **g** | **Integration** | cross-module wiring + MCP-CLI map | 8 | run `touring wiring audit` + map each subtask to existing wiring |
| **h** | **Dependencies** | versions pinned, feature flags verified | 7 | read `Cargo.toml`/`pyproject.toml`; document required features |
| **i** | **Potentiation** | each change unlocks future value (REGRA #0) | 7 | every subtask has a "enables" column — if empty, rewrite |

Detail in [references/dimensions-rubric.md](references/dimensions-rubric.md).
Amplifier playbook in
[references/amplification-strategies.md](references/amplification-strategies.md).

### Stage 3 — PLAN STRUCTURE

Write to `~/.claude/plans/<session-slug>.md` using the canonical skeleton —
generated by `plan_scaffolder.py` from `assets/templates/plan_pln2.md.j2`:

```markdown
# <Title> (Pln2)

> Level: L1-L5 | Registry: current → target | Scope: N items

## 1. Ground Truth Summary
- e2e: X.XX | wiring orphans: N | index coverage: N%
- Symbols verified: [list with file:line + signature]
- Past lessons applied: [memory keys]

## 2. 9-Dimension Scores (current → target → delta)
| Dim | Current | Target | Delta | Amplification |
| ... |

## 3. Phases (P1 ... Pn, parallel|sequential)
### S-N: <Action> [SEVERITY] [P0|P1|P2|P3] [confidence: FACT|INFERENCE|SPECULATION]
- **File**: `path/to/file.rs:LINE`
- **Source truth**: <exact current code from touring command>
- **Change**: <inline code diff>
- **Blast radius**: N direct dependents (from `touring ast blast`)
- **Test**: <test name + assertion>
- **Dimensions**: [a:9, e:8, g:7]
- **Enables**: <future work unlocked, REGRA #0>

## 4. DAG (Mermaid + textual)
[built by dag_builder.py]

## 5. Verification Protocol
cargo / pytest / touring e2e targets

## 6. Potentiation Matrix
| Change | Enables |
```

### Stage 4 — AMPLIFICATION CHECK (Pln1 → Pln2)

Self-audit before delivery — implemented as `dimension_amplifier.py`:

1. Every claim verifiable by a Touring command? → embed evidence; else flag confidence.
2. Every subtask maximizes scope (REGRA #0)? → if it reduces, rewrite to integrate.
3. Every change unlocks future work? → if dead-end, add potentiation note.
4. Blast radius documented on every modified file? → `touring ast blast` everywhere.

If any dimension scores < 7, `dimension_amplifier.py` emits the specific
amplification action and re-scoring guidance. Re-run Stage 2 after amplification.

---

## Elite 50-dimension alignment (authoring → delivery)

The 9 authoring dimensions above govern the **plan**; the 50-dimension elite
harness (`touring-quality`, real standalone binary) governs the **delivered
code** the plan produces. A Pln2 plan MUST encode the 50-dim acceptance gate in
its **§5 Verification Protocol** so the executor (TACO-wt) inherits a measurable
bar, not prose. Keystone: `~/.claude/rules/elite-50-quality.md`.

| 9 authoring dim | maps to 50-dim elite cluster |
|---|---|
| **a** Precision | F1.1 complexity · F1.2 maintainability |
| **b** Scalability | F2.13 scalability · F4.8 deploy |
| **c** Performance | F2.7–F2.12 (db/mem/cache/io/concurrency/frontend) |
| **d** Functionality | F1.7 boundaries · F1.9 API design |
| **e** Quality | F1.6 error-handling · F3.1–F3.7 testing · **6 BLOCK P0** |
| **f** Detail | F3.8–F3.13 docs · F3.4 edge cases |
| **g** Integration | F1.8 dep-cycles · F1.12 arch-consistency |
| **h** Dependencies | F2.5 dep-CVEs⛔ · F4.5 pkg-mgmt⛔ · F4.3 deprecated⛔ |
| **i** Potentiation | REGRA #0 (cross-cutting) |

Every plan's §5 MUST embed (alongside `cargo`/`pytest`/`touring e2e`):

```bash
# 6 BLOCK dims (P0 — fail-closed) on each file the plan touches
touring-quality check --gate F2.1 --target <FILE>   # + F2.4 F2.5 F2.6 F4.3 F4.5
# Delivery floor — Gold (0.80) on the changed tree
touring-quality score <target> --workspace --fail-below 0.80
```

⚠ Real commands only: `touring-quality {score,check,list}` (hyphen, standalone).
**NOT** `touring quality`, `score --gate`, `--enforce`, nor
`generator de qualidade dedicado (inexistente)` (PLANNED W7 → use `Edit tool`). Per-dim rules:
`~/.claude/skills/touring-elite/references/quality/D01..D52.md`.

---

## Architectural map

```
┌──────────────────────────────────────────────────────────────────────┐
│  taco-planning scripts/   (Layer 3 — the leverage)                   │
├──────────────────────────────────────────────────────────────────────┤
│  ground_truth_collector.py — Stage-1 unified Touring sweep           │
│  dimension_scorer.py       — 9 dims: keyword + symbol + blast + ...  │
│  dimension_amplifier.py    — concrete actions for each dim <7        │
│  gap_detector.py           — undefined symbols, vague claims, missing │
│  confidence_tagger.py      — FACT/INFERENCE/SPECULATION auto-tag     │
│  plan_validator.py         — 4-stage Pln2 structure + tag coverage   │
│  plan_scaffolder.py        — Jinja2 Pln2 skeleton from intent + GT   │
│  dag_builder.py            — Mermaid + parallel/sequential extract   │
│  mcts_wrapper.py           — touring mcts search wrapper             │
│  lib.py                    — Pydantic V2 frozen models + helpers     │
└──────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────────┐
│  Per-plan directory: ~/.claude/plans/<slug>/                          │
│  └ plan.md              (the Pln2 markdown)                           │
│  └ data/ground_truth.json + dimension_scores.json + ...               │
└──────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────────┐
│  Sister skill — TACO-wt OPERATES the plan once authored               │
│  • scaffold_wave → forensic_runner → cross_audit                      │
└──────────────────────────────────────────────────────────────────────┘
```

The split — `taco-planning` AUTHORS; `TACO-wt` OPERATES — is intentional. Same
9 dimensions, different lifecycle phase, no code shared (REGRA #3 evaluated;
divergence chosen because the scorer must see *intent + verification* in
authoring vs *execution evidence* in operation).

---

## VGP integration (Verified Generation Protocol)

Every plan must pass VGP — symbol verification via `touring index find` before
**any** code reference appears in the plan. The toolkit enforces this at three
gates:

| Gate | Tool | What it checks |
|------|------|----------------|
| Stage 1 | `ground_truth_collector.py` | every `--cite-symbol <S>` resolves via `touring index find` |
| Stage 2 | `dimension_scorer.py` (dim **a**) | every `path/to/file.rs:LINE` in the draft resolves |
| Stage 4 | `gap_detector.py` | flags `BLOCKED_INVENTED_SYMBOL` for any unverified citation |

A plan with even one invented symbol cannot reach Pln2. Detail in
[references/ground-truth-protocol.md](references/ground-truth-protocol.md).

---

## MCTS planning (for L4+ multi-path decisions)

When a plan has 2+ valid architectural paths, `mcts_wrapper.py` invokes
`touring mcts search <root_state>` and parses the result into a comparable
report. Use this when:

- the choice has a high blast-radius / high reversal cost
- the team disagrees on direction
- past `evolution drift` shows similar choices led to rework

Detail in [references/mcts-planning.md](references/mcts-planning.md).

---

## Lessons L1-L7 (planning-specific)

1. **Never plan from memory** — `touring index find` / `touring ast overview` first.
2. **Every bug fix has a test NAME** — not "add test" generic.
3. **Every new handler has JSON schema** — input AND output, with types.
4. **Orphans are opportunities** (REGRA #0) — `wiring orphans` reveals scope to wire.
5. **Blast radius before edit** — `touring ast blast` for every modified file.
6. **Memory lessons apply** — `memory recall` catches repeated mistakes before they re-happen.
7. **Confidence tags are non-negotiable** — every claim FACT/INFERENCE/SPECULATION or it does not ship.

---

## Cross-references map

| Topic | File |
|-------|------|
| 2-layer architecture + relation with TACO-wt | [references/architecture.md](references/architecture.md) |
| 9 dimensions detailed rubric (for authoring) | [references/dimensions-rubric.md](references/dimensions-rubric.md) |
| Amplification tactics (lift dimensions <7) | [references/amplification-strategies.md](references/amplification-strategies.md) |
| Ground-truth protocol + Touring command sequence | [references/ground-truth-protocol.md](references/ground-truth-protocol.md) |
| MCTS planning via `touring mcts search` | [references/mcts-planning.md](references/mcts-planning.md) |
| Touring CLI command quick-reference | [references/touring-commands.md](references/touring-commands.md) |
| Jinja2 plan templates | `assets/templates/*.j2` |

---

## Hard rules

1. **Ground truth before draft.** No plan line written before `ground_truth_collector.py` completes.
2. **VGP enforced.** Every cited symbol passes `touring index find` — invented symbols block Pln2.
3. **Confidence-tagged or it does not ship.** Every claim carries FACT / INFERENCE / SPECULATION with evidence.
4. **REGRA #0 honored.** Every subtask has a non-empty `Enables` row; if empty, rewrite to potentialize.
5. **Composable, not custom (REGRA #3).** Compose with `TACO-wt` for execution; do not re-implement its scripts.
6. **Code generation via `Touring-native tooling` .** No raw `Write` of `.py`/`.sh` for scripts; use `Write tool (script Python)`.
7. **Hygiene gate every refine.** SKILL.md < 500 lines (REGRA #13); add + prune together.
8. **50-dim acceptance gate in §5.** Every Pln2 plan encodes the delivery bar in its Verification Protocol: 6 BLOCK dims P0 (`touring-quality check --gate F2.1|F2.4|F2.5|F2.6|F4.3|F4.5`) + Gold floor (`touring-quality score --fail-below 0.80`). A plan without a measurable 50-dim gate ships prose, not a contract. Keystone: `~/.claude/rules/elite-50-quality.md`.

---

## Authority + renaming history

| Date | Change |
|------|--------|
| 2026-05-01 | Original skill authored after agentic-paradigm shift (Python infra archived). Prose-only, 142L SKILL.md + 88L touring-commands ref. |
| 2026-05-23 | Toolkit added — 10 scripts (~2 800 LOC) + 5 new references + 3 Jinja2 templates. Insights distilled from `pln2_generator`, `vgp`, `aco`. Diverges from TACO-wt scorers — specialized for **authoring** (vs operation). |
