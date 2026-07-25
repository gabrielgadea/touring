#!/usr/bin/env python3
"""Diagnostic Reporting Contract — the premium-elite presentation discipline the arsenal enforces.

Every arsenal diagnostic script prints this contract at the end of its digest so
the obligation to present a COMPLETE, professional, audit-grade report travels
*in the tool output itself*. That is what makes it survive context compaction —
the exact failure mode that silently narrowed a whole-workspace report down to a
single "one lever" summary on 2026-07-03. A discipline that lives only in the
agent's working context is erased by compaction; a contract emitted to stdout is
not.

The bar is a market-grade audit deliverable (SonarQube / Snyk / a Big-4 security
audit report), not a terminal dump. Seven sections, in order:

    1  VERDICT     — executive headline: tier/severity + the one thing that matters, <=3 lines
    2  SCORECARD   — at-a-glance: P0 gate status, tier distribution, composite
    3  FINDINGS    — BLOCK -> WARN -> ADVISORY, EVERY dimension/unit with a finding (full breadth)
    4  FUSED RISK  — ranked units: quality defect-load x architecture blast
    5  ROOT-CAUSE  — the counterfactual lever(s) that unlock the tree
    6  PROVENANCE  — enforcement from source (verify, don't assert) + lossless artifact + reproduce cmd
    7  ACTIONS     — prioritized remediation (REGRA #0), with the human-decision items flagged

A single-lever or top-N summary that REPLACES the full breakdown is a contract
violation. The lever is a synthesis layered ON TOP of the complete matrix, never
a substitute for it.
"""
from __future__ import annotations

CONTRACT_VERSION = "1.0"

# The seven mandatory sections of an elite diagnostic report, in presentation order.
# (label, one-line intent) — constant across every arsenal diagnostic.
SKELETON: list[tuple[str, str]] = [
    ("VERDICT", "executive headline: tier/severity + the one thing that matters (<=3 lines)"),
    ("SCORECARD", "at-a-glance: 6 P0 BLOCK gate status, tier distribution, composite"),
    ("FINDINGS", "BLOCK -> WARN -> ADVISORY, EVERY dimension/unit with a finding (full breadth)"),
    ("FUSED RISK", "ranked units: weighted quality defect-load x architecture blast"),
    ("ROOT-CAUSE", "the counterfactual lever(s) that unlock the tree"),
    ("PROVENANCE", "enforcement from source (verify) + lossless artifact + reproduce command"),
    ("ACTIONS", "prioritized remediation (REGRA #0); flag the human-decision items"),
]

_HEAVY = "═" * 78  # heavy box rule, professional-report typography


def contract_footer(artifact: str, diagnostic: str, presents: list[str]) -> str:
    """Render the mandatory premium-elite reporting-contract footer for a digest.

    Args:
        artifact:   on-disk lossless record the reader can audit cell-by-cell.
        diagnostic: which arsenal diagnostic emitted this (names the report subject).
        presents:   the concrete tables/rows THIS diagnostic produced that must ALL
                    be relayed — specific per the injection-density invariant, never
                    a generic banner.

    The footer is deterministic (no clock/randomness) so digests stay diffable.
    """
    dotted = lambda s: f"{s} ".ljust(13, ".")  # noqa: E731 — aligned skeleton column
    out = [
        _HEAVY,
        f" REPORTING CONTRACT v{CONTRACT_VERSION} — MANDATORY · premium-elite audit presentation",
        _HEAVY,
        f" Relay '{diagnostic}' to the user as a professional audit report — every section",
        " below, in full depth. A single-lever / top-N summary that REPLACES the complete",
        " breakdown is a contract violation.",
        "",
    ]
    out += [f"   {i}  {dotted(label)} {intent}" for i, (label, intent) in enumerate(SKELETON, 1)]
    out += ["", " This diagnostic's output MUST all reach the report — present each of:"]
    out += [f"   • {p}" for p in presents]
    out += [
        "",
        " Provenance is non-negotiable: enforcement read from source (verify, don't assert).",
        f" Lossless artifact (audit every cell here): {artifact}",
        _HEAVY,
    ]
    return "\n".join(out)


def print_contract(artifact: str, diagnostic: str, presents: list[str]) -> None:
    """Print the reporting-contract footer to stdout — the last thing a digest emits."""
    print(contract_footer(artifact, diagnostic, presents))
