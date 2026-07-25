# Architecture — 2-Layer + Cross-Skill Relation

> **Read when**: first session with taco-planning, or before authoring a tool
> that touches both this skill and TACO-wt.

## The 2 operational layers

```
┌───────────────────────────────────────────────────────────────────────┐
│  TACO/Touring layer  ── ground truth · VGP · memory · MCTS · learning │
│  (taco-planning own)    confidence tags · scaffold · DAG build        │
│                                                                         │
│  10 Python scripts in scripts/ — deterministic, runnable in isolation │
└───────────────────────────────────────────────────────────────────────┘
                              │  produces / consumes
                              ▼
┌───────────────────────────────────────────────────────────────────────┐
│  Per-plan artifacts                                                   │
│  • plans/<slug>.md             (the Pln2 markdown)                    │
│  • plans/<slug>/data/*.json    (machine-readable side-cars)           │
│  • ~/.claude/touring/taco-planning/learning/<plan>.jsonl              │
└───────────────────────────────────────────────────────────────────────┘
```

Unlike skill-creator-style skills, taco-planning **does not call an LLM** in the
critical path. Every analytical step is a regex / Touring command / Pydantic
validation. The model arrives at the end to interpret the report and decide
final wording.

## The split with TACO-wt

`taco-planning` and `TACO-wt` are **complementary**, not redundant. Same 9
dimensions, distinct lifecycle moments:

| Moment | Skill | Scripts | Inputs | Outputs |
|--------|-------|---------|--------|---------|
| **Authoring** (Pln1 → Pln2) | `taco-planning` | scorer/amplifier/tagger/scaffolder/validator/dag/mcts | intent + ground truth + drafts | scored, tagged, validated `plan.md` |
| **Operation** (plan → ship) | `TACO-wt` | scaffold_wave / forensic_runner / cross_audit | `plan.md` (authored by taco-planning) | wave artifacts + composite score + audit |

Why not share code? Because the **same name** (e.g. `dimension_scorer.py`) does
different work in each skill:

- `taco-planning/dimension_scorer.py` measures **intent + verification**:
  schema completeness, JSON contracts, verified symbol citations, blast-radius
  coverage. It scores a draft plan that has not run yet.
- `TACO-wt/scripts/dimension_scorer.py` measures **execution evidence**:
  keyword density on a plan markdown that is being operated. It scores a plan
  that exists in the file system already.

Trying to merge would force one of two harms: (a) a god-object scorer with
mode flags, hurting readability; (b) a thin shared base abstracting almost
nothing, hurting cohesion. The principled choice is **divergence with
shared rubric** — same 9 dimensions canonical, separate measurement code.

## How a plan flows through both

```
1.  taco-planning ground_truth_collector.py      → data/ground_truth.json
2.  taco-planning plan_scaffolder.py             → plans/<slug>.md (skeleton)
3.  Human author + LLM fill the body
4.  taco-planning dimension_scorer.py            → scores all 9 dims
5.  taco-planning dimension_amplifier.py         → suggests amplifications
6.  Human applies amplifications, re-runs Stage 2 until all dims ≥ 7
7.  taco-planning gap_detector.py                → flags undefined symbols
8.  taco-planning confidence_tagger.py --autofill→ tags every claim
9.  taco-planning plan_validator.py --strict     → final 4-stage gate
10. Plan is Pln2 ✓ — hand off to TACO-wt
11. TACO-wt scaffold_wave.py --plan <slug>       → creates W01/, W02/, ...
12. TACO-wt forensic_runner.py --wave W01        → executes sub-scripts
13. TACO-wt cross_audit.py --plan-dir ...         → composite + recommendations
```

Authorship ends at step 10; operation begins at step 11. No script crosses the
boundary.

## When you might write a new script

| Use case | Where it goes |
|----------|---------------|
| New analysis of intent / draft (pre-execution) | `taco-planning/scripts/` |
| New analysis of wave artifacts (post-execution) | `TACO-wt/scripts/` |
| New analysis that needs BOTH ground truth AND wave artifacts | extract a common helper into either lib, then have a thin wrapper in each side |

The "thin wrapper in each side" rule is the practical recipe to keep the
divergence working without leaking abstractions.

## Daemon-down behavior

If `touring doctor -j` reports degraded components, every taco-planning script
runs in **fallback mode**:

- `ground_truth_collector.py` writes the ground_truth.json with `daemon_degraded: true`.
- VGP symbol checks fall back to `grep -rn '<symbol>' <root>` (slower, less precise).
- `mcts_wrapper.py` returns `{"mode": "skip", "reason": "daemon_unavailable"}`.

The plan is still authored — only the verification confidence drops. The
confidence_tagger picks this up and downgrades affected claims to INFERENCE.
