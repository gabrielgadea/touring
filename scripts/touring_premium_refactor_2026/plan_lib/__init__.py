"""plan_lib — Touring Premium Refactor 2026 plan generator library.

Potentialization refactor (2026-06-26, W7): the original 4896-line
``generate_plan.py`` has been split into focused modules while preserving
every byte of the original code (REGRA #0 potentialize, never reduce).

Public API (re-exported for backwards compat):
    Constants:  _ROOT, _PLANS_DIR, _SCRIPTS_DIR, _DATA_DIR, _TESTS_DIR,
                _CHECKPOINTS_DIR, _TODAY, _TODAY_COMPACT, _VERSION,
                _PLAN_NAME, _AUTHOR_GABRIEL, _AUTHOR_TACO
    Types:      Subtask, Wave, CrateTarget, CrateCurrent, Tier, Risk, Kpi
    Data:       CRATES_TARGET, WAVES, register_waves
    Renderers:  render_index_md, render_wave_md, render_cross_audit_md,
                render_architecture_md, render_deployment_md,
                render_commercial_md, render_glossary_md, render_risks_md,
                render_metrics_md, render_rollback_md,
                render_contributing_md, render_changelog_md
    Emitters:   emit_validator_py, emit_subscript_stub, emit_all_subscripts,
                emit_cross_audit_py, emit_all_markdown,
                emit_all_validators, emit_checkpoint
"""
from __future__ import annotations

# Constants
from .constants import (
    _AUTHOR_GABRIEL,
    _AUTHOR_TACO,
    _CHECKPOINTS_DIR,
    _DATA_DIR,
    _PLANS_DIR,
    _PLANS_DIR as _ROOT_PLANS,  # noqa: F401 — legacy alias
    _SCRIPTS_DIR,
    _TESTS_DIR,
    _TODAY,
    _TODAY_COMPACT,
    _VERSION,
    _PLAN_NAME,
)

# Dataclasses
from .dataclasses import (
    CrateCurrent,
    CrateTarget,
    Kpi,
    Risk,
    Subtask,
    Tier,
    Wave,
)

# Data tables
from .data_crates import CRATES_TARGET
from .data_waves import WAVES, register_waves

# Renderers
from .renderers import (
    render_architecture_md,
    render_changelog_md,
    render_commercial_md,
    render_contributing_md,
    render_cross_audit_md,
    render_deployment_md,
    render_glossary_md,
    render_index_md,
    render_metrics_md,
    render_rollback_md,
    render_risks_md,
    render_wave_md,
)

# Emitters
from .emit_artifacts import (
    emit_all_markdown,
    emit_all_validators,
    emit_checkpoint,
)
from .emit_scripts import (
    emit_all_subscripts,
    emit_cross_audit_py,
    emit_subscript_stub,
    emit_validator_py,
)

__all__ = [
    # constants
    "_ROOT", "_PLANS_DIR", "_SCRIPTS_DIR", "_DATA_DIR", "_TESTS_DIR",
    "_CHECKPOINTS_DIR", "_TODAY", "_TODAY_COMPACT", "_VERSION",
    "_PLAN_NAME", "_AUTHOR_GABRIEL", "_AUTHOR_TACO",
    # dataclasses
    "Subtask", "Wave", "CrateTarget", "CrateCurrent",
    "Tier", "Risk", "Kpi",
    # data
    "CRATES_TARGET", "WAVES", "register_waves",
    # renderers
    "render_index_md", "render_wave_md", "render_cross_audit_md",
    "render_architecture_md", "render_deployment_md", "render_commercial_md",
    "render_glossary_md", "render_risks_md", "render_metrics_md",
    "render_rollback_md", "render_contributing_md", "render_changelog_md",
    # emitters
    "emit_validator_py", "emit_subscript_stub", "emit_all_subscripts",
    "emit_cross_audit_py", "emit_all_markdown", "emit_all_validators",
    "emit_checkpoint",
]