#!/usr/bin/env python3
"""loop_doc_link_gate.py — OKF validator + OpenKB-style contradiction lint.

Enforces the loop's step-17 invariant: every ``.md`` in the OKF bundle is a
well-formed OKF document, linked to its plan, with resolving cross-links. Zero
external deps (no PyYAML) — a minimal flat-frontmatter parser, per the MVP policy.

Checks (BLOCKING → exit 1):
  missing_type     — an OKF doc without the required ``type`` field.
  missing_plan_id  — a bundle doc without ``plan_id`` (the step-17 cross-ref anchor).
  broken_links     — a bundle-relative link ``](/foo.md)`` that does not resolve.

Checks (ADVISORY → exit 0 unless --strict):
  orphan_docs      — a doc neither ``index.md`` itself nor referenced by it.
  contradictions   — a doc whose ``plan_id`` differs from the bundle's (OpenKB lint).

Usage:
    loop_doc_link_gate.py --bundle <dir> [--strict] [--json] [--quiet]

Exit codes: 0 clean · 1 blocking issue(s) · 2 usage error.
"""
from __future__ import annotations

import argparse
import json
import re
import shlex
import subprocess
import sys
from pathlib import Path

LINK_RE = re.compile(r"\]\((/[^)\s]+)\)")


def run_external_lint(cmd, bundle):
    """Optional real-tool adapter (ref-b): run an external linter — e.g. a real
    **OpenKB** ``lint`` — over the bundle and return its parsed JSON findings.
    The native OKF validation stands on its own; this only augments it. Absent
    tool / any error → ``{}`` (native validation unaffected)."""
    if not cmd:
        return {}
    try:
        proc = subprocess.run(shlex.split(cmd) + [str(bundle)],
                              capture_output=True, text=True, timeout=120)
        data = json.loads(proc.stdout)
        return data if isinstance(data, dict) else {}
    except Exception:  # noqa: BLE001 — optional adapter, fail-open to native
        return {}


def parse_frontmatter(text):
    """Top-level scalar keys of the leading ``--- ... ---`` block (flat only)."""
    if not text.startswith("---"):
        return {}
    end = text.find("\n---", 3)
    if end < 0:
        return {}
    fm = {}
    for line in text[3:end].splitlines():
        if not line.strip() or line.lstrip().startswith("#") or line.startswith((" ", "\t")):
            continue
        if ":" in line:
            k, v = line.split(":", 1)
            fm[k.strip()] = v.strip()
    return fm


def bundle_links(text):
    return LINK_RE.findall(text or "")


def _referenced(md, bundle, rel, index_text):
    """A doc is referenced if it — or its parent dir — appears in index.md."""
    if rel in index_text:
        return True
    if md.parent != bundle:
        parent_rel = "/" + str(md.parent.relative_to(bundle))
        return parent_rel in index_text
    return False


def check_doc(md, bundle, index_text, root_plan_id, report):
    rel = "/" + str(md.relative_to(bundle))
    text = md.read_text(errors="ignore")
    fm = parse_frontmatter(text)

    if not fm.get("type"):
        report["missing_type"].append(rel)
    pid = fm.get("plan_id")
    if not pid:
        report["missing_plan_id"].append(rel)
    elif root_plan_id and pid != root_plan_id:
        report["contradictions"].append(f"{rel}: plan_id {pid} != bundle {root_plan_id}")

    for link in bundle_links(text):
        target = link.split("#", 1)[0].lstrip("/")
        if target and not (bundle / target).exists():
            report["broken_links"].append(f"{rel} -> {link}")

    if md.name != "index.md" and not _referenced(md, bundle, rel, index_text):
        report["orphan_docs"].append(rel)


def validate_bundle(bundle: Path, strict):
    mds = sorted(bundle.rglob("*.md"))
    idx = bundle / "index.md"
    index_text = idx.read_text(errors="ignore") if idx.exists() else ""
    root_plan_id = parse_frontmatter(index_text).get("plan_id")

    report = {
        "bundle": str(bundle),
        "docs_checked": len(mds),
        "missing_type": [],
        "missing_plan_id": [],
        "broken_links": [],
        "orphan_docs": [],
        "contradictions": [],
    }
    for md in mds:
        check_doc(md, bundle, index_text, root_plan_id, report)

    blocking = report["missing_type"] + report["missing_plan_id"] + report["broken_links"]
    if strict:
        blocking = blocking + report["orphan_docs"] + report["contradictions"]
    report["ok"] = not blocking
    return report


def main(argv=None):
    ap = argparse.ArgumentParser(description="OKF bundle validator + OpenKB lint.")
    ap.add_argument("--bundle", required=True, help="OKF bundle directory")
    ap.add_argument("--strict", action="store_true", help="orphans + contradictions also block")
    ap.add_argument("--lint-cmd", default=None,
                    help="optional external linter cmd (real OpenKB lint adapter): "
                         "runs `<cmd> <bundle>`, merges its JSON findings under external_lint")
    ap.add_argument("--json", action="store_true", help="emit JSON only")
    ap.add_argument("--quiet", action="store_true", help="no human output")
    args = ap.parse_args(argv)

    bundle = Path(args.bundle)
    if not bundle.is_dir():
        print(f"error: bundle not a directory: {bundle}", file=sys.stderr)
        return 2

    report = validate_bundle(bundle, args.strict)
    external = run_external_lint(args.lint_cmd, bundle)
    if external:
        report["external_lint"] = external

    if args.json:
        print(json.dumps(report, indent=2))
    elif not args.quiet:
        state = "✅ CLEAN" if report["ok"] else "❌ ISSUES"
        print(f"{state}  bundle={report['bundle']}  docs={report['docs_checked']}")
        for key in ("missing_type", "missing_plan_id", "broken_links", "orphan_docs", "contradictions"):
            items = report[key]
            if items:
                glyph = "❌" if key in ("missing_type", "missing_plan_id", "broken_links") else "⚠ "
                print(f"  {glyph} {key} ({len(items)}):")
                for it in items[:10]:
                    print(f"      {it}")

    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
