# 9-Dimension Rubric — For Authoring

> **Read when**: scoring a draft plan, or arguing whether a particular sub-task
> moves the right dimension. Each dimension lists what is being measured by
> `dimension_scorer.py`, the canonical evidence, and the failure signature.

## Why these 9?

The 9 dimensions are inherited from `pln2_generator` (the canonical TACO
analytical taxonomy). They cover the dimensions of a plan that, taken together,
determine whether the plan is **executable in practice** by an engineering team
without needing the author present.

## Per-dimension rubric

### a — Precision

**Definition**: exact `file:LINE` citations, signatures verified, no hand-wavy
"somewhere in module X".

**What `dimension_scorer.py` measures**:
- Keyword density: `\d+\.\d+`, `\d+%`, `P50`, `P99`, `\d+ms`, `≥\d+`, `<\d+`.
- **Symbol verification**: every `path/to/file.rs:LINE` in the plan is
  cross-checked against `touring index find` / `touring ast find`.
- Bonus: signature embedded (e.g. `fn foo(x: u32) -> Result<Bar, Err>`).

**Failure signature**: "modify the auth module" — no file, no line, no signature.
**Amplification**: replace with `crates/auth/src/middleware.rs:142 — fn validate_token(...)`.

### b — Scalability

**Definition**: changes that compose; patterns reusable across the codebase;
no bespoke one-off hacks.

**What measures**: density of `factory`, `trait`, `interface`, `registry`,
`pattern`, `generic`, `dispatch`, `async`, `parallel`, `partition`, `shard`,
`worker`.

**Failure signature**: "add a special case for tenant X" — does not scale to
tenant Y.
**Amplification**: introduce a `TenantPolicy` trait + registry pattern.

### c — Performance

**Definition**: target latencies stated; worst-case complexity called out;
benchmarks named.

**What measures**: `latency`, `throughput`, `P50`, `P99`, `<\d+s`, `\d+ms`,
`req/s`, `O\(.*\)`, `benchmark`, `criterion`, `bench`, `SIMD`, `optim`.

**Failure signature**: "should be fast enough" with no number.
**Amplification**: declare `P99 < 50ms under 10 RPS`; name the criterion bench.

### d — Functionality

**Definition**: maximize capabilities exposed — orphans wired in, dead code
brought to life, REGRA #0.

**What measures**:
- Density of `feature`, `capabilit`, `expos`, `pub`, `surface`.
- **Wiring orphans coverage**: cross-check against `touring wiring orphans -j`
  — the plan should wire (or explicitly defer) every orphan in the target tree.

**Failure signature**: orphans listed by Touring but not addressed by the plan.
**Amplification**: add a wave that connects each orphan to its rightful consumer.

### e — Quality

**Definition**: error handling, tests named, 0 `unwrap()`, `0 errors` from
clippy/ruff/pyright.

**What measures**: `unwrap`, `panic`, `expect`, `?`, `Result`, `clippy`,
`ruff`, `pyright`, `0\s*errors`, `coverage`, `\d+%\s*coverage`, `frozen\s*=\s*True`,
`assert`, `test_\w+`, `#\[test\]`, `def test_`.

**Failure signature**: "add tests" with no test names.
**Amplification**: name each test (`test_auth_rejects_expired_token`) +
assertion (`assert resp.status == 401`).

### f — Detail

**Definition**: JSON schemas, edge cases enumerated, exact code shown.

**What measures**: `schema`, `JSON`, `pydantic`, `BaseModel`, `Cargo.toml`,
`pyproject.toml`, ` ```rust`, ` ```python`, `\.json`, `\.toml`, `edge case`,
`null`, `empty`, `boundary`.

**Failure signature**: API described but no input/output schema.
**Amplification**: paste the Pydantic / serde struct; enumerate the edges.

### g — Integration

**Definition**: cross-module wiring, MCP ↔ CLI map, hook chain documented.

**What measures**: `cross[_-]?ref`, `wir(?:e|ing)`, `hook`, `->`, `<->`,
`PreToolUse`, `PostToolUse`, `pipeline`, `integrat`, `dispatch`, `chain`,
`composes`.

**Failure signature**: the new component built in isolation without naming who
calls it.
**Amplification**: `touring wiring audit -j` then enumerate every connection the
plan adds.

### h — Dependencies

**Definition**: versions pinned, feature flags verified, compatibility matrix
present.

**What measures**: `>=\d+\.\d+`, `v\d+\.\d+`, `compatib`, `feature\s*=`,
`workspace\s*=\s*true`, `pin`, `MSRV`, `PyO3`, `pydantic`, `ruff`, `pyright`.

**Failure signature**: "use the latest tokio" — no version, no MSRV note.
**Amplification**: `tokio = { version = "1.42", features = ["sync","rt-multi-thread"] }`.

### i — Potentiation (REGRA #0)

**Definition**: every change unlocks future value. No dead-ends.

**What measures**: `enables`, `unlocks`, `compound`, `flywheel`, `multiplier`,
`growth`, `extens`, `paves\s*the\s*way`, `building\s*block`.

Also: **structural check** — every subtask must have a non-empty `Enables` row
in the table.

**Failure signature**: subtask with empty `Enables`; one-off patch.
**Amplification**: rewrite so the subtask exposes a hook/trait/extension point
others can build on.

## How the composite score is computed

```python
composite = sum(dim_score) / 9
target    = sum(max(8.5, dim_score + 3.0)) / 9   # capped at 10.0
delta     = target - composite
```

A plan with `composite ≥ 8.0` and **no dimension below 7.0** is Pln2. Anything
else gets routed to `dimension_amplifier.py` for targeted rewrites.

## Scoring is deterministic

`dimension_scorer.py` does not use an LLM. Two reasons:

1. **Reproducibility** — same input produces same scores across sessions.
2. **Speed** — scoring takes ~50ms on a 500-line plan, not 30s.

The trade-off: keyword density is a proxy. A plan can game it by stuffing
keywords. The countermeasures are:

- `gap_detector.py` checks symbols are real (VGP), claims have evidence, file
  lines exist.
- `confidence_tagger.py` downgrades unevidenced claims to SPECULATION.
- `plan_validator.py --strict` rejects plans missing one of the 4 stages.

Together the three keep the scorer honest. A keyword-stuffed plan that does not
pass gap_detector + plan_validator does not ship as Pln2.
