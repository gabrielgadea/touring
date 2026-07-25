# Pipeline Patterns

> **Read when**: designing a new multi-wave generator (a wave-style pipeline
> that produces parts of a document, parts of a refactor, parts of a plan).
> **Origin**: distilled from `analise/scripts/pln2_generator` —
> the canonical 6-stage Pln2² pipeline.

---

## The 6-stage canonical pipeline

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ 1. source    │ →  │ 2. dimension │ →  │ 3. gap       │
│    parser    │    │    analyzer  │    │    detector  │
└──────────────┘    └──────────────┘    └──────────────┘
                                                │
                                                ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ 6. toon      │ ←  │ 5. validator │ ←  │ 4. part      │
│    checkpoint│    │              │    │    generator │
└──────────────┘    └──────────────┘    └──────────────┘
```

| Stage | Input | Output | Determinism |
|-------|-------|--------|-------------|
| 1. parser | markdown docs | `{filename, frontmatter, content, principles[]}` | regex-only, no LLM |
| 2. dim analyzer | doc content | `DimensionScore × 9` | keyword density → 0-10 score |
| 3. gap detector | principles + content | `Gap × N` with P0-P3 severity | set algebra (principle-IDs not found in content) |
| 4. part generator | dims + gaps + principles | markdown parts with YAML frontmatter | template assembly |
| 5. validator | output dir | `{frontmatter_errors, cross_ref_errors, token_violations, overall_pass}` | structural check |
| 6. checkpoint | pipeline data | TOON v1.0 file with blake2b hash | content-addressed, idempotent |

---

## Why this works for waves

The pipeline maps cleanly onto a multi-wave plan:

| Pipeline stage | TACO-wt equivalent | TACO-wt script |
|----------------|---------------------|----------------|
| 1. parser | parse `plan.md` frontmatter + wave declarations | `plan_validator.py` |
| 2. dim analyzer | score 9 dimensions of the plan doc | `dimension_scorer.py` |
| 3. gap detector | detect missing wave coverage / unverified premises | `gap_detector.py` |
| 4. part generator | scaffold wave directories + sub-scripts | `scaffold_wave.py` |
| 5. validator | per-wave validator + cross-audit | `cross_audit.py` + `validate_W<N>.py` |
| 6. checkpoint | TOON snapshot of wave-complete state | `toon_checkpoint.py` |

---

## The 9 canonical dimensions

Same 9 used by `pln2_generator/dimension_analyzer.py`:

| Dimension | What it measures | Keyword signal |
|-----------|------------------|----------------|
| `precision` | Exact numeric targets | `\d+\.\d+`, `\d+%`, `≥\d+`, `P50`, `P99` |
| `scalability` | Horizontal scaling, async, partition | `scal(?:e|ing|ability)`, `horizontal`, `shard`, `async` |
| `performance` | Latency, throughput, SIMD | `latency`, `throughput`, `<\d+s`, `SIMD`, `benchmark` |
| `functionality` | Feature surface area | `\d+\s*skills?`, `feature`, `capabilit`, `pipeline` |
| `code_quality` | Lint, type, coverage | `ruff`, `pyright`, `coverage`, `frozen\s*=\s*True` |
| `detail` | Concrete paths, LOC, pseudocode | `\.py\b`, `LOC`, `path:`, `pseudocode` |
| `integration` | Cross-references, hooks | `cross[_-]?ref`, `integrat`, `->`, `<->`, `wir(?:e|ing)` |
| `dependencies` | Versions, compatibility | `>=\d+`, `v\d+\.\d+`, `compatib`, `pin` |
| `potentiation` | Compounding, flywheel | `flywheel`, `multiplier`, `compound`, `autonomous` |

Each is scored 0-10 by keyword-density (`hits / (total_lines / 100)`). The
density-to-score table is in `dimension_scorer.py`.

Pln2-target = `max(8.5, pln1_score + 3.0)`, capped at 10.0. The **delta**
(pln2 - pln1) is what waves are designed to close.

---

## Gap severity P0-P3

Inherited from `pln2_generator/gap_detector.py`:

| Severity | Category source | Action timing |
|----------|-----------------|---------------|
| **P0** | epistemological, architectural | Block plan acceptance until covered |
| **P1** | state, evolution | Resolve before mid-plan |
| **P2** | quality, protocol | Resolve before plan close |
| **P3** | (default if no category match) | Document; defer if time-bound |

A gap has `{id, principle_id, current_state, target_state, severity, remediation}`.
`gap_detector.py` produces them; `cross_audit.py` rolls them up.

---

## TOON v1.0 envelope

Inherited from `pln2_generator/toon_checkpoint.py`:

```toon
format: TOON
format_version: 1.0
kind: wave-checkpoint
wave: W12
timestamp: 2026-05-23T15:22:00Z
hash_chain: <blake2b-256-hex>
data:
  status: PASS
  score: 0.87
  sub_results: ...
  artifacts: ...
```

The `hash_chain` is `blake2b(digest_size=32)` of the JSON-canonicalized
`data` payload — gives content-addressed idempotency. Re-running the
checkpoint with identical data produces an identical file.

---

## Pydantic V2 frozen models

The contract objects are immutable. The same pattern lives in `lib.py`:

```python
from pydantic import BaseModel, ConfigDict, Field

class WaveFinding(BaseModel):
    model_config = ConfigDict(frozen=True)

    file: str = Field(description="Relative path to affected file")
    pattern: str = Field(description="Pattern that matched")
    line: int = Field(ge=1, description="1-based line number")
    severity: Literal["P0", "P1", "P2", "P3"] = "P2"
    context: str = Field(default="", description="±2 lines of context")
```

Frozen prevents accidental mutation in the runner / collector. Field constraints
catch malformed JSON at deserialization.

---

## Regex-only NLP

The pln2_generator parser uses regex patterns (not spaCy / not NLTK). Same
discipline in TACO-wt:

- Match principle/section/wave declarations with explicit `re.compile`.
- Confidence levels are uppercase enums (`HIGH | MEDIUM | LOW`).
- Bold concepts are filtered by a noise list (`{"primeiro", "table", "figure", ...}`).
- No semantic embedding, no LLM in the path.

The trade-off: TACO-wt sub-scripts run in **seconds**, not minutes, and produce
**byte-identical** JSON on rerun.

---

## Categorization keywords (from source_parser.py)

For wave classification, the inherited taxonomy is:

| Category | Sample keywords |
|----------|----------------|
| `epistemological` | computation, deterministic, probabilistic, verify, zero-hallucination |
| `architectural` | architecture, layer, sandbox, actor, isolation, encapsulation |
| `state` | repl, loop, checkpoint, recovery, WAL, event, knowledge, tier |
| `evolution` | learning, drift, mutation, evolv, autonomous, self-heal, cron |
| `quality` | lint, ruff, pyright, coverage, test, type, ci/cd, complexity |
| `protocol` | contract, interface, protocol, plugin, registry, obligation, spec, api |

`gap_detector.py` uses these categories to assign P0/P1/P2/P3 severity.
