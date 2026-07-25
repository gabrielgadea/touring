#!/usr/bin/env python3
"""loop_diagnose.py — one-shot deep diagnostic for the Loop Engineering loop.

Composes the Touring intelligence commands into a single digest so the loop's
OUTER phase (steps 1-4: recall + deep diagnostic + overview) is ONE call, not N.
Each sub-diagnostic is best-effort (fail-open); an OKF-compliant diagnostic
document is written into the bundle when ``--bundle`` is given.

Sub-diagnostics:
  health    — touring status -j            (composite health, symbols, orphans)
  quality50 — touring-quality --workspace  (composite, tier, blockers, warnings)
  wiring    — touring wiring orphans -j     (orphan count)
  memory    — touring memory recall <topic> (prior context — top keys)
  structure — touring map <scope>           (workspace structure, best-effort)

Usage:
    loop_diagnose.py --scope <path> [--topic <str>] [--bundle <dir>]
                     [--plan-id <id>] [--json] [--quiet]
"""
from __future__ import annotations

import argparse
import datetime
import json
import subprocess
import sys
from pathlib import Path


def run(cmd, timeout=300):
    """Run a command; return (rc, stdout, stderr). Never raises (fail-open)."""
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout, p.stderr
    except Exception as exc:  # noqa: BLE001 — fail-open by design
        return 127, "", str(exc)


def parse_json(text):
    text = (text or "").strip()
    if not text:
        return None
    try:
        return json.loads(text)
    except Exception:  # noqa: BLE001
        start, end = text.find("{"), text.rfind("}")
        if 0 <= start < end:
            try:
                return json.loads(text[start : end + 1])
            except Exception:  # noqa: BLE001
                return None
    return None


# ── Sub-diagnostics ──────────────────────────────────────────────────────────
def diag_health():
    _, out, _e = run(["touring", "status", "-j"], timeout=30)
    data = parse_json(out)
    if not data:
        return {"available": False}
    idx = data.get("index", {}) if isinstance(data.get("index"), dict) else {}
    wir = data.get("wiring", {}) if isinstance(data.get("wiring"), dict) else {}
    return {
        "available": True,
        "composite_health": data.get("composite_health_score"),
        "symbols": idx.get("symbol_count"),
        "orphans": wir.get("orphan_count"),
    }


def diag_quality(scope):
    # Score the SCOPE directly — NO --workspace (which resolves to the ambient
    # workspace and scores the wrong tree — audit finding 2026-07-02).
    _, out, _e = run(
        ["touring-quality", "score", str(scope), "--format", "json"], timeout=1800)
    data = parse_json(out)
    if not data:
        return {"available": False}
    return {
        "available": True,
        "composite": data.get("composite"),
        "tier": data.get("tier"),
        "blockers": data.get("blockers"),
        "warnings": data.get("warnings"),
        "file_count": data.get("file_count"),
    }


def diag_wiring():
    _, out, _e = run(["touring", "wiring", "orphans", "-j"], timeout=60)
    data = parse_json(out)
    count = data.get("orphan_count", data.get("count")) if data else None
    return {"available": data is not None, "orphans": count}


def diag_memory(topic):
    _, out, _e = run(["touring", "memory", "recall", topic], timeout=30)
    data = parse_json(out)
    hits = []
    if data and isinstance(data.get("entries"), list):
        hits = [e.get("key") for e in data["entries"][:8] if isinstance(e, dict)]
    return {"topic": topic, "hits": hits}


def _fs_summary(scope: Path):
    """File-count + extension breakdown — the fallback structure for a scope
    that is not a Touring workspace (`touring map` returns nothing there)."""
    exts, files = {}, 0
    if scope.is_dir():
        for p in scope.rglob("*"):
            if p.is_file() and "__pycache__" not in p.parts:
                files += 1
                key = p.suffix or "(none)"
                exts[key] = exts.get(key, 0) + 1
    elif scope.is_file():
        files, exts = 1, {scope.suffix or "(none)": 1}
    top = dict(sorted(exts.items(), key=lambda kv: -kv[1])[:8])
    return {"files": files, "by_ext": top}


def diag_structure(scope):
    # Prefer `touring map` (workspace intel); fall back to a filesystem summary
    # for non-workspace scopes so `structure` is NEVER empty (ref-c 2026-07-02).
    rc, out, err = run(["touring", "map", str(scope)], timeout=120)
    raw = (out or err or "").strip()
    if rc == 0 and raw:
        return {"available": True, "source": "touring-map", "excerpt": raw[:800]}
    return {"available": True, "source": "fs-fallback", **_fs_summary(Path(scope))}


# ── Assembly + OKF emission ──────────────────────────────────────────────────
def diagnose(scope, topic):
    return {
        "scope": str(scope),
        "health": diag_health(),
        "quality50": diag_quality(scope),
        "wiring": diag_wiring(),
        "memory": diag_memory(topic),
        "structure": diag_structure(scope),
    }


def _slug(scope):
    return Path(scope).resolve().name or "workspace"


def plan_id_from_bundle(bundle: Path):
    idx = bundle / "index.md"
    if not idx.exists():
        return None
    for line in idx.read_text(errors="ignore").splitlines():
        if line.startswith("plan_id:"):
            return line.split(":", 1)[1].strip()
    return None


def write_okf_diagnostic(bundle: Path, plan_id, digest, ts):
    slug = f"{_slug(digest['scope'])}-{ts.replace(':', '').replace('-', '')[:15]}"
    path = bundle / "diagnostics" / f"{slug}.md"
    path.parent.mkdir(parents=True, exist_ok=True)
    q = digest["quality50"]
    h = digest["health"]
    fm = (
        "---\n"
        "type: Diagnostic\n"
        f"title: Diagnostic — {digest['scope']}\n"
        "description: One-shot deep diagnostic digest (health, 50-dim quality, wiring, memory, structure).\n"
        f"plan_id: {plan_id or 'unknown'}\n"
        "tags: [loop, diagnostic]\n"
        f"timestamp: {ts}\n"
        'okf_version: "0.1"\n'
        "---\n\n"
    )
    body = [
        f"# Diagnostic — `{digest['scope']}`",
        "",
        "Part of the [bundle](/index.md).",
        "",
        "## Schema",
        "",
        "| Signal | Value |",
        "|--------|-------|",
        f"| composite_health | {h.get('composite_health')} |",
        f"| symbols | {h.get('symbols')} |",
        f"| quality composite | {q.get('composite')} |",
        f"| quality tier | {q.get('tier')} |",
        f"| blockers | {q.get('blockers')} |",
        f"| warnings | {q.get('warnings')} |",
        f"| orphans | {digest['wiring'].get('orphans')} |",
        f"| memory hits | {len(digest['memory'].get('hits', []))} |",
        "",
        "## Citations",
        "",
        "- `touring status -j`, `touring-quality score --workspace`, "
        "`touring wiring orphans -j`, `touring memory recall`, `touring map`.",
    ]
    path.write_text(fm + "\n".join(body) + "\n")
    return str(path)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Loop Engineering one-shot diagnostic.")
    ap.add_argument("--scope", default=".", help="path to diagnose (default: cwd)")
    ap.add_argument("--topic", default="loop-engineering", help="memory recall topic")
    ap.add_argument("--bundle", default=None, help="OKF bundle dir — writes an OKF diagnostic doc")
    ap.add_argument("--plan-id", default=None, help="plan_id for the OKF doc (else read from bundle/index.md)")
    ap.add_argument("--json", action="store_true", help="emit JSON only")
    ap.add_argument("--quiet", action="store_true", help="no human output")
    args = ap.parse_args(argv)

    digest = diagnose(Path(args.scope), args.topic)

    written = None
    if args.bundle:
        bundle = Path(args.bundle)
        plan_id = args.plan_id or plan_id_from_bundle(bundle)
        ts = datetime.datetime.now().astimezone().isoformat()
        written = write_okf_diagnostic(bundle, plan_id, digest, ts)
        digest["okf_doc"] = written

    if args.json:
        print(json.dumps(digest, indent=2))
    elif not args.quiet:
        q, h = digest["quality50"], digest["health"]
        print(f"diagnostic · scope={digest['scope']}")
        print(f"  health   composite={h.get('composite_health')} symbols={h.get('symbols')}")
        print(f"  quality  composite={q.get('composite')} tier={q.get('tier')} "
              f"blockers={q.get('blockers')}")
        print(f"  wiring   orphans={digest['wiring'].get('orphans')}")
        print(f"  memory   hits={len(digest['memory'].get('hits', []))} (topic={digest['memory'].get('topic')})")
        print(f"  structure available={digest['structure'].get('available')}")
        if written:
            print(f"  → OKF diagnostic: {written}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
