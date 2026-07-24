"""Domain dataclasses for the touring-premium-refactor-2026 plan.

Extracted from generate_plan.py:65-167. Eight dataclasses describe the plan's
entities (Subtask, Wave, CrateTarget, CrateCurrent, Tier, Risk, Kpi).
"""
from __future__ import annotations

from dataclasses import dataclass, field

# ─── Dataclasses ─────────────────────────────────────────────────────────────


@dataclass
class Subtask:
    """A single sub-task inside a Wave."""
    id: str                              # e.g. "W0.1"
    name: str
    description: str
    discover: list[str] = field(default_factory=list)
    tdd_red: str = ""
    validation: str = ""
    days: float = 0.5
    blocking: bool = False               # If True, blocks all subsequent subtasks


@dataclass
class Wave:
    """A wave in the refactor (W0..W14)."""
    id: str                              # e.g. "W0"
    name: str
    phase: str                           # F1/F2/F3/... grouping
    depends_on: list[str]
    parallel_with: list[str] = field(default_factory=list)
    cila: str = "L3"
    rust_changes: str = "MIXED"          # ZERO | ADITIVE | MIXED | FUSION | SPLIT
    days_min: int = 5
    days_max: int = 7
    description: str = ""
    contribution: str = ""
    effects: list[str] = field(default_factory=list)
    subtasks: list[Subtask] = field(default_factory=list)
    gate: str = ""                       # Exit-gate criteria
    risks: list[str] = field(default_factory=list)


@dataclass
class CrateTarget:
    """A target crate in the new 13-crate topology."""
    name: str
    layer: int                           # 1..6
    modules: list[str]
    public_api: list[str]
    features: list[str]
    internal_deps: list[str]
    loc_src_target: int
    loc_test_target: int
    pub_target: int
    msrv: str = "1.83"
    notes: str = ""
    absorves: list[str] = field(default_factory=list)


@dataclass
class CrateCurrent:
    """A current crate (one of the 46) with its disposition in the refactor."""
    name: str
    loc_src: int
    loc_test: int
    pub_count: int
    file_count: int
    disposition: str                     # KEEP | FUSE_INTO_X | SPLIT_INTO_X | DELETE
    target: str = ""                     # Target crate name (if FUSE_INTO_X)
    notes: str = ""


@dataclass
class Tier:
    """A commercial tier."""
    name: str                            # "Free" | "Standard" | "Premium" | "Enterprise"
    price_monthly: str                   # "$0" | "$29/mo" | ...
    price_annual: str
    target: str
    features: list[str]                  # Cargo feature names
    telemetry: str                       # "ON" | "OFF default" | "OFF + audit"
    support_sla: str
    license: str


@dataclass
class Risk:
    """A risk in the risk register."""
    id: str                              # "R1", "R2", ...
    wave: str                            # Wave it applies to (or "ALL")
    description: str
    probability: str                     # "LOW" | "MEDIUM" | "HIGH"
    impact: str                          # "LOW" | "MEDIUM" | "HIGH" | "CATASTROPHIC"
    mitigation: str


@dataclass
class Kpi:
    """A KPI tracked across horizons."""
    name: str
    t0: str
    m3: str
    m6: str
    m12: str
    m24: str
    unit: str = ""
    direction: str = "increase"          # "increase" | "decrease"


