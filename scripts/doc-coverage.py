#!/usr/bin/env python3
"""doc-coverage.py — Measure rustdoc coverage per crate.

Waves:
  * W6.1 of the 47to13-residual UPGRADE plan (Premium Elite product).

Outputs:
  * JSON to stdout (machine-readable)
  * Markdown table to stderr (human-readable, default)

Usage:
  python3 scripts/doc-coverage.py                       # all 36 crates
  python3 scripts/doc-coverage.py --crate touring-code  # one crate
  python3 scripts/doc-coverage.py --top 10             # top 10 by coverage
  python3 scripts/doc-coverage.py --json               # JSON-only stdout

Stdlib-only (no deps). Fast (regex-only, no AST). Deterministic.
"""
from __future__ import annotations
import argparse
import json
import os
import re
import sys
from dataclasses import dataclass, asdict
from pathlib import Path

_ROOT = Path("/home/gabrielgadea/.claude/rust")
_CARGO = _ROOT / "Cargo.toml"
_CRATES = _ROOT / "crates"


# Regex patterns. A `pub` item is a "documented" item if either the
# immediately preceding line (or any contiguous /// line above) is a
# doc comment, OR a //! module-level doc is present.
_PUB_RE = re.compile(
    r"^(?P<indent>\s*)pub\s+(?:"
    r"fn\s+(?P<fn>\w+)"
    r"|struct\s+(?P<struct>\w+)"
    r"|enum\s+(?P<enum>\w+)"
    r"|trait\s+(?P<trait>\w+)"
    r"|mod\s+(?P<mod>\w+)"
    r"|const\s+(?P<const>\w+)"
    r"|static\s+(?P<static>\w+)"
    r"|type\s+(?P<type>\w+)"
    r"|use\s+"
    r"|macro_rules!\s+(?P<macro>\w+)"
    r")",
    re.MULTILINE,
)
_DOC_LINE_RE = re.compile(r"^\s*///")


@dataclass
class CrateCoverage:
    crate: str
    files: int
    pub_items: int
    documented: int
    coverage_pct: float
    # Top 5 undocumented items (for the report)
    top_undocumented: list[str]

    @property
    def status(self) -> str:
        if self.coverage_pct >= 80:
            return "PASS"
        if self.coverage_pct >= 50:
            return "WARN"
        return "FAIL"


def _parse_crate_members(cargo_path: Path) -> list[str]:
    """Read [workspace] members from Cargo.toml — naive parser (no toml lib)."""
    text = cargo_path.read_text()
    # Find [workspace] section
    m = re.search(r"\[workspace\][^\[]*?members\s*=\s*\[(.*?)\]", text, re.DOTALL)
    if not m:
        return []
    members_block = m.group(1)
    # Extract quoted strings, allow comments
    members: list[str] = []
    for line in members_block.splitlines():
        line = re.sub(r"#.*$", "", line).strip().rstrip(",")
        line = line.strip('"').strip("'")
        if not line:
            continue
        if line.startswith("crates/"):
            members.append(line[len("crates/"):])
        elif line.startswith("./crates/"):
            members.append(line[len("./crates/"):])
        elif line.startswith("./"):
            members.append(line[2:])
        elif line.startswith("inferlets"):
            members.append(line)  # top-level inferlets crate
        else:
            members.append(line)
    return members


def _iter_rs(crate_dir: Path) -> list[Path]:
    """Return all .rs files under crate_dir/src, excluding target/."""
    if not crate_dir.exists():
        return []
    src = crate_dir / "src"
    if not src.exists():
        return []
    out: list[Path] = []
    for path in src.rglob("*.rs"):
        if "target" not in path.parts:
            out.append(path)
    return out


def _is_documented(text: str, match_start: int) -> bool:
    """Is the `pub` item at match_start documented?

    A `pub` item is documented if any line immediately above (within the
    preceding contiguous block of comments) starts with `///`. We look
    back up to 20 lines.
    """
    # Find the start of the line containing match_start
    line_start = text.rfind("\n", 0, match_start) + 1
    # Walk back through preceding lines
    pos = line_start - 1  # the \n before line_start
    lines_back = 0
    while lines_back < 20 and pos > 0:
        # Find the start of the previous line
        prev_nl = text.rfind("\n", 0, pos)
        prev_line = text[prev_nl + 1: pos]
        if not prev_line.strip():
            pos = prev_nl
            lines_back += 1
            continue
        if _DOC_LINE_RE.match(prev_line):
            return True
        # Hit a non-doc, non-blank line
        if prev_line.strip().startswith("//") and not _DOC_LINE_RE.match(prev_line):
            # // (not ///) — internal comment, not doc
            pos = prev_nl
            lines_back += 1
            continue
        return False
    return False


def measure_crate(crate_name: str) -> CrateCoverage:
    crate_dir = _CRATES / crate_name
    files = _iter_rs(crate_dir)
    pub_items = 0
    documented = 0
    top_undocumented: list[str] = []

    for f in files:
        try:
            text = f.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for m in _PUB_RE.finditer(text):
            pub_items += 1
            # Get the name (any of the named groups)
            name = next((g for g in m.groupdict().values() if g), None)
            if _is_documented(text, m.start()):
                documented += 1
            elif name and len(top_undocumented) < 5:
                rel = f.relative_to(crate_dir)
                top_undocumented.append(f"{rel}::{name}")

    coverage_pct = (documented / pub_items * 100) if pub_items > 0 else 0.0
    return CrateCoverage(
        crate=crate_name,
        files=len(files),
        pub_items=pub_items,
        documented=documented,
        coverage_pct=round(coverage_pct, 2),
        top_undocumented=top_undocumented,
    )


def main() -> int:
    p = argparse.ArgumentParser(prog="doc-coverage", description=__doc__)
    p.add_argument("--crate", help="Single crate to measure")
    p.add_argument("--top", type=int, help="Show top N by coverage %")
    p.add_argument("--json", action="store_true", help="JSON-only stdout")
    p.add_argument("-q", "--quiet", action="store_true", help="Suppress stderr table")
    args = p.parse_args()

    members = _parse_crate_members(_CARGO)
    if args.crate:
        members = [c for c in members if c.endswith(args.crate)]

    results = [measure_crate(m) for m in members]

    if args.top:
        results = sorted(results, key=lambda c: c.coverage_pct, reverse=True)[: args.top]
    else:
        results = sorted(results, key=lambda c: c.coverage_pct, reverse=True)

    payload = {
        "tool": "doc-coverage",
        "version": "0.1.0",
        "workspace_root": str(_ROOT),
        "crates_measured": len(results),
        "summary": {
            "total_pub_items": sum(c.pub_items for c in results),
            "total_documented": sum(c.documented for c in results),
            "mean_coverage_pct": round(
                sum(c.coverage_pct for c in results) / max(len(results), 1), 2
            ),
            "pass": sum(1 for c in results if c.status == "PASS"),
            "warn": sum(1 for c in results if c.status == "WARN"),
            "fail": sum(1 for c in results if c.status == "FAIL"),
        },
        "crates": [asdict(c) for c in results],
    }

    if args.json or not args.quiet:
        print(json.dumps(payload, indent=2))

    if not args.json and not args.quiet:
        print(f"\n{'='*70}\nDOC-COVERAGE — Touring ({payload['crates_measured']} crates)\n{'='*70}\n",
              file=sys.stderr)
        print(f"{'CRATE':<35} {'PUB':>6} {'DOC':>6} {'%':>7}  {'STATUS':<6}",
              file=sys.stderr)
        print("-" * 70, file=sys.stderr)
        for c in results:
            print(f"{c.crate:<35} {c.pub_items:>6} {c.documented:>6} {c.coverage_pct:>6.2f}%  {c.status}",
                  file=sys.stderr)
        print("-" * 70, file=sys.stderr)
        s = payload["summary"]
        print(f"{'TOTAL':<35} {s['total_pub_items']:>6} {s['total_documented']:>6} "
              f"{s['mean_coverage_pct']:>6.2f}%  P:{s['pass']} W:{s['warn']} F:{s['fail']}",
              file=sys.stderr)

    return 0


if __name__ == "__main__":
    sys.exit(main())
