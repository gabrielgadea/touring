---
name: TACO-wt
description: TACO Wave Template — scaffolding, execution, validation and cross-audit of multi-wave plans via deterministic Python scripts. Wraps the proven sub-script pattern (origin Touring Premium Refactor 2026-05-11, 55 scripts / 164 tests) with a code-first toolkit: scaffold_wave + forensic_runner (parallel) + cross_audit (--baseline) + dimension_scorer (9 dims) + gap_detector (P0-P3) + plan_validator (Kahn-sorted deps) + evidence_collector + toon_checkpoint + learning JSONL. Use when authoring or executing a multi-wave plan (8+ waves, 100+ engineer-days), a cross-crate migration with ≥10 consumers, or an architecture fusion/split. Triggers — "criar wave forensic", "novo plano multi-wave", "scaffold wave", "validate W<N>", "cross audit baseline", "per-wave forensic", "wave template", "forense por wave", "auditoria por wave", "plano multi-wave". Replaces the prose-only forensic-per-wave-template.
---

# TACO-wt — TACO Wave Template

> **Identity**: `TACO-wt` = "Wave Template" tooling for the TACO orchestrator.
> **Origin**: Touring Premium Refactor 2026-05-11 (55 sub-scripts, 164 pytest tests)
> distilled into a reusable, code-first toolkit.
> **Layer 3 honored**: every recurring step is a Python script in `scripts/`, never prose.

---

## Quando aplicar

| Cenário | Aplicar? |
|---|---|
| Plano multi-wave (8+ waves, 100+ engineer-days) | ✅ obrigatório |
| Refactor single-crate (<5 dias) | ❌ overkill |
| Migration cross-crate com ≥10 consumers | ✅ obrigatório |
| Feature single-PR | ❌ direto ao código |
| Architecture change (fusion/split) | ✅ medir cycles + LOC distribution |
| Plan with PENDING/IN-PROGRESS/DONE waves needing rollup score | ✅ cross_audit `--baseline` |
| ANTT process analysis pipeline (F1-F16, packages/) | ✅ adapt scaffold_wave |

**Princípio**: medir desbloqueia decisão de escopo. Cada hora de script forense
economiza 10× em retrabalho.

---

## Quick-start em 4 comandos

```bash
# 1. Scaffold a new wave from the canonical templates
python3 scaffold_wave.py --plan touring-premium-refactor-2026 \
                        --wave W12 \
                        --title "Test Debt Repayment" \
                        --sub-scripts 3 \
                        --with-tests

# 2. Run all sub-scripts of a wave in parallel (--apply omitted = dry-run)
python3 forensic_runner.py --plan touring-premium-refactor-2026 --wave W12 -j

# 3. Cross-audit the whole plan
python3 cross_audit.py --plan touring-premium-refactor-2026 --baseline   # pre-execution
python3 cross_audit.py --plan touring-premium-refactor-2026              # post-execution score

# 4. Persist the wave checkpoint (TOON v1.0 + blake2b hash chain)
python3 toon_checkpoint.py emit --phase "W12-complete" --data data/W12-aggregate.json
```

All scripts emit JSON to stdout and write artifacts to `data/`, `staging/`, and
`~/.claude/touring/taco-wt/learning/` (the cross-session learning store).

---

## Architectural map

```
┌─────────────────────────────────────────────────────────────────────────┐
│  TACO-wt scripts/   (Layer 3 — the leverage)                            │
├─────────────────────────────────────────────────────────────────────────┤
│  scaffold_wave.py       —  generate sub-scripts + tests + validator     │
│  forensic_runner.py     —  ThreadPoolExecutor parallel execution        │
│  cross_audit.py         —  aggregate validate_W*.py → composite score   │
│  dimension_scorer.py    —  9-dimension keyword-density grading          │
│  gap_detector.py        —  P0-P3 gaps + remediation generator           │
│  evidence_collector.py  —  data/*.json integrity + completeness check   │
│  plan_validator.py      —  frontmatter + Kahn-sorted wave-deps          │
│  toon_checkpoint.py     —  TOON v1.0 emit/load + blake2b hash chain     │
│  lib.py                 —  Pydantic V2 frozen models + helpers          │
└─────────────────────────────────────────────────────────────────────────┘
                          │  produces / reads
                          ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  Per-plan directory: scripts/<plan>/                                    │
├─────────────────────────────────────────────────────────────────────────┤
│  W<N>/<sub>.py         (forensic sub-script — Phase 1-4 anatomy)        │
│  W<N>/validate_W<N>.py (per-wave validator → {status, score, evidence}) │
│  W<N>/tests/test_*.py  (pytest, 5-15 cases / sub-script)                │
│  data/W<N>-*.json      (machine-readable artifacts)                     │
│  staging/W<N>-*.md     (human-readable narrative, optional)             │
│  cross_audit.json      (composite — output of cross_audit.py)           │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 4-Phase anatomy of a sub-script

Every forensic sub-script has the same shape — see
[references/anatomy.md](references/anatomy.md) for the canonical template:

| Phase | Responsibility | Side effects? |
|-------|----------------|---------------|
| 1. Imports + constants | path roots, dirs, exit codes | none |
| 2. CLI parser | `--apply`, `--output-dir`, `-v`, `-j` | none |
| 3. Pure scan | `scan_X() → list[Finding]` | read-only |
| 4. Optional mutation | `apply_changes(findings) → dict` | gated by `--apply` |

A wave validator (`validate_W<N>.py`) reads `data/W<N>-*.json` and emits
`{status, score, evidence_files}` — see [references/cross-audit-protocol.md](references/cross-audit-protocol.md).

---

## Lessons L1-L10

Full lessons in [references/lessons.md](references/lessons.md). One-line digest:

| # | Lesson | Where it bit |
|---|--------|--------------|
| L1 | Iterate v1→v5 with single hypothesis per version | W8 4-iteration shared-bucket fix |
| L2 | "shared types" bucket MUST be leaf (no outgoing `crate::` deps) | W8 leaf invariant |
| L3 | `textwrap.dedent` needs uniform leading whitespace | scaffold templates |
| L4 | Cross-audit `--baseline` mode distinguishes PENDING vs FAIL | rollup score realism |
| L5 | Forensic discovery first, refactor second | every wave |
| L6 | Re-measure premises before each wave (W6, W3.2, W10) | stale plan assumptions |
| L7 | Daemon `(deleted)` after binary rebuild → restart required | toolchain hygiene |
| L8 | A validation script is itself a sub-script (same anatomy) | symmetry, no special-casing |
| L9 | `--apply` ALWAYS opt-in; default is dry-run | irreversible mutations |
| L10 | JSON output is the contract — humans get markdown via `--md` | machine-readable first |

---

## Quality gates (every wave passes these before merge)

| Gate | Tool | Pass criterion |
|------|------|----------------|
| Sub-scripts compile | `python3 -m py_compile` | exit 0 |
| Sub-scripts type-clean | `pyright --outputjson` (advisory) | 0 errors |
| Sub-scripts lint-clean | `ruff check` (advisory) | 0 errors |
| Sub-scripts pass tests | `pytest -x W<N>/tests/` | all green |
| Wave validator returns PASS | `python3 validate_W<N>.py` | `status=PASS`, `score≥0.8` |
| Cross-audit normal-mode | `cross_audit.py` | composite `≥0.8` |
| Evidence completeness | `evidence_collector.py --strict` | 0 missing |
| TOON checkpoint emitted | `toon_checkpoint.py emit` | hash chain valid |

---

## Test suite pattern

Each sub-script has a sibling `tests/test_<sub>.py`. Conftest pattern:

```python
import sys
from pathlib import Path
_SCRIPTS_DIR = Path(__file__).resolve().parent.parent
if str(_SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPTS_DIR))

import pytest

@pytest.fixture
def mock_workspace(tmp_path: Path) -> Path:
    """Build minimal mock workspace with crates/, Cargo.toml stub."""
    (tmp_path / "crates" / "demo").mkdir(parents=True)
    (tmp_path / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["crates/demo"]\n'
    )
    return tmp_path
```

5-15 tests / sub-script across `TestRegexes`, `TestParseX`, `TestEmit`, `TestRun`.

---

## Cross-references map

| Topic | File |
|-------|------|
| Anatomy of a sub-script + canonical template | [references/anatomy.md](references/anatomy.md) |
| Lessons L1-L10 with full narrative | [references/lessons.md](references/lessons.md) |
| 6-stage pipeline pattern (parse → analyze → detect → generate → validate → checkpoint) | [references/pipeline-patterns.md](references/pipeline-patterns.md) |
| Cross-audit protocol (baseline mode, scoring, status taxonomy) | [references/cross-audit-protocol.md](references/cross-audit-protocol.md) |
| Orchestration patterns (7 phases, pre/post hooks, TACOState, refinement loop) | [references/orchestration-patterns.md](references/orchestration-patterns.md) |
| Jinja2 templates | `assets/templates/*.j2` |
| Plan markdown skeleton | `assets/templates/plan_skeleton.md` |

---

## Insights sources

This skill distills patterns from three sister projects:

| Project | Pattern adopted |
|---------|----------------|
| `analise/scripts/pln2_generator` | 6-stage pipeline, Pydantic V2 frozen models, 9 quality dimensions, P0-P3 gaps, blake2b hash chain, regex-only NLP, TOON checkpointing |
| `analise/scripts/vgp` | Topological sort (Kahn) for wave ordering, cycle detection (DFS back-edges), shadow lint testing (ruff in tempfile), learning JSONL (hallucination hotspots, effectiveness report), risk-weighted impact (code 1.0 + config 0.5 + docs 0.1) |
| `analise/scripts/aco` | Pre/post phase hooks, TACOState (process_id + phase_id + error_history), refinement loop max=3, parallel generator execution, UnifiedCheckpointManager, Jinja2 templates |

---

## Hard rules

1. **Layer 3 over prose.** Every recurring step must be a script in `scripts/`.
2. **Code analyses, the model synthesises.** No LLM in the critical path of a sub-script — regex / cargo / grep / ripgrep only.
3. **Dry-run by default.** Mutations require explicit `--apply`. L9 is non-negotiable.
4. **JSON is the contract.** Machine-readable first; humans get markdown via `--md` flag.
5. **Validators are sub-scripts too.** Same anatomy, same 4 phases.
6. **`--baseline` before execution.** Cross-audit baseline run is mandatory before the first sub-script runs.
7. **Persist + reward.** After every successful wave, `touring memory store --tier semantic` the lesson AND `touring learning reward orchestrate +1.0`.

---

## Authority and renaming history

| Date | Change |
|------|--------|
| 2026-05-11 | Original skill `forensic-per-wave-template` authored from Touring Premium Refactor experience (prose-only, ~175L). |
| 2026-05-23 | Renamed `forensic-per-wave-template` → `TACO-wt`. Added 9 scripts (~2 000 LOC), 5 references, 4 assets/templates. Distilled insights from `pln2_generator`, `vgp`, `aco`. Old skill deleted. |
