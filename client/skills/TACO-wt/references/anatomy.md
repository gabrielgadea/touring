# Anatomy of a Forensic Sub-Script

> **Read when**: authoring a new sub-script by hand (instead of `scaffold_wave.py`),
> or modifying an existing one and wanting to keep the 4-phase shape intact.

## Why a fixed shape?

The 4-phase anatomy makes sub-scripts **interchangeable** under the runner:
`forensic_runner.py` calls `run()`, captures `result["status"]` and the JSON
report path; `evidence_collector.py` reads the same JSON; `validate_W<N>.py`
aggregates them. Drift away from the shape, the toolkit silently degrades.

---

## The 4 phases

### Phase 1 — Imports + constants

```python
from __future__ import annotations
import argparse, json, logging, sys
from datetime import UTC, datetime
from pathlib import Path

_ROOT = Path(__file__).resolve().parents[2]
_DATA_DIR = _ROOT / "scripts" / "<plan-dir>" / "data"
_STAGING_DIR = _ROOT / "scripts" / "<plan-dir>" / "staging"
_WAVE = "W<N>"
_NAME = "<sub_name>"

_EXIT_OK = 0
_EXIT_FAIL = 1
_EXIT_INTERRUPTED = 130
```

Constants are top-level so tests can monkey-patch them. `_ROOT` walks up from
the script file, so sub-scripts are runnable from any cwd.

### Phase 2 — CLI parser

```python
def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog=_NAME, description=__doc__)
    p.add_argument("--apply", action="store_true",
                   help="DESTRUCTIVE — actually perform changes.")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-j", "--json", action="store_true",
                   help="Emit only JSON to stdout (machine-readable).")
    p.add_argument("-v", "--verbose", action="store_true")
    return p
```

Universal flags every sub-script accepts (L9: dry-run is the default).

### Phase 3 — Pure scan

```python
def scan_X(workspace: Path) -> list[dict]:
    """Read-only: scan workspace for the X pattern, return findings."""
    findings: list[dict] = []
    for path in workspace.rglob("*.rs"):
        # ... regex / AST analysis ...
        findings.append({"file": str(path.relative_to(workspace)), ...})
    return findings
```

**Pure** means no `Path.write_text`, no `subprocess` that mutates, no env writes.
Only reads. Easy to test, parallelizable, idempotent.

### Phase 4 — Optional mutation (gated by `--apply`)

```python
def apply_changes(findings: list[dict], workspace: Path) -> dict:
    """DESTRUCTIVE — runs only with --apply. Returns mutation summary."""
    applied = 0
    skipped = 0
    for f in findings:
        # ... edit, rename, move ...
        applied += 1
    return {"applied": applied, "skipped": skipped}
```

Default: this never runs. Mutations require explicit `--apply`.

---

## The `run()` orchestrator

```python
def run(args: argparse.Namespace) -> dict:
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)

    findings = scan_X(_ROOT)
    applied = apply_changes(findings, _ROOT) if args.apply else {}

    report = {
        "script": _NAME,
        "wave": _WAVE,
        "subtask_refs": [f"{_WAVE}.{_NAME}"],
        "timestamp": datetime.now(UTC).isoformat(),
        "status": "OK",
        "apply": args.apply,
        "totals": {"findings": len(findings), **applied},
        "findings": findings,
    }

    json_path = args.output_dir / f"{_WAVE}-{_NAME}.json"
    json_path.write_text(
        json.dumps(report, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )

    return {**report, "json_path": str(json_path.relative_to(_ROOT))}
```

---

## The `main()` entry point

```python
def main() -> int:
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(
            json.dumps(result, indent=2, ensure_ascii=False) + "\n"
        )
        return _EXIT_OK if result["status"] == "OK" else _EXIT_FAIL
    except KeyboardInterrupt:
        return _EXIT_INTERRUPTED
    except Exception:  # noqa: BLE001 — top-level catch-all is intentional
        logging.getLogger(__name__).exception("error")
        return _EXIT_FAIL

if __name__ == "__main__":
    raise SystemExit(main())
```

---

## The wave validator pattern

`validate_W<N>.py` is itself a sub-script — same anatomy. Its `run()` reads
`data/W<N>-*.json` and emits:

```python
{
    "status": "PASS|WARN|FAIL|PENDING",  # PENDING = not yet executed
    "score": 0.0,                         # 0.0..1.0
    "evidence_files": ["data/W<N>-foo.json", ...],
    "wave": "W<N>",
    "missing_evidence": [],
    "child_results": {...},               # per-sub status
}
```

The validator does NOT mutate. It always runs read-only — `--apply` is a no-op
for validators by convention.

---

## Symmetry with the toolkit

| Sub-script function | Read by which TACO-wt script |
|---------------------|------------------------------|
| `scan_X()` returns findings list | `evidence_collector.py` (counts) |
| `run()` returns report dict | `forensic_runner.py` (parallel aggregation) |
| `data/W<N>-*.json` artifact | `cross_audit.py` (rollup), `evidence_collector.py` |
| `validate_W<N>.py` returns `{status, score, ...}` | `cross_audit.py` (composite score) |
| `_WAVE`, `_NAME` constants | `scaffold_wave.py` (filled by template) |

Keep the names. Keep the shape. The toolkit assumes it.
