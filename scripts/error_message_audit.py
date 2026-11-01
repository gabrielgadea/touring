#!/usr/bin/env python3
# #tags: kind:script lang:python purpose:error-message-audit domain:code-mode process:w8 status:active
"""W8 T6 — audit CLI error messages for agent-first quality (P9).

An error that names its next action lets the model self-correct in the same
turn; a bare "invalid X" burns a round-trip. This audit scans `anyhow!`/
`bail!` literals in the CLI-facing crates and flags messages that carry no
actionable continuation.

Heuristic (calibrated on the run/exec paths): a message TEACHES when it
contains an imperative continuation marker — a flag (`--`), "run ", "use ",
"try ", "rerun", "available", "expected", "provide", "set ", "see ", or names
a concrete command (`touring `). Everything else is flagged for review.

Usage:
    python3 scripts/error_message_audit.py [--crate touring-server ...] [--json]
Exit 0 always (advisory audit — the report is the product, REGRA #21 applies
to the fixes it feeds, not to the audit run itself).
"""
from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

DEFAULT_CRATES = ["touring-server", "touring-cli", "touring-ceg"]
TEACH_MARKERS = (
    "--", "run ", "use ", "try ", "rerun", "available", "expected",
    "provide", "set ", "see ", "touring ", "one of", "instead",
)
ERR_RE = re.compile(r'(?:bail!|anyhow!)\(\s*"((?:[^"\\]|\\.){8,200}?)"', re.S)


def teaches(msg: str) -> bool:
    low = msg.lower()
    return any(m in low for m in TEACH_MARKERS)


def audit(root: pathlib.Path, crates: list[str]) -> dict:
    flagged: list[dict] = []
    total = 0
    for crate in crates:
        for rs in (root / "crates" / crate / "src").rglob("*.rs"):
            text = rs.read_text(errors="ignore")
            for m in ERR_RE.finditer(text):
                total += 1
                msg = m.group(1)
                if not teaches(msg):
                    line = text[: m.start()].count("\n") + 1
                    flagged.append(
                        {"file": str(rs.relative_to(root)), "line": line, "message": msg[:120]}
                    )
    return {
        "total_error_literals": total,
        "flagged": len(flagged),
        "teach_ratio": round(1 - len(flagged) / total, 3) if total else None,
        "offenders": flagged,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--crate", action="append", dest="crates")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--top", type=int, default=20)
    ap.add_argument(
        "--check-ratio", type=float, default=None, metavar="X",
        help="campaign predicate: print METRIC=<teach_ratio> and exit 0 iff ratio >= X",
    )
    args = ap.parse_args()
    root = pathlib.Path(__file__).resolve().parent.parent
    report = audit(root, args.crates or DEFAULT_CRATES)
    if args.check_ratio is not None:
        ratio = report["teach_ratio"] or 0.0
        print(f"METRIC={ratio}")
        return 0 if ratio >= args.check_ratio else 1
    if args.json:
        report["offenders"] = report["offenders"][: args.top]
        print(json.dumps(report, indent=1))
    else:
        print(
            f"error literals: {report['total_error_literals']} | teach-ratio: "
            f"{report['teach_ratio']} | flagged: {report['flagged']}"
        )
        for o in report["offenders"][: args.top]:
            print(f"  {o['file']}:{o['line']}  {o['message']!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
