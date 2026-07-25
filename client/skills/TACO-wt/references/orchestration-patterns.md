# Orchestration Patterns

> **Read when**: extending `forensic_runner.py` to coordinate multiple waves,
> adding pre/post hooks to a wave, or wiring TACO-wt into a larger pipeline.
> **Origin**: distilled from `analise/scripts/aco/orchestrator.py` (7-phase ACO).

---

## The 7-phase ACO model — adapted for waves

ACO orchestrates 7 phases from raw prompt to evolution package. The same
structure applies cleanly to wave execution:

| ACO phase | TACO-wt wave equivalent | Sub-script |
|-----------|--------------------------|-----------|
| 0. Intent | Read plan frontmatter | `plan_validator.py` |
| 1. Perception | Discover real state (forensic) | `wN_discover_*.py` |
| 2. Decomposition | Break wave into sub-tasks (DAG) | `scaffold_wave.py --sub-scripts N` |
| 3. Generation | Execute sub-scripts (dry-run default) | `forensic_runner.py` |
| 4. Tracking | Validate per-wave | `validate_W<N>.py` |
| 5. Refinement | On failure, retry with backoff | `forensic_runner.py --retry` |
| 6. Consolidation | Cross-audit + checkpoint | `cross_audit.py` + `toon_checkpoint.py` |

---

## Pre/post phase hooks

Inspired by ACO's `pre_phase` / `post_phase` pattern. In TACO-wt:

```python
class WaveHook(Protocol):
    def pre_wave(self, state: WaveState) -> HookResult: ...
    def post_wave(self, state: WaveState, results: list[dict]) -> HookResult: ...

@dataclass(frozen=True)
class HookResult:
    action: Literal["continue", "abort", "retry"]
    quality_score: float | None = None
    reason: str = ""
```

Default `GenericWaveHook` reports `quality_score=1.0` always. Custom hooks
can implement:

- Pre-wave check: "is `cargo check` clean before we run this wave?"
- Post-wave check: "did any sub-script leave artifacts in unexpected paths?"

Hooks are registered per-plan in `.taco-wt/hooks.py`. The runner loads them
via importlib.

---

## TACOState — coherent wave-context

ACO's `TACOState`:

```python
@dataclass(frozen=True)
class TACOState:
    process_id: str
    phase_id: str
    mission_context: str
    error_history: list[str]
    quality_score: dict[str, float]
```

TACO-wt's `WaveState`:

```python
@dataclass(frozen=True)
class WaveState:
    plan: str
    wave_id: str           # "W12"
    sub_id: str | None     # "W12.discover_consumers"
    started_at: str        # ISO 8601
    error_history: tuple[str, ...] = ()
    quality_score: float | None = None
    apply_mode: bool = False

    def with_sub(self, sub_id: str) -> WaveState: ...
    def with_error(self, err: str) -> WaveState: ...
    def with_quality(self, score: float) -> WaveState: ...
```

Frozen + `with_*` methods → immutable updates. The runner threads `WaveState`
through every sub-script invocation.

---

## Refinement loop (max=3)

ACO caps refinement at 3 iterations. TACO-wt inherits this:

```python
MAX_REFINEMENT_ITERATIONS = 3

def run_with_refinement(wave: str) -> CrossAuditReport:
    for attempt in range(MAX_REFINEMENT_ITERATIONS):
        report = forensic_runner.run(wave)
        validator = validate_W(wave)
        if validator["status"] == "PASS":
            return report
        # Identify failures, attempt targeted fix
        refine(wave, validator["child_results"])
    # Give up — return last report with status FAIL
    return report
```

The cap prevents infinite loops on systemic failures. Beyond 3 attempts, the
problem is not local — escalate to operator.

---

## UnifiedCheckpointManager pattern

ACO uses `UnifiedCheckpointManager(domain="aco")`. TACO-wt analog:

```python
class TacoWtCheckpointManager:
    def __init__(self, plan: str):
        self._root = Path(f".claude/checkpoints/{plan}")

    def save(self, name: str, data: dict) -> Path:
        """Persist a TOON v1.0 checkpoint with blake2b hash chain."""
        path = self._root / f"{name}_{date_str}.toon"
        path.parent.mkdir(parents=True, exist_ok=True)
        # ... emit TOON with hash_chain ...
        return path

    def latest(self, name_prefix: str) -> Path | None:
        """Return most recent checkpoint matching prefix."""
        ...

    def list_all(self) -> list[Path]:
        ...
```

Implemented in `toon_checkpoint.py`. Checkpoints survive process restart;
the orchestrator can resume mid-plan after a crash.

---

## Parallel execution

ACO's `parallel_generator_engine.py` runs generator nodes concurrently.
TACO-wt's `forensic_runner.py` does the same for sub-scripts:

```python
def run_parallel(wave_dir: Path, scripts: list[Path], max_workers: int = 4) -> list[dict]:
    with ThreadPoolExecutor(max_workers=max_workers) as ex:
        futures = {ex.submit(_run_one, s): s for s in scripts}
        results = []
        for future in as_completed(futures):
            results.append(future.result())
    return results
```

Why `ThreadPoolExecutor` not `ProcessPoolExecutor`? Sub-scripts are
**I/O-bound** (grep, file reads, json writes). Process pool's overhead
exceeds the I/O win. Threading + GIL is fine — there's no CPU contention.

Parallelism is bounded by `max_workers` (default 4). The runner respects
explicit per-wave overrides in `.taco-wt/config.json`.

---

## Plugin orchestrator (jinja2 + generators)

ACO has `plan_renderer/` with Jinja2 templates + `plan_plugins/`. TACO-wt
follows the same shape:

```
assets/templates/
  forensic_script.py.j2      # template body for a forensic sub-script
  validate_wave.py.j2        # template body for a wave validator
  conftest.py.j2             # template body for tests/conftest.py
  plan_skeleton.md           # template skeleton for a plan markdown
```

The Jinja2 env in `scaffold_wave.py`:

```python
from jinja2 import Environment, FileSystemLoader, select_autoescape

env = Environment(
    loader=FileSystemLoader(ASSETS_TEMPLATES),
    autoescape=select_autoescape(disabled_extensions=("j2", "tmpl")),
    keep_trailing_newline=True,
    trim_blocks=True,
    lstrip_blocks=True,
)
```

`trim_blocks=True` + `lstrip_blocks=True` prevent L3 (the `textwrap.dedent`
gotcha) by giving full control over template indentation.

---

## Saga pattern (ESAA insights)

ACO has ESAA — Event-Sourced Saga Architecture (CQRS + event store +
hash chain + time travel). TACO-wt does NOT adopt the full saga; it
borrows two ideas:

1. **Hash chain** in TOON checkpoints (already adopted).
2. **Append-only learning JSONL** — every wave failure / success is appended
   to `~/.claude/touring/taco-wt/learning/<plan>.jsonl`. Each line is a
   `WaveOutcome` record with fields `{timestamp, wave, status, score, duration_ms, hallucinated_assumptions[]}`.

The learning JSONL is read by:
- `cross_audit.py` (to flag "wave W11 has failed 3 times in 7 days").
- An optional `learning_analyzer.py` (not part of v1.0, future) for
  hallucination hotspots à la `vgp/learning.py`.

---

## Wiring to Touring memory

Decided in the design session (2026-05-23): the learning store is **hybrid**.

For every wave outcome:

```python
# 1. Append to local JSONL (fast, portable)
outcome = WaveOutcome(...)
learning_path = Path("~/.claude/touring/taco-wt/learning/<plan>.jsonl").expanduser()
learning_path.parent.mkdir(parents=True, exist_ok=True)
with learning_path.open("a", encoding="utf-8") as f:
    f.write(json.dumps(outcome.model_dump(), default=str) + "\n")

# 2. Persist a high-signal lesson in Touring memory (cross-session)
if outcome.status == "PASS" and outcome.lesson:
    subprocess.run([
        "touring", "memory", "store",
        f"lesson:taco-wt:{outcome.plan}:{outcome.wave}",
        outcome.lesson,
        "--tier", "semantic",
    ], check=False)

# 3. RL reward
subprocess.run([
    "touring", "learning", "reward", "orchestrate",
    "1.0" if outcome.status == "PASS" else "-1.0",
    f"wave:{outcome.plan}:{outcome.wave}:{outcome.status}",
], check=False)
```

Daemon-down fallback: each `subprocess.run` is `check=False`. If `touring`
is unavailable, the JSONL still persists. Cross-session learning degrades
gracefully — the local journal is the source of truth, Touring memory is
the cross-session index.
