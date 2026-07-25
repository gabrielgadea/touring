"""TACO-wt lib — shared helpers (Pydantic V2 frozen models + io utilities).

This module is the foundation for the TACO-wt toolkit. It defines:
  * Pydantic V2 frozen models (WaveFinding, WaveSubReport, WaveValidatorReport,
    CrossAuditReport, WaveOutcome) — immutable contract objects.
  * JSON / JSONL helpers — atomic write, append-only, deterministic ordering.
  * Lightweight `touring` CLI wrappers — fail-open (daemon down ≠ script crash).
  * Path discovery — workspace root, plan directory, data/staging/learning paths.

All scripts in `scripts/` import from this module. The Pydantic schema doubles
as documentation of the JSON envelopes the toolkit produces and consumes.
"""

from __future__ import annotations

import json
import logging
import os
import re
import subprocess
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Literal

try:
    from pydantic import BaseModel, ConfigDict, Field
    _PYDANTIC_AVAILABLE = True
except ImportError:  # pragma: no cover — stdlib-only fallback path
    _PYDANTIC_AVAILABLE = False
    BaseModel = object  # type: ignore[assignment,misc]
    Field = None  # type: ignore[assignment]
    ConfigDict = None  # type: ignore[assignment]

logger = logging.getLogger(__name__)

# ── Constants ──────────────────────────────────────────────────────────────

VALID_SEVERITIES: tuple[str, ...] = ("P0", "P1", "P2", "P3")
VALID_STATUSES: tuple[str, ...] = ("PASS", "WARN", "FAIL", "PENDING", "BASELINE", "OK")
VALID_DIMENSIONS: tuple[str, ...] = (
    "precision", "scalability", "performance", "functionality",
    "code_quality", "detail", "integration", "dependencies", "potentiation",
)

LEARNING_ROOT = Path.home() / ".claude" / "touring" / "taco-wt" / "learning"


# ── Pydantic models (when available) ───────────────────────────────────────

if _PYDANTIC_AVAILABLE:

    class WaveFinding(BaseModel):
        """A single forensic finding produced by a sub-script."""

        model_config = ConfigDict(frozen=True)

        file: str = Field(description="Relative path to the affected file")
        line: int = Field(default=1, ge=1, description="1-based line number")
        severity: Literal["P0", "P1", "P2", "P3"] = "P2"
        pattern: str = Field(default="", description="Pattern that matched")
        context: str = Field(default="", description="Lines around the match")
        remediation: str = Field(default="", description="Suggested fix")

    class WaveSubReport(BaseModel):
        """A sub-script JSON envelope (what each forensic script emits)."""

        model_config = ConfigDict(frozen=True)

        script: str = Field(description="Sub-script identifier (no .py)")
        wave: str = Field(description="Wave id, e.g. 'W12'")
        subtask_refs: list[str] = Field(default_factory=list)
        timestamp: str = Field(default="")
        status: Literal["OK", "PASS", "WARN", "FAIL", "PENDING"] = "OK"
        apply: bool = False
        totals: dict[str, int] = Field(default_factory=dict)
        findings: list[dict[str, Any]] = Field(default_factory=list)
        json_path: str = Field(default="")

    class WaveValidatorReport(BaseModel):
        """A validate_W<N>.py output envelope."""

        model_config = ConfigDict(frozen=True)

        status: Literal["PASS", "WARN", "FAIL", "PENDING"] = "PENDING"
        score: float = Field(default=0.0, ge=0.0, le=1.0)
        wave: str = ""
        evidence_files: list[str] = Field(default_factory=list)
        missing_evidence: list[str] = Field(default_factory=list)
        child_results: dict[str, str] = Field(default_factory=dict)
        timestamp: str = ""

    class CrossAuditReport(BaseModel):
        """Top-level cross_audit.py composite report."""

        model_config = ConfigDict(frozen=True)

        plan: str
        mode: Literal["baseline", "normal"] = "normal"
        timestamp: str = ""
        composite_score: float = Field(default=0.0, ge=0.0, le=1.0)
        composite_status: Literal["PASS", "WARN", "FAIL", "BASELINE"] = "BASELINE"
        waves: dict[str, dict[str, Any]] = Field(default_factory=dict)
        missing_evidence: list[str] = Field(default_factory=list)
        summary: dict[str, int] = Field(default_factory=dict)
        recommendations: list[str] = Field(default_factory=list)

    class WaveOutcome(BaseModel):
        """A line in the cross-session learning JSONL."""

        model_config = ConfigDict(frozen=True)

        timestamp: str
        plan: str
        wave: str
        status: str
        score: float = 0.0
        duration_ms: float = 0.0
        lesson: str = ""
        hallucinated_assumptions: list[str] = Field(default_factory=list)

else:  # pragma: no cover

    class _DictModel:
        """Dict-backed stand-in for Pydantic models when pydantic is missing."""

        def __init__(self, **data: Any) -> None:
            for key, value in data.items():
                setattr(self, key, value)

        def model_dump(self) -> dict[str, Any]:
            return {
                k: v for k, v in self.__dict__.items()
                if not k.startswith("_")
            }

    class WaveFinding(_DictModel): ...  # type: ignore[no-redef]
    class WaveSubReport(_DictModel): ...  # type: ignore[no-redef]
    class WaveValidatorReport(_DictModel): ...  # type: ignore[no-redef]
    class CrossAuditReport(_DictModel): ...  # type: ignore[no-redef]
    class WaveOutcome(_DictModel): ...  # type: ignore[no-redef]


# ── IO helpers ─────────────────────────────────────────────────────────────


def utcnow_iso() -> str:
    """ISO 8601 timestamp in UTC, second precision."""
    return datetime.now(UTC).replace(microsecond=0).isoformat()


def write_json_atomic(path: Path, payload: Any, *, indent: int = 2) -> Path:
    """Write JSON atomically via .tmp + os.replace.

    Args:
        path: Destination file.
        payload: JSON-serializable object.
        indent: Indentation passed to json.dump.

    Returns:
        The destination path (same as ``path``).
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(
        json.dumps(payload, indent=indent, ensure_ascii=False, default=str),
        encoding="utf-8",
    )
    os.replace(tmp, path)
    return path


def append_jsonl(path: Path, record: dict[str, Any]) -> None:
    """Append a single JSON line to a JSONL file (creates parent dir)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    line = json.dumps(record, ensure_ascii=False, default=str)
    with path.open("a", encoding="utf-8") as fh:
        fh.write(line + "\n")


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    """Read every line of a JSONL file. Skips invalid lines."""
    if not path.exists() or path.stat().st_size == 0:
        return []
    records: list[dict[str, Any]] = []
    with path.open(encoding="utf-8") as fh:
        for line in fh:
            stripped = line.strip()
            if not stripped:
                continue
            try:
                records.append(json.loads(stripped))
            except json.JSONDecodeError:
                logger.debug("Skipping malformed JSONL line in %s", path)
    return records


def safe_load_json(path: Path) -> dict[str, Any] | None:
    """Read a JSON file. Return None on error (logs at debug)."""
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        logger.debug("Failed to load JSON %s: %s", path, exc)
        return None


# ── Path discovery ────────────────────────────────────────────────────────


def find_plan_dir(plan: str, *, search_from: Path | None = None) -> Path | None:
    """Locate a plan directory by walking up from ``search_from``.

    Looks for ``scripts/<plan>/`` or ``<plan>/`` directories.

    Args:
        plan: Plan identifier (kebab-case).
        search_from: Starting directory. Defaults to cwd.

    Returns:
        Absolute path to the plan directory, or None.
    """
    cur = (search_from or Path.cwd()).resolve()
    while cur != cur.parent:
        candidates = [
            cur / "scripts" / plan,
            cur / plan,
            cur / ".claude" / "plans" / plan,
        ]
        for candidate in candidates:
            if candidate.is_dir():
                return candidate
        cur = cur.parent
    return None


def learning_path(plan: str) -> Path:
    """Path to the cross-session learning JSONL for a plan."""
    return LEARNING_ROOT / f"{plan}.jsonl"


# ── Touring CLI wrappers (fail-open) ──────────────────────────────────────


def touring_available() -> bool:
    """True iff the ``touring`` CLI is on PATH."""
    try:
        result = subprocess.run(
            ["touring", "--version"],
            capture_output=True, text=True, timeout=2, check=False,
        )
        return result.returncode == 0
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False


def touring_memory_store(key: str, value: str, *, tier: str = "semantic") -> bool:
    """Persist a lesson into Touring memory. Fail-open."""
    if not touring_available():
        return False
    try:
        subprocess.run(
            ["touring", "memory", "store", key, value, "--tier", tier],
            timeout=5, check=False, capture_output=True,
        )
        return True
    except (FileNotFoundError, subprocess.TimeoutExpired) as exc:
        logger.debug("touring memory store failed: %s", exc)
        return False


def touring_learning_reward(tool: str, value: float, context: str = "") -> bool:
    """Inject an RL reward into Touring learning system. Fail-open."""
    if not touring_available():
        return False
    try:
        args = ["touring", "learning", "reward", tool, str(value)]
        if context:
            args.append(context)
        subprocess.run(args, timeout=3, check=False, capture_output=True)
        return True
    except (FileNotFoundError, subprocess.TimeoutExpired) as exc:
        logger.debug("touring learning reward failed: %s", exc)
        return False


# ── Regex helpers ─────────────────────────────────────────────────────────


_KEBAB_RE = re.compile(r"^[a-z][a-z0-9-]*[a-z0-9]$")
_WAVE_RE = re.compile(r"^W\d{1,3}(?:\.\d+)?$")
_SUB_NAME_RE = re.compile(r"^[a-z][a-z0-9_]{1,60}$")


def is_kebab(name: str) -> bool:
    """True iff ``name`` is a valid kebab-case identifier (plan id)."""
    return bool(_KEBAB_RE.match(name))


def is_wave_id(name: str) -> bool:
    """True iff ``name`` matches ``W<N>`` or ``W<N>.<M>`` (e.g. 'W12', 'W12.3')."""
    return bool(_WAVE_RE.match(name))


def is_sub_name(name: str) -> bool:
    """True iff ``name`` is a valid sub-script identifier (snake_case)."""
    return bool(_SUB_NAME_RE.match(name))


# ── Exit codes ────────────────────────────────────────────────────────────

EXIT_OK = 0
EXIT_FAIL = 1
EXIT_WARN = 2
EXIT_STRUCTURAL = 3
EXIT_INTERRUPTED = 130


__all__ = [
    # constants
    "VALID_SEVERITIES", "VALID_STATUSES", "VALID_DIMENSIONS", "LEARNING_ROOT",
    "EXIT_OK", "EXIT_FAIL", "EXIT_WARN", "EXIT_STRUCTURAL", "EXIT_INTERRUPTED",
    # models
    "WaveFinding", "WaveSubReport", "WaveValidatorReport",
    "CrossAuditReport", "WaveOutcome",
    # io
    "utcnow_iso", "write_json_atomic", "append_jsonl", "read_jsonl", "safe_load_json",
    # paths
    "find_plan_dir", "learning_path",
    # touring
    "touring_available", "touring_memory_store", "touring_learning_reward",
    # regex
    "is_kebab", "is_wave_id", "is_sub_name",
]
