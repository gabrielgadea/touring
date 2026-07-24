"""renderers/ — markdown renderer library.

Each module owns one logical rendering concern:
  - ``utilities``        — yaml_frontmatter / md_table / write_atomic / sha256_hex / _slug
  - ``index_wave``       — render_index_md / render_wave_md / render_cross_audit_md
  - ``architecture``     — render_architecture_md
  - ``deployment``       — render_deployment_md
  - ``commercial``       — render_commercial_md
  - ``glossary``         — render_glossary_md
  - ``risks``            — render_risks_md
  - ``metrics``          — render_metrics_md
  - ``rollback``         — render_rollback_md
  - ``contributing``     — render_contributing_md
  - ``changelog``        — render_changelog_md

All public functions are re-exported here for the existing flat
``from plan_lib.renderers import render_*`` callers.
"""
from __future__ import annotations

from . import (
    architecture,
    changelog,
    commercial,
    contributing,
    deployment,
    glossary,
    index_wave,
    metrics,
    risks,
    rollback,
    utilities,
)
from .utilities import md_table, sha256_hex, write_atomic, yaml_frontmatter
from .index_wave import _slug
from .architecture import render_architecture_md
from .changelog import render_changelog_md
from .commercial import render_commercial_md
from .contributing import render_contributing_md
from .deployment import render_deployment_md
from .glossary import render_glossary_md
from .index_wave import render_cross_audit_md, render_index_md, render_wave_md
from .metrics import render_metrics_md
from .risks import render_risks_md
from .rollback import render_rollback_md

__all__ = [
    "render_index_md", "render_wave_md", "render_cross_audit_md",
    "render_architecture_md", "render_deployment_md", "render_commercial_md",
    "render_glossary_md", "render_risks_md", "render_metrics_md",
    "render_rollback_md", "render_contributing_md", "render_changelog_md",
]
