#!/usr/bin/env python3
"""loop_diagnose.py — one-shot deep diagnostic for the Loop Engineering loop.

Composes the Touring intelligence commands into a single digest so the loop's
OUTER phase (steps 1-4: recall + deep diagnostic + overview) is ONE call, not N.
Each sub-diagnostic is best-effort (fail-open); an OKF-compliant diagnostic
document is written into the bundle when ``--bundle`` is given.

Sub-diagnostics (TWO LANES — quality50 runs parallel to the four
daemon-backed calls, which stay sequential among themselves):
  health    — touring status -j            (composite health, symbols, orphans)
  quality50 — touring-quality score <scope> (composite, tier, blockers, warnings)
  wiring    — touring wiring orphans -j     (orphan count)
  memory    — touring memory recall <topic> (prior context — top keys)
  structure — touring map <scope>           (workspace structure, best-effort)

Measured cost (27/08/2026, scope=analise — 8.790 files / 2.3M LOC):
  quality50 ≈ 84s (dominant, scales with repo size) · memory ≈ 15s ·
  health ≈ 1.2s · wiring ≈ 0.15s · structure(fs-fallback) ≈ 2s.
Sequential that is ~102s — three sessions died to a caller-side `timeout 90`
(mid-quality-score). Concurrent it is ≈ max(84s, 15s, …) ≈ 86s.
**Caller guidance: use a timeout ≥ 150s for repo-scale scopes** — or bound the
quality pass with --quality-timeout and accept `quality50.available=false`
(the digest stays useful; fail-open by design).

Usage:
    loop_diagnose.py --scope <path> [--topic <str>] [--bundle <dir>]
                     [--plan-id <id>] [--quality-timeout <s>] [--json] [--quiet]
"""
from __future__ import annotations

import argparse
import datetime
import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path


def _import_okf_emit():
    """O emissor único de documentos OKF (arsenal compartilhado) — todo
    frontmatter deste script sai dele (D8: campo ausente impossível por
    construção). Caminho relativo vale no live E no espelho client/;
    falha LOUD — sem o executor seria voltar à convenção."""
    here = Path(__file__).absolute()
    candidatos = [
        here.parents[2] / "Touring" / "scripts",
        here.resolve().parents[2] / "Touring" / "scripts",
        Path.home() / ".claude" / "skills" / "Touring" / "scripts",
    ]
    for c in candidatos:
        if (c / "okf_emit.py").is_file():
            if str(c) not in sys.path:
                sys.path.insert(0, str(c))
            import okf_emit as mod
            return mod
    raise ImportError(
        "okf_emit.py não encontrado — procurado em: "
        + ", ".join(str(c) for c in candidatos)
    )


okf_emit = _import_okf_emit()


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


def diag_quality(scope, timeout=300):
    # Score the SCOPE directly — NO --workspace (which resolves to the ambient
    # workspace and scores the wrong tree — audit finding 2026-07-02).
    # Bounded by `timeout` (default 300s; repo-scale measured 84s on 27/08/2026):
    # a timeout must degrade THIS sub-diagnostic, never kill the whole digest.
    _, out, _e = run(
        ["touring-quality", "score", str(scope), "--format", "json"], timeout=timeout)
    data = parse_json(out)
    if not data:
        return {"available": False, "reason": f"no JSON after up to {timeout}s"}
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
    _, out, err = run(["touring", "memory", "recall", topic], timeout=30)
    data = parse_json(out)
    if data and isinstance(data.get("entries"), list):
        hits = [e.get("key") for e in data["entries"][:8] if isinstance(e, dict)]
        return {"topic": topic, "hits": hits}
    # Fail-open, mas honesto: uma falha do daemon NÃO é "zero memórias" —
    # leitura fiel: ausência de sinal nunca vira zero. O digest carrega o
    # motivo para o operador saber que o sinal está degradado, não vazio.
    reason = None
    if (err or "").find("budget") >= 0:
        reason = "daemon handler budget exceeded (recall >15s — consulta lenta)"
    elif (err or "").strip():
        reason = (err or "").strip().splitlines()[-1][:160]
    elif not (out or "").strip():
        reason = "empty-output"
    else:
        reason = "unparseable-output"
    return {"topic": topic, "hits": [], "degraded": reason}


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
def diagnose(scope, topic, quality_timeout=300):
    """Run the sub-diagnostics in TWO LANES and assemble the digest.

    Lane 1 is ``diag_quality`` alone (the standalone ``touring-quality``
    binary, ~84s repo-scale — the long pole). Lane 2 runs the four
    daemon-backed calls SEQUENTIALLY (~19s total): the project daemon
    serializes requests on one actor, and firing them concurrently starves
    the 15s handler budget — measured 27/08/2026: full-parallel returned
    ``wiring.available=false`` and 0 memory hits; the two-lane shape keeps
    the same ~84s wall clock with ALL signals present.
    """
    with ThreadPoolExecutor(max_workers=2) as pool:
        f_quality = pool.submit(diag_quality, scope, quality_timeout)
        health = diag_health()
        wiring = diag_wiring()
        memory = diag_memory(topic)
        structure = diag_structure(scope)
        return {
            "scope": str(scope),
            "health": health,
            "quality50": f_quality.result(),
            "wiring": wiring,
            "memory": memory,
            "structure": structure,
        }


def _slug(scope):
    return Path(scope).resolve().name or "workspace"


def plan_id_from_bundle(bundle: Path):
    """Resolve the bundle's plan_id: index.md frontmatter, else the dir name.

    The directory-name fallback matters because the deterministic OUTER writes
    its diagnostic BEFORE index.md exists (the ADW's `diagnose` node runs first),
    which stamped every diagnostic `plan_id: unknown` and then made
    loop_doc_link_gate.py report a contradiction against the bundle's own id.
    The bundle layout IS `docs/plans/<plan_id>/`, so the dir name is the
    authoritative answer whenever the frontmatter cannot supply one.
    """
    idx = bundle / "index.md"
    if idx.exists():
        for line in idx.read_text(errors="ignore").splitlines():
            if line.startswith("plan_id:"):
                found = line.split(":", 1)[1].strip()
                if found:
                    return found
    name = bundle.resolve().name
    return name or None


def ensure_bundle_log(bundle: Path, plan_id, ts) -> Path:
    """Create the bundle's ``log.md`` if absent — never overwrite an existing one.

    The bundle's chronological leg was dead code: ``loop_snapshot`` only appends
    ``if log.exists()`` and NOTHING ever created the file, so every PreCompact
    resume note was silently dropped (no bundle on disk had a log.md on
    2026-08-02). The diagnostic is where a bundle is born — the OUTER's first
    artifact — so that is where the log starts.
    """
    log = bundle / "log.md"
    if log.exists():
        return log
    try:
        bundle.mkdir(parents=True, exist_ok=True)
        log.write_text(
            okf_emit.render_frontmatter(
                "Log", "Log — chronological history of this loop run",
                "Append-only history; PreCompact resume notes and phase closes land here.",
                timestamp=ts, tags=["loop", "log"],
                fields={"plan_id": plan_id or bundle.resolve().name,
                        "okf_version": "0.1"},
            )
            + "# Log\n\nPart of the [bundle](/index.md).\n",
            encoding="utf-8",
        )
    except Exception:  # noqa: BLE001 — a missing log must never fail the diagnostic
        pass
    return log


def write_okf_diagnostic(bundle: Path, plan_id, digest, ts):
    slug = f"{_slug(digest['scope'])}-{ts.replace(':', '').replace('-', '')[:15]}"
    path = bundle / "diagnostics" / f"{slug}.md"
    path.parent.mkdir(parents=True, exist_ok=True)
    ensure_bundle_log(bundle, plan_id, ts)
    q = digest["quality50"]
    h = digest["health"]
    fm = okf_emit.render_frontmatter(
        "Diagnostic", f"Diagnostic — {digest['scope']}",
        "One-shot deep diagnostic digest (health, 50-dim quality, wiring, memory, structure).",
        timestamp=ts, tags=["loop", "diagnostic"],
        fields={"plan_id": plan_id or "unknown", "okf_version": "0.1"},
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
        f"| memory hits | {len(digest['memory'].get('hits', []))}"
        + (f" (degraded: {digest['memory']['degraded']})" if digest["memory"].get("degraded") else "")
        + " |",
        "",
        "## Citations",
        "",
        "- `touring status -j`, `touring-quality score --workspace`, "
        "`touring wiring orphans -j`, `touring memory recall`, `touring map`.",
    ]
    path.write_text(fm + "\n".join(body) + "\n")
    # O diagnóstico entra no grafo como artefato facetado (#artifact:diagnostic)
    # — sem isso recall/moc só alcançam o resumo, nunca o documento (furo
    # medido 30/08/2026). Fail-open: daemon mudo nunca gata o diagnóstico.
    okf_emit.register_artifact(
        path, f"report:diagnostic:{slug}", f"Diagnostic — {digest['scope']}",
        facets=["#artifact:diagnostic", "#process:loop"],
    )
    return str(path)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Loop Engineering one-shot diagnostic.")
    ap.add_argument("--scope", default=".", help="path to diagnose (default: cwd)")
    ap.add_argument("--topic", default="loop-engineering", help="memory recall topic")
    ap.add_argument("--bundle", default=None, help="OKF bundle dir — writes an OKF diagnostic doc")
    ap.add_argument("--plan-id", default=None, help="plan_id for the OKF doc (else read from bundle/index.md)")
    ap.add_argument("--quality-timeout", type=int, default=300,
                    help="cap the 50-dim quality pass at N seconds (default: 300; "
                         "repo-scale measured 84s on 27/08/2026). On expiry the "
                         "digest carries quality50.available=false — fail-open.")
    ap.add_argument("--json", action="store_true", help="emit JSON only")
    ap.add_argument("--quiet", action="store_true", help="no human output")
    args = ap.parse_args(argv)

    digest = diagnose(Path(args.scope), args.topic, args.quality_timeout)

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
