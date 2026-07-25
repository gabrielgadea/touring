---
plan: pipeline-premium-elevator
title: Elevate pipeline_runner.py to Premium Market Product
authored: 2026-05-26
level: L3
status: DRAFT
intent: |
  elevate pipeline_runner.py to premium market product
quality_dimensions:
  - precision
  - scalability
  - performance
  - functionality
  - quality
  - detail
  - integration
  - dependencies
  - potentiation
ground_truth_ref: data/ground_truth.json
toolkit_version: taco-planning-v2.0
---

# Elevate pipeline_runner.py to Premium Market Product (Pln2)

> Level: L3 | Registry: current → premium | Scope: 7 FCs + 12 Ps (12-week roadmap)
> **Daemon state**: DEGRADED — claims tagged INFERENCE where Touring evidence unavailable

---

## 1. Ground Truth Summary

| Metric | Value | Confidence |
|--------|-------|------------|
| e2e composite | **N/A (daemon degraded)** | INFERENCE [0.7] |
| wiring orphans | **10963** (full workspace) | FACT [1.0] |
| symbols verified (by GT) | **0** | FACT [1.0] |
| lessons applied | **10** | FACT [1.0] |
| TDG grade (current) | **D (0.669)** | FACT [1.0] |
| TDG cognitive (current) | **0.549** | FACT [1.0] |

**Symbols verified in this plan** (VGP — each must resolve before Pln2):
- `PipelineRunner` → `scripts/process_analysis/pipeline_runner.py:182`
- `PhaseStrategy` → `scripts/process_analysis/pipeline_runner.py:182`
- `TACOCoordinator` → `scripts/process_analysis/pipeline_runner.py:6`
- `ApexCheckpointManager` → `scripts/process_analysis/pipeline_runner.py:7`
- `EvidenceChain` → `scripts/process_analysis/pipeline_runner.py:9`
- `QualityGateOrchestrator` → `scripts/process_analysis/pipeline_runner.py:10`

---

## 2. 9-Dimension Scores (current → target → delta)

| Dim | Current | Target | Delta | Amplification |
|Dim| Current |Target|Delta|Amplification|
|------|---------|--------|------|---------------|
| **a** | Precision | 8.00 | 9.00 | +1.00 | _amplifier pending_ |
| **b** | Scalability | 7.00 | 9.00 | +2.00 | _amplifier pending_ |
| **c** | Performance | 7.00 | 9.00 | +2.00 | **Pf-1**: Declare 'P99 < N ms under M RPS; bench: `criterion::cache_hit_bench`.' **Pf-2**: For every hot path: 'O(n) over n = active sessions (~10k typical).' |
| **d** | Functionality | 8.00 | 9.00 | +1.00 | **F-1**: For each orphan in `wiring_orphans` not addressed, add subtask wiring it (or document why deferred). **F-2**: Name every new `pub` symbol + at least one consumer. |
| **e** | Quality | 8.00 | 9.00 | +1.00 | **Q-1**: Replace 'add tests' with `test_<name>` + exact assertion. **Q-2**: Replace all `unwrap()` with `?` or `.unwrap_or_else(|e| ...)`. |
| **f** | Detail | 8.00 | 9.00 | +1.00 | _amplifier pending_ |
| **g** | Integration | 8.00 | 9.00 | +1.00 | _amplifier pending_ |
| **h** | Dependencies | 7.00 | 9.00 | +2.00 | **Dep-1**: Replace `tokio = "*"` with `tokio = { version = "1.42", features = [...] }`. **Dep-2**: Document MSRV / Python range and why required. |
| **i** | Potentiation | 7.00 | 9.00 | +2.00 | **Pt-1**: For every subtask with empty Enables, rewrite so change exposes hook/trait/extension point. **Pt-2**: Add Potentiation Matrix showing how each subtask compounds. |

---

## 3. Functional Criticalities (7FCs — pre-existing bugs)

| FC | Severity | Title | Locations | Evidence | Fix |
|---|----------|-------|-----------|---------|-----|
| FC | Severity | Title | Locations | Evidence | Fix |
|---|----------|-------|-----------|---------|-----|
| **FC-1** | HIGH | asyncio.run() creates/destroys event loop per call | L409,L517 | asyncio.run(hook.pre_phase(...)) inside run_phase() — called... | Use asyncio.Runner context manager (Python 3.11+) or single ... |
| **FC-2** | HIGH | ThreadPoolExecutor for CPU-bound parallel phases (GIL-bound) | L775-778 | concurrent.futures.ThreadPoolExecutor running _run_one for F... | Replace with ProcessPoolExecutor for CPU-bound phases OR run... |
| **FC-3** | MEDIUM | run_phase() cyclomatic complexity = 28 (8+ responsibilities mixed) | L376-544 | CC=28 — should_run_phase + APEX cache + state mutation + asy... | Extract into 5 single-responsibility methods: _should_run, _... |
| **FC-4** | HIGH | No per-phase timeout (potential infinite blocking) | Throughout run_phase | No timeout argument to run_phase(), _run_parallel_phase_grou... |  asyncio.wait_for(phase_fn(), timeout=phase.timeout) per pha... |
| **FC-5** | MEDIUM | Quality gate failure = immediate pipeline abort, no retry | L603-637 _enforce_gate | _enforce_gate: verdict.processing_failed or quality_below_th... | Retry budget (e.g., 2 retries) before abort; circuit-breaker... |
| **FC-6** | MEDIUM | Race condition: get_phase_strategy() read without lock | L200-206 | global _current_strategy write is locked, but read in get_ph... | Lock in get_phase_strategy() read: with _strategy_lock: retu... |
| **FC-7** | LOW | Import inside except block masks original exception | L443 | except Exception as e:\n    import traceback as _tb... | Move import traceback to module top-level; handle ImportErro... |


---

## 4. Prioritized Improvements (12 Ps)

### PP1: Per-phase timeout + retry budget [HIGH] [P1--FC-4]
- **LOC estimate**: ~~40
- **Enables**: OpenTelemetry trace spans with deadline; circuit breaker per phase
- **FC reference**: [FC-4] — FC-4
- **Impact**: Eliminates pipeline-wide hung-phase failure; partial results preserved
- **Test**: `test_pp1_{}` + assertion of per-phase
- **Verification**: `cargo test -p pipeline_runner -- PP1`
### PP2: Lock in get_phase_strategy() read (race condition FC-6) [MEDIUM] [P2--FC-6]
- **LOC estimate**: ~1
- **Enables**: Concurrent TACO hook invocations from multiple phases
- **FC reference**: [FC-6] — FC-6
- **Impact**: Thread-safe PhaseStrategy reads; zero regression risk
- **Test**: `test_pp2_{}` + assertion of lock
- **Verification**: `cargo test -p pipeline_runner -- PP2`
### PP3: ProcessPoolExecutor for BlocoB CPU-bound phases [HIGH] [P3--FC-2]
- **LOC estimate**: ~~20
- **Enables**: GPU service integration; Rust APEX acceleration; MEF grid extraction
- **FC reference**: [FC-2] — FC-2
- **Impact**: True parallelism for F10||F11||F12||F13||F14||F15; ~N× speedup on N cores
- **Test**: `test_pp3_{}` + assertion of processpoolexecutor
- **Verification**: `cargo test -p pipeline_runner -- PP3`
### PP4: Extract run_phase() into 5 single-responsibility methods [MEDIUM] [P4--FC-3]
- **LOC estimate**: ~~80
- **Enables**: Phase-level observability; granular error handling per responsibility
- **FC reference**: [FC-3] — FC-3
- **Impact**: CC 28→4 per method; testable in isolation; quality TDG D→B+
- **Test**: `test_pp4_{}` + assertion of extract
- **Verification**: `cargo test -p pipeline_runner -- PP4`
### PP5: Retry quality gate failures (transient vs persistent) [MEDIUM] [P5--FC-5]
- **LOC estimate**: ~~35
- **Enables**: Adaptive quality threshold; circuit breaker with half-open state
- **FC reference**: [FC-5] — FC-5
- **Impact**: Resilient pipeline; composite score elevates 0.669→0.85+
- **Test**: `test_pp5_{}` + assertion of retry
- **Verification**: `cargo test -p pipeline_runner -- PP5`
### PP6: Async hooks without asyncio.run() via asyncio.Runner [HIGH] [P6--FC-1]
- **LOC estimate**: ~~30
- **Enables**: Persistent hook state across phases; pre/post phase metrics aggregation
- **FC reference**: [FC-1] — FC-1
- **Impact**: Eliminates event loop creation/destruction overhead per hook; ~1-3ms saved × N phases
- **Test**: `test_pp6_{}` + assertion of async
- **Verification**: `cargo test -p pipeline_runner -- PP6`
### PP7: Pydantic v2 pipeline API — typed PhaseConfig, PhaseResult, ProcessDeps [MEDIUM] [P7--FC-3]
- **LOC estimate**: ~~100
- **Enables**: JSON Schema generation for pipeline config; OpenAPI docs for pipeline runner CLI
- **FC reference**: [FC-3] — FC-3
- **Impact**: Compile-time validation of phase config; autocomplete in IDE; self-documenting contracts
- **Test**: `test_pp7_{}` + assertion of pydantic
- **Verification**: `cargo test -p pipeline_runner -- PP7`
### PP8: Circuit breaker per phase (FC-5 escalation path) [MEDIUM] [P8--FC-5]
- **LOC estimate**: ~~60
- **Enables**: Dashboard alert 'phase N circuit open'; automated escalation to human review
- **FC reference**: [FC-5] — FC-5
- **Impact**: Persistent quality failures open circuit → skip phase with evidence preserved
- **Test**: `test_pp8_{}` + assertion of circuit
- **Verification**: `cargo test -p pipeline_runner -- PP8`
### PP9: RSS backpressure + MemoryManager adaptive batching [MEDIUM] [P9--None]
- **LOC estimate**: ~~50
- **Enables**: 16-phase pipeline on memory-constrained environments; proactive OOM prevention
- **FC reference**: [None] — None
- **Impact**: Pipeline adapts batch size to available memory; MemoryError becomes gracefully handled
- **Test**: `test_pp9_{}` + assertion of rss
- **Verification**: `cargo test -p pipeline_runner -- PP9`
### PP10: OpenTelemetry distributed tracing (phase spans) [LOW] [P10--None]
- **LOC estimate**: ~~80
- **Enables**: Alerting on P99 > threshold; flame graphs per phase; cost attribution by corridor
- **FC reference**: [None] — None
- **Impact**: End-to-end traces for every phase; P50/P99 latency per phase; propagation to Grafana
- **Test**: `test_pp10_{}` + assertion of opentelemetry
- **Verification**: `cargo test -p pipeline_runner -- PP10`
### PP11: DAG config — pipeline_description.yaml + --dag flag [LOW] [P11--None]
- **LOC estimate**: ~~40
- **Enables**: Pipeline visualizer (Mermaid diagram from YAML); non-programmer editing of phase order
- **FC reference**: [None] — None
- **Impact**: Pipeline structure declared in YAML; validation at startup; visual DAG rendering
- **Test**: `test_pp11_{}` + assertion of dag
- **Verification**: `cargo test -p pipeline_runner -- PP11`
### PP12: Rich CLI dashboard + progress bar (tqdm + rich.Table) [LOW] [P12--None]
- **LOC estimate**: ~~45
- **Enables**: CI/CD integration with JUnit XML output; Datadog APM integration
- **FC reference**: [None] — None
- **Impact**: Human-friendly output; real-time phase progress; color-coded status
- **Test**: `test_pp12_{}` + assertion of rich
- **Verification**: `cargo test -p pipeline_runner -- PP12`


---

## 5. Phases — Detailed Implementation

### P1: Per-phase timeout + retry budget
**File**: `scripts/process_analysis/pipeline_runner.py`
**Severity**: HIGH | **Lines**: ~40
**Confidence**: FACT [1.0] (asyncio.wait_for is stdlib)

Add to `PhaseConfig`:
```python
from dataclasses import dataclass, field
from typing import Optional
import asyncio

@dataclass
class PhaseConfig:
    name: str
    fn: str
    timeout: float = 300.0       # seconds, default 5min
    retry_budget: int = 2       # retries on transient failure
    depends_on: list[str] = field(default_factory=list)
    max_parallel: int = 4
```

Change `run_phase()`:
```python
async def _run_phase_async(self, name: str, fn: str, cfg: PhaseConfig) -> PhaseResult:
    try:
        result = await asyncio.wait_for(
            self._execute_phase(fn),
            timeout=cfg.timeout
        )
    except asyncio.TimeoutError:
        return PhaseResult(
            status='partial',
            phase_name=name,
            output={},
            quality_score=0.0,
            error='TimeoutError'
        )
    return result

def run_phase(self, name: str, fn: str, cfg: PhaseConfig | None = None) -> PhaseResult:
    cfg = cfg or PhaseConfig(name=name, fn=fn)
    result = asyncio.run(self._runner.run(self._run_phase_async(name, fn, cfg)))
    # retry on transient quality failure
    if result.status == 'failed' and cfg.retry_budget > 0 and self._is_transient(result.error):
        return self.run_phase(name, fn, PhaseConfig(
            name=cfg.name, fn=cfg.fn, timeout=cfg.timeout,
            retry_budget=cfg.retry_budget - 1,
            depends_on=cfg.depends_on, max_parallel=cfg.max_parallel
        ))
    return result
```

**Test**: `test_P1_timeout_returns_partial` + `test_P1_retry_budget_decrements`

---

### P2: Lock in get_phase_strategy() read (FC-6 fix)
**File**: `scripts/process_analysis/pipeline_runner.py:200-206`
**Confidence**: FACT [1.0]

```python
# BEFORE (FC-6):
def get_phase_strategy() -> PhaseStrategy:
    return _current_strategy  # READ NOT LOCKED

# AFTER:
def get_phase_strategy() -> PhaseStrategy:
    with _strategy_lock:
        return _current_strategy
```

**Test**: `test_P2_concurrent_read_returns_consistent_value` (spawn 10 threads, all read simultaneously)

---

### P3: ProcessPoolExecutor for BlocoB (FC-2 fix)
**File**: `scripts/process_analysis/pipeline_runner.py:775-778`
**Confidence**: INFERENCE [0.85] (ProcessPoolExecutor, no context7 confirmation yet)

```python
# BEFORE (ThreadPoolExecutor — GIL-bound):
with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as executor:

# AFTER (ProcessPoolExecutor — true parallelism):
# Warning: _run_one must be a top-level function (not a lambda) for pickling
with concurrent.futures.ProcessPoolExecutor(max_workers=max_workers) as executor:
    futures = {
        executor.submit(_run_one, name, fn): (name, fn)
        for name, fn in phase_pairs
    }
    for future in concurrent.futures.as_completed(futures):
        result = future.result(timeout=cfg.timeout if cfg else 300.0)
```

**Test**: `test_P3_speedup_benchmark` (compare wall-clock time F10||F11||F12||F13)

---

### P4: Extract run_phase() into 5 single-responsibility methods (FC-3 fix)
**File**: `scripts/process_analysis/pipeline_runner.py`
**Confidence**: FACT [1.0]

```python
def _should_run_phase(self, phase_name: str) -> bool:
    """Gate: should this phase even execute given state?"""
    ...

def _check_apex_cache(self, phase_name: str) -> PhaseResult | None:
    """APEX PhaseCache lookup; returns cached PhaseResult if found."""
    ...

async def _execute_with_hooks(self, phase_fn: str, cfg: PhaseConfig) -> PhaseResult:
    """Runs the actual phase fn with pre/post hooks via asyncio.Runner."""
    hook_result = self._runner.run(hook.pre_phase(self._taco_state, self.deps))
    result = await self._execute_phase(phase_fn)
    post_result = self._runner.run(hook.post_phase(self._taco_state, self.deps, result))
    return result

def _handle_phase_result(self, result: PhaseResult, phase_name: str) -> PhaseResult:
    """State mutations, checkpoint save, EvidenceChain."""
    ...

def _enforce_quality_gate(self, result: PhaseResult) -> PhaseResult:
    """Quality gate with retry on transient failure."""
    ...
```

**Test**: `test_P4_each_method returns expected output in isolation`

---

### P6: Async hooks via asyncio.Runner (FC-1 fix)
**File**: `scripts/process_analysis/pipeline_runner.py:409,517`
**Confidence**: INFERENCE [0.85] (context7 confirms Runner pattern; implementation needs t


## context7: asyncio.Runner (Python 3.11+)
Use `asyncio.Runner` context manager to run multiple async functions in a
shared event loop without creating/destroying a loop per call:

    with asyncio.Runner() as runner:
        runner.run(async_operation_one())
        blocking_code()          # interleaved
        runner.run(async_operation_two())

**优于 asyncio.run()**: avoids ~1-3ms loop creation per call; loop persists across
multiple invocations within the same runner context.


Replace per-call `asyncio.run()` with persistent `asyncio.Runner()`:

```python
class PipelineRunner:
    def __init__(self, ...):
        self._runner = asyncio.Runner()

    def run_phase(self, ...):
        # Pre-phase hook — no more asyncio.run() per call
        hook_result = self._runner.run(hook.pre_phase(self._taco_state, self.deps))
        result = asyncio.run(self._run_phase_async(...))  # still uses run for main phase
        bus_result = self._runner.run(hook.post_phase(self._taco_state, self.deps, result))

    def close(self):
        if hasattr(self, '_runner'):
            self._runner.close()
            # Runner.__del__ also closes automatically
```

**Python 3.11+ requirement** — document in `pyproject.toml`: `requires-python = ">=3.11"`.

**Test**: `test_P6_runner_persistent_across_phases` (assert single Runner instance across 16 phases)

---

### P7: Pydantic v2 models (FC-3 escalation)
**File**: `scripts/process_analysis/pipeline_runner.py`
**Models**: `PhaseConfig`, `PhaseResult`, `ProcessDeps`, `TACOState`


## context7: Pydantic v2 Validators
- `field_validator(mode='before')` — preprocess input before type coercion
- `field_validator(mode='after')` — run after fields are validated
- `model_validator(mode='after')` — cross-field validation on complete model

    @field_validator('phase_timeout', mode='before')
    @classmethod def ensure_positive(cls, v):
        if v is None:
            return 300.0  # default 5min
        if v < 0:
            raise ValueError('timeout must be non-negative')
        return float(v)

    @model_validator(mode='after')
    def check_phase_order(self):
        if self.max_parallel_phases < len(self.parallel_blocks):
            raise ValueError('max_parallel < parallel blocks count')
        return self


Add to `pipeline_runner.py` or `pipeline_models.py`:

```python
from pydantic import BaseModel, field_validator, model_validator
from typing import Optional

class PhaseConfig(BaseModel):
    name: str
    fn: str
    timeout: float = 300.0
    retry_budget: int = 2
    depends_on: list[str] = []
    max_parallel: int = 4

    @field_validator('timeout', mode='before')
    @classmethod def coerce_timeout(cls, v):
        if v is None:
            return None
        try:
            return float(v)
        except (TypeError, ValueError):
            raise ValueError('timeout must be numeric')

    @field_validator('retry_budget', 'max_parallel', mode='before')
    @classmethod def coerce_ints(cls, v):
        if v is None:
            return None
        try:
            return int(v)
        except (TypeError, ValueError):
            raise ValueError('field must be numeric')

    @model_validator(mode='after')
    def check_no_self_dependency(self):
        if self.name in self.depends_on:
            raise ValueError(f'Phase {self.name} cannot depend on itself')
        return self

class PhaseResult(BaseModel):
    status: Literal['success', 'failed', 'partial', 'skipped']
    phase_name: str
    output: dict
    quality_score: float
    error: Optional[str] = None
    elapsed_ms: float

    @field_validator('quality_score', mode='before')
    @classmethod def clamp_quality(cls, v):
        return max(0.0, min(1.0, float(v) if v is not None else 0.0))
```

**Test**: `test_P7_self_dependency_rejected`
 `test_P7_quality_clamped_0_1`

---

## 6. Potentiation Matrix

| Change | Enables |
|---------|---------|
| PP1: Per-phase timeout + retry budget | OpenTelemetry trace spans with deadline; circuit breaker per phase |
| PP2: Lock in get_phase_strategy() read (race condition FC-6) | Concurrent TACO hook invocations from multiple phases |
| PP3: ProcessPoolExecutor for BlocoB CPU-bound phases | GPU service integration; Rust APEX acceleration; MEF grid extraction |
| PP4: Extract run_phase() into 5 single-responsibility methods | Phase-level observability; granular error handling per responsibility |
| PP5: Retry quality gate failures (transient vs persistent) | Adaptive quality threshold; circuit breaker with half-open state |
| PP6: Async hooks without asyncio.run() via asyncio.Runner | Persistent hook state across phases; pre/post phase metrics aggregation |
| PP7: Pydantic v2 pipeline API — typed PhaseConfig, PhaseResult, ProcessDeps | JSON Schema generation for pipeline config; OpenAPI docs for pipeline runner CLI |
| PP8: Circuit breaker per phase (FC-5 escalation path) | Dashboard alert 'phase N circuit open'; automated escalation to human review |
| PP9: RSS backpressure + MemoryManager adaptive batching | 16-phase pipeline on memory-constrained environments; proactive OOM prevention |
| PP10: OpenTelemetry distributed tracing (phase spans) | Alerting on P99 > threshold; flame graphs per phase; cost attribution by corridor |
| PP11: DAG config — pipeline_description.yaml + --dag flag | Pipeline visualizer (Mermaid diagram from YAML); non-programmer editing of phase order |
| PP12: Rich CLI dashboard + progress bar (tqdm + rich.Table) | CI/CD integration with JUnit XML output; Datadog APM integration |


---

## 7. Verification Protocol

```bash
# Phase-level
cargo test -p pipeline_runner -- P1
cargo test -p pipeline_runner -- P2
cargo test -p pipeline_runner -- P3
cargo test -p pipeline_runner -- P4
cargo test -p pipeline_runner -- P6
cargo test -p pipeline_runner -- P7

# Integration
cargo test -p pipeline_runner -- --trace
touring e2e -j
touring ast tdg scripts/process_analysis/pipeline_runner.py  # expect grade D -> C or better
```

**Metrics to validate** (target after all Ps implemented):
| Metric | Before | After |
|--------|--------|-------|
| TDG grade | D (0.669) | B+ (~0.80) |
| asyncio.run calls per 16-phase run | 32 (2 per phase) | 1 (Runner reused) |
| ThreadPoolExecutor → ProcessPoolExecutor | Threads | Processes |
| Per-phase timeout | None | 300s default |
| Quality gate retry | 0 | 2 |
| Cyclomatic complexity run_phase | 28 | 5 per method |

---

## 8. OpenTelemetry Distributed Tracing Integration (P10)

Add tracing spans per phase:
```python
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter

provider = TracerProvider()
processor = BatchSpanProcessor(OTLPSpanExporter(endpoint="http://localhost:4317"))
provider.add_span_processor(processor)
trace.set_tracer_provider(provider)
tracer = trace.get_tracer(__name__)

def run_phase(self, ...):
    with tracer.start_as_current_span(f"phase.{name}") as span:
        span.set_attribute("phase.name", name)
        span.set_attribute("phase.timeout", cfg.timeout)
        result = self._run_phase_async(...)
        span.set_attribute("phase.status", result.status)
        span.set_attribute("phase.quality_score", result.quality_score)
        return result
```

**Trace propagation**: W3C `traceparent` header injected into HTTP calls for distributed pipeline across services.

---

## 9. Implementation Order (DAG)

```
Phase 0 (GATE)
└── P2 (lock read)        ← smallest, zero-risk, immediate
└── P6 (asyncio.Runner)    ← foundational for P1, P4
└── P1 (timeout+retry)     ← depends on P6
└── P4 (extract methods)  ← depends on P1
└── P3 (ProcessPool)       ← depends on P4 extracted interfaces
└── P7 (Pydantic v2)      ← depends on P4 extracted interfaces
└── P5 (retry quality)    ← depends on P1
└── P8 (circuit breaker)  ← depends on P5
└── P9 (RSS backpressure)  ← independent
└── P10 (OpenTelemetry)    ← depends on P4
└── P11 (DAG YAML)         ← independent
└── P12 (Rich CLI)         ← independent
```

---

## 10. T-Shirt Sizing & Effort Estimate

| Phase | Name | Effort | Weeks | TDG Target |
|-------|------|--------|-------|------------|
| P1 | Per-phase timeout + retry | M | 1 | +0.05 |
| P2 | Lock in get_phase_strategy | S | 0.5 | +0.01 |
| P3 | ProcessPoolExecutor | L | 2 | +0.10 |
| P4 | Extract run_phase() | XL | 3 | +0.15 |
| P5 | Retry quality gate | M | 1 | +0.05 |
| V6 | asyncio.Runner | M | 1 | +0.08 |
| P7 | Pydantic v2 models | L | 2 | +0.10 |
| P8 | Circuit breaker | M | 1 | +0.05 |
| P9 | RSS backpressure | M | 1 | +0.03 |
| P10 | OpenTelemetry | L | 2 | +0.05 |
| P11 | DAG YAML | S | 0.5 | +0.02 |
| P12 | Rich CLI | S | 0.5 | +0.02 |

**Total**: 12 weeks, 4 XL effort units.

