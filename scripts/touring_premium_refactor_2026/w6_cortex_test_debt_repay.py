#!/usr/bin/env python3
"""W6.0 — touring-cortex test debt re-evaluation (REFINED v2).

REFINEMENT (2026-05-11): v1 reported pub-ratio = 2.36 (236%) and concluded
"no debt", but the priority list showed 30+ files with pub > 3 AND tests = 0.
The aggregate masked per-file gaps. v2 uses **3 distinct metrics**:

  1. PUB-RATIO   = test_fns / pub_items (aggregate, file-summed)
  2. LOC-RATIO   = test_block_loc / src_loc (excluding test blocks)
  3. FILE-GAP    = count of files where pub > 3 AND test_fns == 0

The blocker fires if FILE-GAP ≥ 20 (default), regardless of the aggregate
ratios. This is the "agregado mente" guard.

Outputs:
  - data/w6-cortex-coverage-map.json          (3-metric report)
  - staging/w6-cortex-priority-modules.md     (human-readable triage)
  - staging/test-stubs/<module>_tests.rs      (if --emit-stubs)
  - staging/w6-decision.json                  (blocker fires? rationale)

Usage
-----
    python3 w6_cortex_test_debt_repay.py                  # 3-metric audit
    python3 w6_cortex_test_debt_repay.py --emit-stubs     # generate skeletons
    python3 w6_cortex_test_debt_repay.py --file-gap-threshold 30
"""
from __future__ import annotations

import argparse
import json
import logging
import re
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
_CORTEX_DIR = _ROOT / "crates" / "touring-cortex" / "src"
_DATA_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
_STAGING_DIR = _ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"

TEST_FN_RE = re.compile(r"#\[(?:test|tokio::test|cfg\(test\))\]")
PUB_ITEM_RE = re.compile(r"^\s*pub\s+(?:fn|struct|enum|trait|const|static|type)\s+(\w+)",
                          re.MULTILINE)
# Match a #[cfg(test)] mod block to compute its LOC separately
CFG_TEST_MOD_RE = re.compile(
    r"#\[cfg\(test\)\]\s*mod\s+\w+\s*\{",
    re.MULTILINE,
)


def find_test_block_end(content: str, start: int) -> int:
    """Find the closing `}` of a `mod tests { ... }` block by brace count."""
    depth = 0
    i = start
    started = False
    while i < len(content):
        c = content[i]
        if c == "{":
            depth += 1
            started = True
        elif c == "}":
            depth -= 1
            if started and depth == 0:
                return i + 1
        i += 1
    return len(content)


def count_test_block_loc(content: str) -> int:
    """Count LOC inside #[cfg(test)] mod blocks (closed-brace aware)."""
    test_loc = 0
    for m in CFG_TEST_MOD_RE.finditer(content):
        block_start = m.end() - 1  # position of `{`
        block_end = find_test_block_end(content, block_start)
        snippet = content[block_start:block_end]
        test_loc += sum(1 for _ in snippet.splitlines())
    return test_loc


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w6_cortex_test_debt_repay", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--file-gap-threshold", type=int, default=20,
                   help="Blocker fires if count of files with pub>3 and tests=0 ≥ this.")
    p.add_argument("--min-pub-for-priority", type=int, default=3,
                   help="Min pub items to consider a file a priority target.")
    p.add_argument("--target-pub-ratio", type=float, default=0.15,
                   help="Aggregate pub-ratio target.")
    p.add_argument("--target-loc-ratio", type=float, default=0.10,
                   help="Aggregate loc-ratio target.")
    p.add_argument("--emit-stubs", action="store_true",
                   help="Generate test stubs for priority files.")
    p.add_argument("--top-n", type=int, default=20,
                   help="Top-N priority modules to stub.")
    p.add_argument("--output-dir", type=Path, default=_DATA_DIR)
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def audit_file(path: Path) -> dict:
    """3-metric audit of a single .rs file."""
    try:
        content = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return {"file": str(path), "error": "unreadable"}
    total_loc = sum(1 for _ in content.splitlines())
    test_block_loc = count_test_block_loc(content)
    src_loc = total_loc - test_block_loc
    test_fns = len(TEST_FN_RE.findall(content))
    pub_items = PUB_ITEM_RE.findall(content)
    return {
        "file": str(path.relative_to(_ROOT)),
        "total_loc": total_loc,
        "src_loc": src_loc,
        "test_block_loc": test_block_loc,
        "test_fns": test_fns,
        "pub_items": pub_items,
        "pub_count": len(pub_items),
        "pub_ratio": (test_fns / max(len(pub_items), 1)) if pub_items else 0.0,
        "loc_ratio": (test_block_loc / max(src_loc, 1)) if src_loc else 0.0,
        # Priority: many pubs, no tests → high score
        "priority_score": (
            len(pub_items) * (1 - min(test_fns / max(len(pub_items), 1), 1.0))
        ),
    }


def emit_test_stub(file_audit: dict, target_dir: Path) -> Path:
    """Generate minimal test stub for a priority file."""
    src_path = Path(file_audit["file"])
    mod_name = src_path.stem
    stub_path = target_dir / f"{mod_name}_tests.rs"
    pub_items = file_audit["pub_items"][:8]
    lines = [
        f"// AUTO-GENERATED test stub for {src_path}",
        f"// {file_audit['pub_count']} pub items, {file_audit['test_fns']} existing tests",
        f"// Pub ratio: {file_audit['pub_ratio']:.1%}, LOC ratio: {file_audit['loc_ratio']:.1%}",
        "// Replace `unimplemented!()` with real assertions during W6 prep.",
        "",
        "#[cfg(test)]",
        f"mod {mod_name}_priority_tests {{",
        "    use super::*;",
        "",
    ]
    for item in pub_items:
        lines.extend([
            "    #[test]",
            f"    fn test_{item.lower()}_happy_path() {{",
            f"        // TODO: invoke {item}, assert expected state",
            "        unimplemented!();",
            "    }",
            "",
            "    #[test]",
            f"    fn test_{item.lower()}_error_path() {{",
            f"        // TODO: invoke {item} with invalid inputs",
            "        unimplemented!();",
            "    }",
            "",
        ])
    lines.append("}")
    target_dir.mkdir(parents=True, exist_ok=True)
    stub_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return stub_path


def compute_decision(report: dict, args: argparse.Namespace) -> dict:
    """Blocker decision matrix."""
    t = report["totals"]
    triggers: list[str] = []
    if t["file_gap_count"] >= args.file_gap_threshold:
        triggers.append(
            f"FILE_GAP_EXCEEDED: {t['file_gap_count']} files with pub>{args.min_pub_for_priority} "
            f"and 0 tests (threshold: {args.file_gap_threshold})"
        )
    if t["pub_ratio"] < args.target_pub_ratio:
        triggers.append(
            f"PUB_RATIO_BELOW: {t['pub_ratio']:.2%} < {args.target_pub_ratio:.2%}"
        )
    if t["loc_ratio"] < args.target_loc_ratio:
        triggers.append(
            f"LOC_RATIO_BELOW: {t['loc_ratio']:.2%} < {args.target_loc_ratio:.2%}"
        )
    return {
        "blocker_fires": len(triggers) > 0,
        "triggers": triggers,
        "rationale": ("aggregate-may-mislead — per-file gap is the load-bearing signal"
                       if triggers else "all 3 metrics within target"),
    }


def run(args: argparse.Namespace) -> dict:
    """3-metric audit + decision."""
    if not _CORTEX_DIR.exists():
        return {"status": "MISSING_CRATE", "path": str(_CORTEX_DIR)}
    audits = [audit_file(f) for f in _CORTEX_DIR.rglob("*.rs") if f.is_file()]
    audits = [a for a in audits if "error" not in a]
    # Aggregate metrics
    total_src_loc = sum(a["src_loc"] for a in audits)
    total_test_block_loc = sum(a["test_block_loc"] for a in audits)
    total_test_fns = sum(a["test_fns"] for a in audits)
    total_pub = sum(a["pub_count"] for a in audits)
    file_gap = sum(
        1 for a in audits
        if a["pub_count"] >= args.min_pub_for_priority and a["test_fns"] == 0
    )
    pub_ratio = total_test_fns / max(total_pub, 1)
    loc_ratio = total_test_block_loc / max(total_src_loc, 1)
    # Priority
    priority = sorted(
        (a for a in audits if a["pub_count"] >= args.min_pub_for_priority
         and a["pub_ratio"] < args.target_pub_ratio),
        key=lambda x: -x["priority_score"],
    )
    args.output_dir.mkdir(parents=True, exist_ok=True)
    _STAGING_DIR.mkdir(parents=True, exist_ok=True)
    report = {
        "script": "w6_cortex_test_debt_repay",
        "version": "v2-refined-3metrics",
        "wave": "W6", "subtask_refs": ["W6.0"],
        "timestamp": datetime.now(UTC).isoformat(),
        "totals": {
            "files_audited": len(audits),
            "total_src_loc": total_src_loc,
            "total_test_block_loc": total_test_block_loc,
            "total_test_fns": total_test_fns,
            "total_pub_items": total_pub,
            "pub_ratio": round(pub_ratio, 4),
            "loc_ratio": round(loc_ratio, 4),
            "file_gap_count": file_gap,
            "file_gap_threshold": args.file_gap_threshold,
        },
        "priority_modules": priority[:args.top_n],
    }
    decision = compute_decision(report, args)
    report["decision"] = decision
    report_path = args.output_dir / "w6-cortex-coverage-map.json"
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                           encoding="utf-8")
    decision_path = _STAGING_DIR / "w6-decision.json"
    decision_path.write_text(json.dumps(decision, indent=2, ensure_ascii=False),
                             encoding="utf-8")
    # Markdown
    md_path = _STAGING_DIR / "w6-cortex-priority-modules.md"
    blocker_emoji = "🔴 FIRES" if decision["blocker_fires"] else "🟢 SKIP"
    md = [
        "# W6 — touring-cortex Test Debt (v2 — 3 metrics)",
        "",
        f"## Blocker decision: **{blocker_emoji}**",
        "",
        f"**Rationale**: {decision['rationale']}",
        ""]
    if decision["triggers"]:
        md.append("### Triggers")
        for t in decision["triggers"]:
            md.append(f"- {t}")
        md.append("")
    md.extend([
        "## 3 distinct metrics",
        "",
        "| Metric | Value | Target | Status |",
        "|--------|-------|--------|--------|",
        f"| Pub-ratio (test_fns/pub_items) | {pub_ratio:.2%} | {args.target_pub_ratio:.2%} | "
        f"{'🟢' if pub_ratio >= args.target_pub_ratio else '🔴'} |",
        f"| LOC-ratio (test_loc/src_loc)   | {loc_ratio:.2%} | {args.target_loc_ratio:.2%} | "
        f"{'🟢' if loc_ratio >= args.target_loc_ratio else '🔴'} |",
        f"| File-gap (pub≥{args.min_pub_for_priority} & tests=0) | {file_gap} | "
        f"<{args.file_gap_threshold} | "
        f"{'🟢' if file_gap < args.file_gap_threshold else '🔴'} |",
        "",
        f"## Top {args.top_n} priority modules",
        "",
        "| Rank | File | LOC | Pub | Tests | Pub% | Priority |",
        "|------|------|-----|-----|-------|------|----------|",
    ])
    for i, p in enumerate(priority[:args.top_n], 1):
        md.append(
            f"| {i} | `{p['file']}` | {p['src_loc']} | {p['pub_count']} | "
            f"{p['test_fns']} | {p['pub_ratio']:.0%} | {p['priority_score']:.1f} |"
        )
    md_path.write_text("\n".join(md) + "\n", encoding="utf-8")
    stubs_emitted = []
    if args.emit_stubs:
        stub_dir = _STAGING_DIR / "test-stubs"
        for p in priority[:args.top_n]:
            stub_path = emit_test_stub(p, stub_dir)
            stubs_emitted.append(str(stub_path.relative_to(_ROOT)))
    report["status"] = "OK"
    report["report_path"] = str(report_path.relative_to(_ROOT))
    report["priority_md_path"] = str(md_path.relative_to(_ROOT))
    report["decision_path"] = str(decision_path.relative_to(_ROOT))
    report["stubs_emitted"] = stubs_emitted
    return report


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        if result.get("status") == "MISSING_CRATE":
            print(json.dumps(result, indent=2, ensure_ascii=False))
            return 1
        print(json.dumps({
            "status": result["status"],
            "version": result.get("version"),
            "totals": result["totals"],
            "decision": result["decision"],
            "top_priority_files": [p["file"] for p in result["priority_modules"][:5]],
            "stubs_emitted_count": len(result["stubs_emitted"]),
            "report_path": result["report_path"],
            "priority_md_path": result["priority_md_path"],
        }, indent=2, ensure_ascii=False))
        return 0 if not result["decision"]["blocker_fires"] else 2
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
