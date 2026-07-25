# Cross-Audit Protocol

> **Read when**: configuring `cross_audit.py`, designing a `validate_W<N>.py`,
> or interpreting a composite score across waves.
> **Origin**: lesson L4 + adaptation of `pln2_generator/validator.py`.

---

## What the cross-audit produces

```json
{
  "plan": "touring-premium-refactor-2026",
  "mode": "baseline | normal",
  "timestamp": "2026-05-23T15:22:00Z",
  "composite_score": 0.74,
  "composite_status": "BASELINE | PASS | WARN | FAIL",
  "waves": {
    "W01": {"status": "PASS", "score": 0.92, "evidence_files": [...]},
    "W02": {"status": "WARN", "score": 0.73, "evidence_files": [...]},
    "W03": {"status": "PENDING"},
    ...
  },
  "missing_evidence": [...],
  "summary": {
    "total_waves": 15,
    "pass": 4, "warn": 2, "fail": 0, "pending": 9
  },
  "recommendations": [...]
}
```

---

## Status taxonomy

| Status | Meaning | Score | Distinguishes |
|--------|---------|-------|---------------|
| `PASS` | Wave ran, validator returned score ≥ 0.8 | 0.8..1.0 | Healthy |
| `WARN` | Wave ran, validator returned 0.5 ≤ score < 0.8 | 0.5..0.79 | Needs attention |
| `FAIL` | Wave ran, validator returned score < 0.5 | 0.0..0.49 | Real failure |
| `PENDING` | Wave not yet executed (no `data/W<N>-*.json`) | — | NOT a failure |

The distinction `PENDING` vs `FAIL` is the heart of L4 — without it, a plan
at the start of life is indistinguishable from a plan in catastrophe.

---

## Two modes

### `--baseline`

```bash
python3 cross_audit.py --plan <plan> --baseline
```

- Used **before any wave runs**, or to checkpoint a plan that is still partially un-executed.
- PENDING waves are **excluded** from the composite-score average.
- If 100% of waves are PENDING → `composite_status = BASELINE` (exit 0).
- Anything PASS/WARN/FAIL is averaged among themselves.
- Idea: `--baseline` is "tell me about the parts that ran".

### `normal` (default)

```bash
python3 cross_audit.py --plan <plan>
```

- PENDING waves are counted as **score = 0.0** in the composite average.
- A plan with 9 PENDING + 4 PASS will score low — and that is the point;
  it tells the operator the plan is incomplete.
- `composite_status` is derived from the average:
  - `≥ 0.8` → `PASS`
  - `≥ 0.5` → `WARN`
  - `< 0.5` → `FAIL`

---

## How `validate_W<N>.py` reports its state

Every wave validator returns a dict with this shape — read by `cross_audit.py`:

```python
def run(args: argparse.Namespace) -> dict:
    """Validator for W<N>. Read-only — --apply is a no-op."""
    wave_data_glob = _DATA_DIR / "W<N>-*.json"
    sub_reports = [json.loads(p.read_text()) for p in wave_data_glob.glob("*.json")]

    if not sub_reports:
        return {"status": "PENDING", "wave": _WAVE, "missing_evidence": [...]}

    # ... per-sub gates ...
    score = _compute_score(sub_reports)
    status = _status_from_score(score)

    return {
        "status": status,                  # PASS / WARN / FAIL
        "score": score,                    # 0.0 .. 1.0
        "wave": _WAVE,
        "evidence_files": [str(p) for p in wave_data_glob.glob("*.json")],
        "missing_evidence": [],            # explicit list when sub-script JSON expected but missing
        "child_results": {sub_name: status for ...},
    }
```

The validator does NOT mutate. It runs read-only against `data/`.

---

## How scores combine

The composite is a weighted average:

```python
def composite_score(wave_results: dict[str, dict]) -> float:
    counted = []
    for wave_id, result in wave_results.items():
        status = result.get("status")
        if mode == "baseline" and status == "PENDING":
            continue          # baseline: skip PENDING
        if status == "PENDING":
            counted.append(0.0)   # normal: PENDING is 0.0
        else:
            counted.append(float(result.get("score", 0.0)))
    if not counted:
        return 0.0
    return sum(counted) / len(counted)
```

Weights can be added per-wave via plan frontmatter `wave_weights:`:

```yaml
---
plan: touring-premium-refactor-2026
wave_weights:
  W01: 1.0
  W02: 1.0
  W11: 2.0   # Test Debt Repayment double-counts
  W15: 0.5   # Documentation wave half-counts
---
```

The aggregator reads weights, defaults to 1.0 when absent.

---

## Evidence completeness

`evidence_collector.py --strict` runs alongside cross_audit and asserts:

| Check | Pass criterion |
|-------|----------------|
| All declared sub-scripts produced JSON in `data/` | 0 missing |
| Every JSON is valid (deserializes) | 0 errors |
| Every JSON has the standard envelope (`script`, `wave`, `timestamp`, `status`, ...) | 0 violations |
| Every `evidence_files` reference in validator output exists on disk | 0 dangling |

A wave can be `PASS` per validator but `MISSING_EVIDENCE` per collector —
they are independent gates. Both must clear for merge.

---

## Recommendations engine

`cross_audit.py` emits human-readable recommendations:

```json
"recommendations": [
  "W02: 3 sub-scripts WARN. Re-run with -v to inspect findings.",
  "W11: PENDING. Per L6, re-measure premises before scaffolding.",
  "W14: composite score 0.45 (FAIL). Check data/W14-apply.json for unrecovered errors."
]
```

Recommendations are derived from:
- Validator output (status, missing_evidence)
- Sub-script `findings[].severity` rollups
- Lessons triggered (e.g., L6 re-measure when wave_age_days > 14)

---

## Exit code contract

| Exit | Meaning |
|------|---------|
| `0` | composite `PASS` or `BASELINE` |
| `1` | composite `WARN` |
| `2` | composite `FAIL` |
| `3` | structural error (cannot find plan dir, JSON unreadable, ...) |
| `130` | KeyboardInterrupt |

CI uses exit code; humans read JSON.
