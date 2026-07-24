"""Top-level artifact orchestration (all-markdown / all-validators / checkpoint).

Extracted from generate_plan.py:4665-4756. ``emit_all_markdown`` regenerates
the 26 markdown deliverables; ``emit_all_validators`` regenerates the 15
``validate_W<N>.py`` scripts; ``emit_checkpoint`` writes the deterministic
TOON-style checkpoint manifest.
"""
from __future__ import annotations

from pathlib import Path
from typing import Callable

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

# ─── Orchestration: emit all artifacts ───────────────────────────────────────


ARTIFACTS_CROSS_CUTTING: dict[str, Callable[[], str]] = {
    "00-INDEX.md": render_index_md,
    "01-ARCHITECTURE.md": render_architecture_md,
    "02-DEPLOYMENT.md": render_deployment_md,
    "03-COMMERCIAL.md": render_commercial_md,
    "04-GLOSSARY.md": render_glossary_md,
    "05-RISKS.md": render_risks_md,
    "06-METRICS.md": render_metrics_md,
    "07-ROLLBACK.md": render_rollback_md,
    "08-CONTRIBUTING.md": render_contributing_md,
    "09-CHANGELOG.md": render_changelog_md,
    "CROSS-AUDIT.md": render_cross_audit_md,
}


def emit_all_markdown(force: bool = False) -> list[Path]:
    """Emit all markdown files (cross-cutting + waves)."""
    written: list[Path] = []
    # Cross-cutting docs
    for filename, renderer in ARTIFACTS_CROSS_CUTTING.items():
        path = _PLANS_DIR / filename
        if path.exists() and not force:
            LOGGER.debug("skip %s (exists, no --force)", path.name)
        content = renderer()
        write_atomic(path, content)
        written.append(path)
        LOGGER.info("  wrote %s (%d bytes)", path.name, len(content.encode("utf-8")))
    # Waves
    for wave in WAVES:
        filename = f"{wave.id}-{_slug(wave.name)}.md"
        path = _PLANS_DIR / filename
        content = render_wave_md(wave)
        write_atomic(path, content)
        written.append(path)
        LOGGER.info("  wrote %s (%d bytes)", path.name, len(content.encode("utf-8")))
    return written


def emit_all_validators(force: bool = False) -> list[Path]:
    """Emit all validate_WX.py files + cross_audit_e2e.py."""
    written: list[Path] = []
    for wave in WAVES:
        path = _SCRIPTS_DIR / f"validate_{wave.id}.py"
        content = emit_validator_py(wave)
        write_atomic(path, content)
        path.chmod(0o755)
        written.append(path)
        LOGGER.info("  wrote %s (%d bytes)", path.name, len(content.encode("utf-8")))
    # Cross-audit
    ca_path = _SCRIPTS_DIR / "cross_audit_e2e.py"
    ca_content = emit_cross_audit_py()
    write_atomic(ca_path, ca_content)
    ca_path.chmod(0o755)
    written.append(ca_path)
    LOGGER.info("  wrote %s (%d bytes)", ca_path.name, len(ca_content.encode("utf-8")))
    return written


def emit_checkpoint(written_md: list[Path], written_py: list[Path]) -> Path:
    """Emit checkpoint JSON capturing what was generated."""
    cp_path = _CHECKPOINTS_DIR / f"touring_premium_generated_{_TODAY_COMPACT}.json"
    cp_path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "plan": _PLAN_NAME,
        "version": _VERSION,
        "generated_at": datetime.now(UTC).isoformat(),
        "markdown_files": [
            {"path": str(p.relative_to(_ROOT)),
             "bytes": len(p.read_bytes()),
             "sha256": sha256_hex(p.read_text(encoding="utf-8"))}
            for p in written_md
        ],
        "validator_files": [
            {"path": str(p.relative_to(_ROOT)),
             "bytes": len(p.read_bytes())}
            for p in written_py
        ],
        "stats": {
            "waves": len(WAVES),
            "total_days_min": sum(w.days_min for w in WAVES),
            "total_days_max": sum(w.days_max for w in WAVES),
            "target_crates": len(CRATES_TARGET),
            "cross_cutting_docs": len(ARTIFACTS_CROSS_CUTTING),
        },
    }
    cp_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    return cp_path


