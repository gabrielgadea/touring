#!/usr/bin/env python3
"""W1.4 — Diagnose + propose fix for Cycle #1 (intra-server file_tools↔project_tools).

Reads the wiring cycles baseline + analyzes both files to determine which
type/function references cause the cycle. Proposes 3 strategies:
  - shared: extract common types to tools/shared.rs
  - trait: invert direction via trait abstraction
  - merge: combine both modules into a single tools.rs

Output:
  - scripts/touring_premium_refactor_2026/staging/w1-cycle1-diagnosis.json
  - scripts/touring_premium_refactor_2026/staging/w1-cycle1-fix-proposal.diff

Usage
-----
    python3 w1_fix_cycle1.py                  # diagnose only
    python3 w1_fix_cycle1.py --strategy shared --apply
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import subprocess
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"
_FILE_TOOLS = _ROOT / "crates" / "touring-server" / "src" / "tools" / "file_tools.rs"
_PROJECT_TOOLS = _ROOT / "crates" / "touring-server" / "src" / "tools" / "project_tools.rs"

CROSS_REF_RE = re.compile(r"\b(?:use\s+super::|use\s+crate::tools::)(\w+)")
TYPE_USE_RE = re.compile(r"\b(?:super::|crate::tools::)(\w+)::(\w+)")


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w1_fix_cycle1", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--strategy", choices=["shared", "trait", "merge"],
                   default="shared", help="Mitigation strategy.")
    p.add_argument("--apply", action="store_true",
                   help="Generate the actual diff/patch (still requires manual cargo check).")
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def analyze_cross_refs(path: Path) -> dict:
    """Find all `use super::X` and `super::X::Y` references."""
    if not path.exists():
        return {"exists": False, "path": str(path)}
    content = path.read_text(encoding="utf-8", errors="replace")
    imports = CROSS_REF_RE.findall(content)
    type_uses = TYPE_USE_RE.findall(content)
    return {
        "exists": True,
        "path": str(path.relative_to(_ROOT)),
        "imports_via_super_or_tools": sorted(set(imports)),
        "type_usages": [{"module": m, "item": i} for m, i in set(type_uses)],
        "loc": sum(1 for _ in content.splitlines()),
    }


def diagnose() -> dict:
    """Diagnose Cycle #1 by analyzing both files."""
    a = analyze_cross_refs(_FILE_TOOLS)
    b = analyze_cross_refs(_PROJECT_TOOLS)
    # Find common items (the ones causing the cycle)
    a_refs_b = "project_tools" in a.get("imports_via_super_or_tools", [])
    b_refs_a = "file_tools" in b.get("imports_via_super_or_tools", [])
    return {
        "file_tools": a,
        "project_tools": b,
        "cycle_evidence": {
            "file_tools_uses_project_tools": a_refs_b,
            "project_tools_uses_file_tools": b_refs_a,
            "is_real_cycle": a_refs_b and b_refs_a,
        },
    }


def propose_fix_shared(diag: dict) -> str:
    """Propose extracting common types to tools/shared.rs."""
    a_types = set((u["item"] for u in diag["file_tools"].get("type_usages", [])
                   if u["module"] == "project_tools"))
    b_types = set((u["item"] for u in diag["project_tools"].get("type_usages", [])
                   if u["module"] == "file_tools"))
    candidates = sorted(a_types | b_types)
    body = "# Strategy: shared\n\n"
    body += f"## Items to extract into crates/touring-server/src/tools/shared.rs:\n\n"
    for c in candidates or ["(diagnose detected no concrete type refs; manual review)"]:
        body += f"  - {c}\n"
    body += "\n## Steps:\n"
    body += "1. Create `crates/touring-server/src/tools/shared.rs` with extracted items.\n"
    body += "2. Update `tools/mod.rs` with `pub mod shared;`.\n"
    body += "3. Replace `use super::project_tools::X` with `use super::shared::X` in file_tools.rs.\n"
    body += "4. Replace `use super::file_tools::X` with `use super::shared::X` in project_tools.rs.\n"
    body += "5. cargo check --workspace + touring wiring cycles --min-depth 2 (expect Cycle #1 GONE).\n"
    return body


def propose_fix_trait(_diag: dict) -> str:
    """Propose trait abstraction to invert direction."""
    return (
        "# Strategy: trait\n\n"
        "Define a trait `ToolHandler` (or similar) in tools/mod.rs.\n"
        "Both file_tools and project_tools implement it.\n"
        "Cross-references happen via trait object `&dyn ToolHandler`, breaking the cycle.\n"
        "Cost: indirection at call sites; consider runtime dispatch overhead.\n"
    )


def propose_fix_merge(_diag: dict) -> str:
    """Propose merging both modules."""
    return (
        "# Strategy: merge\n\n"
        "Combine file_tools.rs + project_tools.rs into a single tools.rs (or both as submods of tools/file_project.rs).\n"
        "Eliminates the cycle by removing the boundary.\n"
        "Cost: file_project.rs becomes larger; semantic distinction lost.\n"
    )


def run(strategy: str, apply: bool) -> dict:
    """Diagnose + propose fix."""
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    diag = diagnose()
    diag_path = _STAGING_DIR / "w1-cycle1-diagnosis.json"
    diag_path.write_text(json.dumps(diag, indent=2, ensure_ascii=False), encoding="utf-8")
    proposers = {"shared": propose_fix_shared,
                 "trait": propose_fix_trait,
                 "merge": propose_fix_merge}
    proposal = proposers[strategy](diag)
    proposal_path = _STAGING_DIR / f"w1-cycle1-fix-proposal-{strategy}.md"
    proposal_path.write_text(proposal, encoding="utf-8")
    return {
        "status": "OK",
        "strategy": strategy,
        "timestamp": datetime.now(UTC).isoformat(),
        "cycle_evidence": diag["cycle_evidence"],
        "diagnosis_file": str(diag_path.relative_to(_ROOT)),
        "proposal_file": str(proposal_path.relative_to(_ROOT)),
        "apply": apply,
        "notes": ("Generated proposal only; apply requires manual cargo check + "
                  "wiring cycles validation."),
    }


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args.strategy, args.apply)
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
