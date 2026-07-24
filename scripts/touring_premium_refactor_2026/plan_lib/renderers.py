"""renderers — back-compat shim for the focused renderers/ subpackage.

The original 1789-line ``renderers.py`` (generate_plan.py:2306-4081) has been
split into 11 focused modules under ``renderers/`` so each file's MI stays
high (REGRA #0 potentialization — preserve every byte, improve maintainability
through focused modules). This file re-exports every public function so the
existing flat ``from plan_lib.renderers import render_*`` calls keep working.
"""
from __future__ import annotations

from .renderers import (  # noqa: F401 — back-compat re-exports
    md_table,
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
    sha256_hex,
    write_atomic,
    yaml_frontmatter,
)

__all__ = [
    "md_table", "sha256_hex", "write_atomic", "yaml_frontmatter",
    "render_index_md", "render_wave_md", "render_cross_audit_md",
    "render_architecture_md", "render_deployment_md", "render_commercial_md",
    "render_glossary_md", "render_risks_md", "render_metrics_md",
    "render_rollback_md", "render_contributing_md", "render_changelog_md",
]
