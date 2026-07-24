#!/usr/bin/env python3
"""W3.2 — Identify anemic crates (low LOC + few pub items) for absorption.

After dead-code purge (W1) and centralization (W2), W3 absorbs micro-crates
that don't justify their own crate boundary. Criteria:
  - src_loc < 500
  - pub_count < 10
  - fan_in (consumers) ≤ 2 (i.e., low reuse)
  - NOT in known long-term roadmap (allow-list)

For each anemic crate, propose a target merging crate based on dependency
proximity (which crates already use it most).

Outputs:
  - data/w3-anemic-crates-map.json
  - staging/w3-absorption-plan.md
"""
from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from collections import defaultdict
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CRATES_DIR = _ROOT / "crates"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

PUB_ITEM_RE = re.compile(r"^\s*pub\s+(?:fn|struct|enum|trait|const|static|type)\s+\w+",
                          re.MULTILINE)
CARGO_DEP_RE = re.compile(r'^\s*([a-z][a-z0-9_-]+)\s*=', re.MULTILINE)

ALLOW_LIST: set[str] = {"touring-types", "touring-protocol"}


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w3_absorve_anemic_crates", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--max-loc", type=int, default=500)
    p.add_argument("--max-pub", type=int, default=10)
    p.add_argument("--max-fan-in", type=int, default=2)
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def count_src_loc(crate: Path) -> int:
    """Count LOC in <crate>/src/."""
    src = crate / "src"
    if not src.exists():
        return 0
    total = 0
    for rs in src.rglob("*.rs"):
        if rs.is_file():
            try:
                total += sum(1 for _ in rs.read_text(encoding="utf-8",
                                                      errors="replace").splitlines())
            except OSError:
                pass
    return total


def count_pub_items(crate: Path) -> int:
    """Count pub declarations."""
    src = crate / "src"
    if not src.exists():
        return 0
    total = 0
    for rs in src.rglob("*.rs"):
        if rs.is_file():
            try:
                total += len(PUB_ITEM_RE.findall(
                    rs.read_text(encoding="utf-8", errors="replace")))
            except OSError:
                pass
    return total


def build_dep_graph() -> dict[str, set[str]]:
    """Return {crate: set(consumer crates)}."""
    consumers: dict[str, set[str]] = defaultdict(set)
    for crate_dir in sorted(_CRATES_DIR.glob("touring-*")):
        cargo = crate_dir / "Cargo.toml"
        if not cargo.exists():
            continue
        try:
            text = cargo.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        in_dep_section = False
        for line in text.splitlines():
            stripped = line.strip()
            if stripped.startswith("[") and stripped.endswith("]"):
                in_dep_section = stripped in (
                    "[dependencies]", "[dev-dependencies]", "[build-dependencies]"
                )
                continue
            if not in_dep_section:
                continue
            m = CARGO_DEP_RE.match(line)
            if m and m.group(1).startswith("touring-"):
                consumers[m.group(1)].add(crate_dir.name)
    return dict(consumers)


def audit_crate(crate_dir: Path, consumers: dict[str, set[str]]) -> dict:
    """Audit a single crate."""
    name = crate_dir.name
    return {
        "crate": name,
        "src_loc": count_src_loc(crate_dir),
        "pub_count": count_pub_items(crate_dir),
        "fan_in": len(consumers.get(name, set())),
        "consumers": sorted(consumers.get(name, set())),
        "in_allow_list": name in ALLOW_LIST,
    }


def is_anemic(audit: dict, args: argparse.Namespace) -> bool:
    """Anemia heuristic."""
    return (
        audit["src_loc"] < args.max_loc
        and audit["pub_count"] < args.max_pub
        and audit["fan_in"] <= args.max_fan_in
        and not audit["in_allow_list"]
    )


def propose_target(audit: dict) -> str | None:
    """Pick the consumer with most edges (heuristic: alphabetical-first)."""
    if not audit["consumers"]:
        return None
    return audit["consumers"][0]


def render_md(report: dict, args: argparse.Namespace) -> str:
    """Render markdown absorption plan."""
    lines = ["# W3.2 — Anemic Crate Absorption Plan", "",
             f"Generated: {datetime.now(UTC).isoformat()}", "",
             "## Thresholds",
             f"- src_loc < {args.max_loc}",
             f"- pub_count < {args.max_pub}",
             f"- fan_in ≤ {args.max_fan_in}", "",
             f"## Summary: {report['totals']['anemic']} anemic crates", "",
             "| Crate | LOC | Pub | Fan-in | Target |",
             "|-------|-----|-----|--------|--------|"]
    for a in report["anemic_crates"]:
        lines.append(
            f"| `{a['crate']}` | {a['src_loc']} | {a['pub_count']} | "
            f"{a['fan_in']} | `{a.get('proposed_absorption_target') or '?'}` |"
        )
    lines.extend(["", "## Per-crate absorption steps", ""])
    for a in report["anemic_crates"]:
        target = a.get("proposed_absorption_target") or "TARGET"
        lines.extend([
            f"### `{a['crate']}` → `{target}`", "",
            f"- Src LOC: {a['src_loc']}",
            f"- Pub items: {a['pub_count']}",
            f"- Consumers ({a['fan_in']}): {', '.join(a['consumers']) or '(none)'}",
            "- [ ] Copy `src/` into target crate as module",
            "- [ ] Remove from `workspace.members` in root Cargo.toml",
            f"- [ ] Update consumers: `use {a['crate'].replace('-', '_')}::*` "
            f"→ `use {target.replace('-', '_')}::"
            f"{a['crate'].split('-')[-1]}::*`",
            "- [ ] cargo check --workspace passes", "",
        ])
    return "\n".join(lines) + "\n"


def run(args: argparse.Namespace) -> dict:
    """Scan + classify + propose."""
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    consumers = build_dep_graph()
    audits = [audit_crate(c, consumers) for c in sorted(_CRATES_DIR.glob("touring-*"))]
    anemic = [a for a in audits if is_anemic(a, args)]
    for a in anemic:
        a["proposed_absorption_target"] = propose_target(a)
    json_path = args.output_dir / "w3-anemic-crates-map.json"
    report = {
        "script": "w3_absorve_anemic_crates",
        "wave": "W3", "subtask_refs": ["W3.2"],
        "timestamp": datetime.now(UTC).isoformat(),
        "thresholds": {"max_loc": args.max_loc, "max_pub": args.max_pub,
                        "max_fan_in": args.max_fan_in},
        "totals": {"audited": len(audits), "anemic": len(anemic),
                    "allow_listed": sum(1 for a in audits if a["in_allow_list"])},
        "anemic_crates": anemic,
        "all_audits": audits,
    }
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                          encoding="utf-8")
    md_path = _STAGING_DIR / "w3-absorption-plan.md"
    md_path.write_text(render_md(report, args), encoding="utf-8")
    return {**report, "status": "OK",
             "json_path": str(json_path.relative_to(_ROOT)),
             "md_path": str(md_path.relative_to(_ROOT))}


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps({
            "status": result["status"], "totals": result["totals"],
            "anemic_top_5": [
                {"crate": a["crate"], "loc": a["src_loc"], "pub": a["pub_count"],
                 "target": a["proposed_absorption_target"]}
                for a in result["anemic_crates"][:5]
            ],
            "json_path": result["json_path"], "md_path": result["md_path"],
        }, indent=2, ensure_ascii=False) + "\n")
        return 0
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
