"""changelog — render_changelog_md.

Extracted from renderers.py lines 1692-1789. Each module owns one logical
rendering concern (utility, index/wave/cross-audit, one of the 9 cross-cutting
docs). All public functions are re-exported by ``renderers/__init__.py``.
"""
from __future__ import annotations

from .utilities import yaml_frontmatter, md_table, write_atomic, sha256_hex

def render_changelog_md() -> str:
    """Render 09-CHANGELOG.md — template + initial entries."""
    meta = {"plan": _PLAN_NAME, "version": _VERSION, "type": "changelog", "created": _TODAY}
    fm = yaml_frontmatter(meta)
    body = textwrap.dedent(f"""\
        # 09-CHANGELOG — touring-premium-refactor-2026

        > Conventional changelog following [Keep a Changelog](https://keepachangelog.com/)
        > and [SemVer](https://semver.org/). Per-wave entries added during refactor;
        > per-crate CHANGELOG.md generated via release-plz starting at W13.

        ## [Unreleased]

        ### Planning phase (pre-W0)

        ### Added
        - **{_TODAY}** — Forensic architectural audit completed. Findings:
          46 crates, ~410k LOC, macrociclo depth 618, 5 mega-crates (69% código),
          cortex test ratio 0.56%, 4 dead crates. Memory: `audit:touring-arch-premium-refactor-2026-05-11`.
        - **{_TODAY}** — Decision approved by Gabriel: 13-crate target topology + rustup-like
          per-project deployment + 4-tier commercial model + W14 commercial integration.
          Memory: `decision:touring-premium-roadmap-2026-05-11`.
        - **{_TODAY}** — Plan structure created at `docs/plans/touring-premium-refactor-2026/`:
          - 00-INDEX.md (master index)
          - 01-ARCHITECTURE.md (full 13-crate breakdown)
          - 02-DEPLOYMENT.md (per-project + toolchain manager)
          - 03-COMMERCIAL.md (tiers + GTM)
          - 04-GLOSSARY.md (terminology)
          - 05-RISKS.md (cross-wave risk register, 70+ risks)
          - 06-METRICS.md (KPIs + quality gates)
          - 07-ROLLBACK.md (per-wave rollback procedures)
          - 08-CONTRIBUTING.md (dev setup + gates + PR template)
          - 09-CHANGELOG.md (this file)
          - W0-prep--safety-net.md through W14-product-tiers--distribution.md (15 wave files)
          - CROSS-AUDIT.md (E2E 10-dimension audit)
        - **{_TODAY}** — Scripts created at `scripts/touring_premium_refactor_2026/`:
          - generate_plan.py (N1 generator, data-driven)
          - validate_W0.py through validate_W14.py (15 validators, auto-generated)
          - cross_audit_e2e.py (10-dimension cross-validation)

        ## Template for future entries

        ### [WX-YYYY-MM-DD] — Wave WX: <Name> completed

        #### Added
        - <New features, modules, tests>

        #### Changed
        - <Refactored or relocated items>

        #### Deprecated
        - <Items scheduled for removal in next major>

        #### Removed
        - <Items deleted (e.g., dead crates)>

        #### Fixed
        - <Bug fixes>

        #### Security
        - <Security patches>

        #### Performance
        - <Bench results: speedups, regressions within budget>

        #### Metrics (per gates)

        | Gate | Pre-wave | Post-wave | Delta |
        |---|---|---|---|
        | Crate count | N | N-X | -X |
        | Cycle count | C | C-Y | -Y |
        | Workspace test ratio | R% | R'% | +Z% |
        | composite_health | H | H' | +Δ |

        ## SemVer guidance

        | Wave | Likely impact | Version bump |
        |---|---|---|
        | W0-W2 | Internal only, no public API touched | 0.x.y → 0.x.(y+1) |
        | W3 | Foundation rename (with shim) | 0.x.y → 0.(x+1).0 |
        | W4 | touring-ast → touring-code rename (with shim) | 0.x.y → 0.(x+1).0 |
        | W5-W7 | New crates, new features, shims provide compat | 0.x.y → 0.(x+1).0 |
        | W8-W10 | Internal splits, façade preserved | 0.x.y → 0.(x+1).0 |
        | W11 | Tests only, no API touched | 0.x.y → 0.x.(y+1) |
        | W12 | New CLI (touring init, etc.), backward compat via --legacy-global | 0.x.y → 0.(x+1).0 |
        | W13 | Publishing infra, no functional changes | 0.x.y → 0.x.(y+1) |
        | **W14** | **1.0.0 GA** | 0.(x+1).0 → **1.0.0** |

        ## References

        - SemVer: https://semver.org/
        - Keep a Changelog: https://keepachangelog.com/en/1.1.0/
        - Conventional Commits: https://www.conventionalcommits.org/en/v1.0.0/
        - release-plz: https://release-plz.dev/ (used from W13)
        """)
    return fm + body


