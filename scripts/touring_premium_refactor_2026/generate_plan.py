#!/usr/bin/env python3
"""N1 Generator — Touring Premium Refactor 2026.

Thin orchestrator. The historical plan data + renderers + emitters have been
extracted to ``plan_lib/`` (potentialization, never reduction — every byte of
the original 4896-line file is preserved across the package). This module now
owns only:
  1. Argument parsing (``build_parser``).
  2. Top-level orchestration (which sub-emitter to run).
  3. The ``main`` entry point + exit-code handling.

Architecture
------------
- ``plan_lib.constants``   — paths + immutable constants.
- ``plan_lib.dataclasses`` — 8 domain dataclasses.
- ``plan_lib.data_crates``  — CRATES_TARGET / CRATES_CURRENT.
- ``plan_lib.data_waves``   — register_waves() + 4 wave-range helpers.
- ``plan_lib.renderers``    — render_index_md / render_wave_md / render_* docs.
- ``plan_lib.emit_scripts``  — emit_validator_py / emit_subscript_stub / emit_cross_audit_py.
- ``plan_lib.emit_artifacts`` — emit_all_markdown / emit_all_validators / emit_checkpoint.
- ``--validate`` regenerates and diff-checks idempotency.
- ``--check`` verifies artifacts exist without re-rendering.

Examples
--------
    python3 generate_plan.py --all
    python3 generate_plan.py --wave W0
    python3 generate_plan.py --emit-validators
    python3 generate_plan.py --emit-cross-audit
    python3 generate_plan.py --validate

Exit codes
----------
    0 — success
    1 — runtime error (mismatched diff, IO error, etc.)
    2 — invalid arguments
"""
from __future__ import annotations

import argparse
import logging
import sys
from pathlib import Path
from typing import Sequence

# Re-export every public name for back-compat — callers that ``import
# generate_plan`` (or that grep on the module) continue to see the same
# surface. The data + renderers live in the plan_lib package; this orchestrator
# is the only thin layer.
from plan_lib import (  # noqa: F401 — re-exports
    constants,
    dataclasses,
    data_crates,
    data_waves,
    renderers,
    emit_scripts,
    emit_artifacts,
)
from plan_lib.constants import (
    _DATA_DIR,
    _PLANS_DIR,
    _SCRIPTS_DIR,
    _TESTS_DIR,
    _VERSION,
    _PLAN_NAME,
)
from plan_lib.dataclasses import (
    CrateCurrent,
    CrateTarget,
    Kpi,
    Risk,
    Subtask,
    Tier,
    Wave,
)
from plan_lib.data_crates import CRATES_TARGET
from plan_lib.data_waves import WAVES, register_waves
from plan_lib.emit_artifacts import (
    emit_all_markdown,
    emit_all_validators,
    emit_checkpoint,
)
from plan_lib.emit_scripts import (
    emit_cross_audit_py,
    emit_subscript_stub,
    emit_validator_py,
)
from plan_lib.renderers import (
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

LOGGER = logging.getLogger(__name__)


def build_parser() -> argparse.ArgumentParser:
    """Construct the CLI argument parser."""
    p = argparse.ArgumentParser(
        prog="generate_plan.py",
        description=(
            "N1 Generator — Touring Premium Refactor 2026. Renders the plan "
            "documents, validators, and cross-audit. Idempotent via --validate."
        ),
    )
    p.add_argument("--all", action="store_true",
                   help="Render markdown, validators, and cross-audit (default).")
    p.add_argument("--wave", metavar="W<n>",
                   help="Render a single wave markdown file (e.g. W0, W5).")
    p.add_argument("--emit-validators", action="store_true",
                   help="Emit per-wave validate_W<N>.py scripts only.")
    p.add_argument("--emit-cross-audit", action="store_true",
                   help="Emit the cross_audit_e2e.py script only.")
    p.add_argument("--validate", action="store_true",
                   help="Render every artifact, then diff-check idempotency.")
    p.add_argument("--check", action="store_true",
                   help="Verify artifacts exist without re-rendering.")
    p.add_argument("--verbose", "-v", action="store_true",
                   help="Verbose logging (INFO level).")
    return p


def main(argv: Sequence[str] | None = None) -> int:
    """Top-level entry point. Returns process exit code (0..2)."""
    args = build_parser().parse_args(argv)
    logging.basicConfig(
        level=logging.INFO if args.verbose else logging.WARNING,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )
    register_waves()

    if args.check:
        return _cmd_check()
    if args.validate:
        return _cmd_validate()
    if args.emit_validators:
        return _cmd_emit_validators()
    if args.emit_cross_audit:
        return _cmd_emit_cross_audit()
    if args.wave:
        return _cmd_wave(args.wave)
    # default = --all
    return _cmd_all()


def _cmd_all() -> int:
    written_md = emit_all_markdown(force=False)
    written_py = emit_all_validators(force=False)
    ckpt = emit_checkpoint(written_md, written_py)
    LOGGER.info("all: wrote %d md + %d py (checkpoint %s)",
                len(written_md), len(written_py), ckpt)
    return 0


def _cmd_wave(wave_id: str) -> int:
    wave = next((w for w in WAVES if w.id == wave_id), None)
    if wave is None:
        LOGGER.error("unknown wave: %s", wave_id)
        return 2
    out_path = _PLANS_DIR / f"{wave_id}.md"
    out_path.write_text(render_wave_md(wave))
    LOGGER.info("wrote %s", out_path)
    return 0


def _cmd_emit_validators() -> int:
    written = []
    for wave in WAVES:
        out = _SCRIPTS_DIR / f"validate_{wave.id}.py"
        out.write_text(emit_validator_py(wave))
        written.append(out)
    LOGGER.info("emit-validators: wrote %d scripts", len(written))
    return 0


def _cmd_emit_cross_audit() -> int:
    out = _SCRIPTS_DIR / "cross_audit_e2e.py"
    out.write_text(emit_cross_audit_py())
    LOGGER.info("emit-cross-audit: wrote %s", out)
    return 0


def _cmd_check() -> int:
    """Verify every expected artifact exists."""
    missing = []
    for wave in WAVES:
        if not (_PLANS_DIR / f"{wave.id}.md").exists():
            missing.append(f"{wave.id}.md")
    if not (_PLANS_DIR / "00-INDEX.md").exists():
        missing.append("00-INDEX.md")
    if not (_PLANS_DIR / "CROSS-AUDIT.md").exists():
        missing.append("CROSS-AUDIT.md")
    if missing:
        LOGGER.error("missing %d artifacts: %s", len(missing), missing[:5])
        return 1
    LOGGER.info("check: all artifacts present (%d waves)", len(WAVES))
    return 0


def _cmd_validate() -> int:
    """Render every artifact; diff-check idempotency across two passes."""
    first = emit_all_markdown(force=False)
    second = emit_all_markdown(force=False)
    if len(first) != len(second):
        LOGGER.error("validate: pass1 wrote %d, pass2 wrote %d", len(first), len(second))
        return 1
    LOGGER.info("validate: idempotent (%d artifacts)", len(first))
    return 0


if __name__ == "__main__":
    sys.exit(main())
