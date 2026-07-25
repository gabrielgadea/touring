"""taco-planning lib — Pydantic V2 frozen models + helpers for authoring Pln2 plans.

Imported by all 9 other scripts. Defines:
  * Pydantic models (ConfidenceTag, GroundTruth, PlanDimension, Phase, Subtask, ...)
  * IO helpers (atomic JSON write, JSONL append, safe load)
  * Touring CLI wrappers (fail-open)
  * Intent extraction heuristics (PascalCase / snake_case / paths)
  * Hash + cache utilities
  * Regex validators

Specialized for AUTHORING (vs operation) — pairs with TACO-wt for execution.
"""

from __future__ import annotations

import hashlib
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
except ImportError:  # pragma: no cover
    _PYDANTIC_AVAILABLE = False
    BaseModel = object  # type: ignore[assignment,misc]
    Field = None  # type: ignore[assignment]
    ConfigDict = None  # type: ignore[assignment]

logger = logging.getLogger(__name__)

# ── Constants ──────────────────────────────────────────────────────────────

VALID_DIMENSIONS: tuple[str, ...] = (
    "precision", "scalability", "performance", "functionality", "quality",
    "detail", "integration", "dependencies", "potentiation",
)
VALID_CONFIDENCE: tuple[str, ...] = ("FACT", "INFERENCE", "SPECULATION")
VALID_SEVERITIES: tuple[str, ...] = ("P0", "P1", "P2", "P3")
VALID_PLAN_LEVELS: tuple[str, ...] = ("L0", "L1", "L2", "L3", "L4", "L5")
VALID_PHASE_MODES: tuple[str, ...] = ("parallel", "sequential")

LEARNING_ROOT = Path.home() / ".claude" / "touring" / "taco-planning" / "learning"
CACHE_ROOT = Path.home() / ".claude" / "touring" / "taco-planning" / "cache"
DEFAULT_CACHE_TTL_SECONDS = 600

# ── Pydantic models ───────────────────────────────────────────────────────


if _PYDANTIC_AVAILABLE:

    class ConfidenceTag(BaseModel):
        """Confidence tag attached to a single claim in a plan."""

        model_config = ConfigDict(frozen=True)

        level: Literal["FACT", "INFERENCE", "SPECULATION"] = "INFERENCE"
        score: float = Field(default=0.8, ge=0.0, le=1.0)
        evidence_command: str = Field(default="",
                                       description="Touring command that produced the evidence")
        evidence_excerpt: str = Field(default="", description="Quoted output excerpt")
        rationale: str = Field(default="")

    class VerifiedSymbol(BaseModel):
        """A symbol verified via `touring index find`."""

        model_config = ConfigDict(frozen=True)

        name: str
        verified: bool = False
        file: str = ""
        line: int = 0
        signature: str = ""
        suggestion: str = Field(default="", description="Closest match if not verified")

    class GroundTruth(BaseModel):
        """Stage-1 unified Touring sweep result."""

        model_config = ConfigDict(frozen=True)

        timestamp: str
        intent: str
        duration_ms: float = 0.0
        daemon_degraded: bool = False
        doctor: dict[str, Any] = Field(default_factory=dict)
        status_snapshot: dict[str, Any] = Field(default_factory=dict)
        e2e: dict[str, Any] = Field(default_factory=dict)
        wiring_audit: dict[str, Any] = Field(default_factory=dict)
        wiring_orphans: list[dict[str, Any]] = Field(default_factory=list)
        evolution_drift: dict[str, Any] = Field(default_factory=dict)
        memory_lessons: list[dict[str, Any]] = Field(default_factory=list)
        gotcha_per_file: dict[str, list[dict[str, Any]]] = Field(default_factory=dict)
        vgp_verifications: list[VerifiedSymbol] = Field(default_factory=list)
        ast_overviews: dict[str, Any] = Field(default_factory=dict)
        ast_blasts: dict[str, Any] = Field(default_factory=dict)
        summary: dict[str, int] = Field(default_factory=dict)

    class PlanDimension(BaseModel):
        """Score envelope for one of the 9 canonical dimensions."""

        model_config = ConfigDict(frozen=True)

        name: Literal[
            "precision", "scalability", "performance", "functionality", "quality",
            "detail", "integration", "dependencies", "potentiation",
        ]
        current: float = Field(default=0.0, ge=0.0, le=10.0)
        target: float = Field(default=0.0, ge=0.0, le=10.0)
        delta: float = 0.0
        hits: int = 0
        density: float = 0.0
        evidence: list[str] = Field(default_factory=list)
        recommendations: list[str] = Field(default_factory=list)
        amplification: str = ""
        # extras specific to authoring
        symbol_verifications_ok: int = 0
        symbol_verifications_total: int = 0
        schema_completeness: float = Field(default=0.0, ge=0.0, le=1.0,
                                            description="Fraction of APIs with embedded schemas")

    class Subtask(BaseModel):
        """A single subtask declared inside a phase."""

        model_config = ConfigDict(frozen=True)

        id: str
        action: str
        severity: Literal["P0", "P1", "P2", "P3"] = "P1"
        confidence: Literal["FACT", "INFERENCE", "SPECULATION"] = "INFERENCE"
        file: str = ""
        line: int = 0
        lang: str = "rust"
        source_truth: str = ""
        change: str = ""
        blast_radius: int = 0
        test_name: str = ""
        test_assertion: str = ""
        dimensions_impact: list[str] = Field(default_factory=list)
        enables: str = Field(default="", description="REGRA #0 — empty disallowed")
        evidence_cmd: str = ""

    class Phase(BaseModel):
        """A plan phase composed of subtasks."""

        model_config = ConfigDict(frozen=True)

        number: int = Field(ge=1)
        name: str
        mode: Literal["parallel", "sequential"] = "sequential"
        depends_on: list[int] = Field(default_factory=list)
        subtasks: list[Subtask] = Field(default_factory=list)
        objective: str = ""
        engineer_days: float = 0.0

    class PlanReport(BaseModel):
        """Top-level report envelope."""

        model_config = ConfigDict(frozen=True)

        status: str = "OK"
        script: str = ""
        timestamp: str = ""
        source: str = ""
        composite_current: float = 0.0
        composite_target: float = 0.0
        composite_delta: float = 0.0
        dimensions: list[PlanDimension] = Field(default_factory=list)

else:  # pragma: no cover — stdlib fallback

    class _DictModel:
        def __init__(self, **data: Any) -> None:
            for key, value in data.items():
                setattr(self, key, value)

        def model_dump(self) -> dict[str, Any]:
            return {k: v for k, v in self.__dict__.items() if not k.startswith("_")}

    class ConfidenceTag(_DictModel): ...  # type: ignore[no-redef]
    class VerifiedSymbol(_DictModel): ...  # type: ignore[no-redef]
    class GroundTruth(_DictModel): ...  # type: ignore[no-redef]
    class PlanDimension(_DictModel): ...  # type: ignore[no-redef]
    class Subtask(_DictModel): ...  # type: ignore[no-redef]
    class Phase(_DictModel): ...  # type: ignore[no-redef]
    class PlanReport(_DictModel): ...  # type: ignore[no-redef]


# ── IO helpers ─────────────────────────────────────────────────────────────


def utcnow_iso() -> str:
    """ISO 8601 UTC second-precision timestamp."""
    return datetime.now(UTC).replace(microsecond=0).isoformat()


def write_json_atomic(path: Path, payload: Any, *, indent: int = 2) -> Path:
    """Atomic JSON write via .tmp + os.replace."""
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(
        json.dumps(payload, indent=indent, ensure_ascii=False, default=str),
        encoding="utf-8",
    )
    os.replace(tmp, path)
    return path


def append_jsonl(path: Path, record: dict[str, Any]) -> None:
    """Append a single JSON line."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(record, ensure_ascii=False, default=str) + "\n")


def safe_load_json(path: Path) -> dict[str, Any] | None:
    """Load JSON file; return None on error."""
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        logger.debug("Failed to load JSON %s: %s", path, exc)
        return None


# ── Touring CLI wrappers (fail-open) ──────────────────────────────────────


def touring_available() -> bool:
    """True iff `touring --version` returns 0."""
    try:
        return subprocess.run(
            ["touring", "--version"],
            capture_output=True, text=True, timeout=2, check=False,
        ).returncode == 0
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False


def run_touring(args: list[str], *, timeout: int = 30) -> dict[str, Any] | None:
    """Invoke a touring command expecting JSON output. None on any error."""
    if not touring_available():
        return None
    try:
        result = subprocess.run(
            ["touring", *args],
            capture_output=True, text=True, timeout=timeout, check=False,
        )
        if result.returncode != 0 and not result.stdout:
            return None
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError:
            return None
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return None


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
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False


def touring_learning_reward(tool: str, value: float, context: str = "") -> bool:
    """Inject RL reward. Fail-open."""
    if not touring_available():
        return False
    try:
        args = ["touring", "learning", "reward", tool, str(value)]
        if context:
            args.append(context)
        subprocess.run(args, timeout=3, check=False, capture_output=True)
        return True
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False


# ── Intent extraction heuristics ──────────────────────────────────────────


_PASCAL_CASE_RE = re.compile(r"\b([A-Z][a-z0-9]+){2,}\b")
_SNAKE_CASE_RE = re.compile(r"\b[a-z][a-z0-9]+(?:_[a-z0-9]+){1,}\b")
_KEBAB_RE = re.compile(r"\b[a-z]+(?:-[a-z]+){1,}\b")
_PATH_RE = re.compile(r"\b[a-z_][a-z_0-9/]*\.(rs|py|ts|tsx|js|go|toml|yaml|json|md)\b")


def extract_symbols_from_intent(intent: str) -> list[str]:
    """Heuristic symbol extraction (PascalCase, snake_case) from a free-form intent.

    Returns deduplicated list, max 10 candidates (keep VGP cost bounded).
    """
    candidates: set[str] = set()
    for match in _PASCAL_CASE_RE.finditer(intent):
        candidates.add(match.group(0))
    for match in _SNAKE_CASE_RE.finditer(intent):
        candidates.add(match.group(0))
    return sorted(candidates)[:10]


def extract_paths_from_intent(intent: str) -> list[str]:
    """Extract file paths from a free-form intent."""
    return sorted({m.group(0) for m in _PATH_RE.finditer(intent)})[:10]


def extract_kebab_terms(intent: str) -> list[str]:
    """Extract kebab-case terms (often feature flags or concept tags)."""
    return sorted({m.group(0) for m in _KEBAB_RE.finditer(intent)})[:10]


# ── Hash + cache ──────────────────────────────────────────────────────────


def compute_intent_cache_key(intent: str, *, extra: str = "") -> str:
    """blake2b-256 hex of canonicalized intent + extra (e.g. repo head sha)."""
    canonical = intent.strip().lower() + "\x1f" + extra.strip()
    return hashlib.blake2b(canonical.encode("utf-8"), digest_size=16).hexdigest()


def cache_get(key: str, ttl_seconds: int = DEFAULT_CACHE_TTL_SECONDS) -> dict[str, Any] | None:
    """Read cache entry by key. Return None on miss / expired / corrupt."""
    path = CACHE_ROOT / f"{key}.json"
    if not path.exists():
        return None
    try:
        loaded = json.loads(path.read_text(encoding="utf-8"))
        cached_at = float(loaded.get("__cached_at__", 0))
        if (datetime.now(UTC).timestamp() - cached_at) > ttl_seconds:
            return None
        return loaded.get("payload")
    except (OSError, json.JSONDecodeError, ValueError):
        return None


def cache_put(key: str, payload: dict[str, Any]) -> Path:
    """Write cache entry. Returns the file path."""
    path = CACHE_ROOT / f"{key}.json"
    envelope = {
        "__cached_at__": datetime.now(UTC).timestamp(),
        "payload": payload,
    }
    return write_json_atomic(path, envelope)


# ── Regex validators ─────────────────────────────────────────────────────


_KEBAB_VALID = re.compile(r"^[a-z][a-z0-9-]*[a-z0-9]$")
_PLAN_LEVEL_VALID = re.compile(r"^L[0-5]$")


def is_kebab(name: str) -> bool:
    return bool(_KEBAB_VALID.match(name))


def is_plan_level(value: str) -> bool:
    return bool(_PLAN_LEVEL_VALID.match(value))


# ── Exit codes ────────────────────────────────────────────────────────────

EXIT_OK = 0
EXIT_FAIL = 1
EXIT_WARN = 2
EXIT_STRUCTURAL = 3
EXIT_INTERRUPTED = 130


__all__ = [
    # constants
    "VALID_DIMENSIONS", "VALID_CONFIDENCE", "VALID_SEVERITIES",
    "VALID_PLAN_LEVELS", "VALID_PHASE_MODES",
    "LEARNING_ROOT", "CACHE_ROOT", "DEFAULT_CACHE_TTL_SECONDS",
    "EXIT_OK", "EXIT_FAIL", "EXIT_WARN", "EXIT_STRUCTURAL", "EXIT_INTERRUPTED",
    # models
    "ConfidenceTag", "VerifiedSymbol", "GroundTruth", "PlanDimension",
    "Subtask", "Phase", "PlanReport",
    # io
    "utcnow_iso", "write_json_atomic", "append_jsonl", "safe_load_json",
    # touring
    "touring_available", "run_touring", "touring_memory_store",
    "touring_learning_reward",
    # intent extraction
    "extract_symbols_from_intent", "extract_paths_from_intent", "extract_kebab_terms",
    # cache
    "compute_intent_cache_key", "cache_get", "cache_put",
    # validators
    "is_kebab", "is_plan_level",
]
