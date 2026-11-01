#!/usr/bin/env python3
"""loop_outer_gate.py — deterministic artifact gate for a gated flow's OUTER phase.

Origin (2026-07-23, Gabriel): the loop-engineering Stop guard only engaged once a
marker with a real DAG task existed (written at step 11, AFTER the human gate) —
so the whole OUTER phase (steps 1-9: recall, deep diagnostic, CCE exploration,
strategy persistence) ran with no enforcement at all, and steps were skipped
silently. This gate closes that hole: it verifies the flow's manifest of
REQUIRED ARTIFACTS on disk (files + mtimes — never the orchestrator's narrative,
ADW Law L3) and reports what is missing with a concrete next_action.

Contract:
  * marker.status == "outer" (armed by loop_outer_arm.py at flow invocation)
  * marker.flow selects the manifest in flow_manifests.json (default strategy-outer)
  * an artifact counts only when its glob matches >= min files with
    mtime >= marker.created_at - SLACK (produced DURING this flow, not stale)
  * every evaluation appends one JSONL record to compliance.jsonl (the E3
    measurement feed for the touring.flow KPI)

Exit codes: 0 = complete OR not applicable (no outer marker / unknown flow —
fail-open); 1 = incomplete (missing artifacts listed in JSON). The caller
(loop_stop_guard.py) turns exit 1 into a Stop block; this script never blocks
by itself.

Usage:
  loop_outer_gate.py [--marker <path>] [--manifests <path>] [--json] [--no-emit]
"""
from __future__ import annotations

import argparse
import glob as globmod
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from loop_marker import MARKER_DIR, active_marker, _read  # noqa: E402

MANIFESTS = Path(__file__).resolve().parent / "flow_manifests.json"
COMPLIANCE_LOG = MARKER_DIR / "compliance.jsonl"
MTIME_SLACK_SECONDS = 120  # artifacts written moments before arming still count


def _resolve_glob(pattern: str, scope: str, bundle) -> str | None:
    """Fill {scope}/{bundle} placeholders; None when the pattern needs a bundle
    that the marker does not carry yet (the artifact then reports as missing)."""
    if "{bundle}" in pattern and not bundle:
        return None
    return pattern.replace("{scope}", scope or ".").replace("{bundle}", bundle or "")


def _fill(text: str, scope: str, bundle) -> str:
    return (text or "").replace("{scope}", scope or ".").replace(
        "{bundle}", bundle or "<bundle — run strategy-loop to register it>")


def _satisfies(path: str, require: dict | None) -> bool:
    """Is this file the artifact the flow owes, or merely a file of that shape?

    The glob answers "shape and freshness" and nothing else. On 2026-08-25 that
    let `{scope}/.touring-explore/*.ledger.json` be satisfied by a ledger for a
    DIFFERENT question whose exploration had NOT converged, while the converged
    ledger for the real topic sat one filename away — the gate reported
    `present` and would have let the turn end with the exploration open. A
    predicate that reads the file's own verdict is the difference between
    "a file exists" and "the work is done".

    Fail-CLOSED: a file we cannot read or parse has not proven anything.
    """
    if not require:
        return True
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return False
    for pointer, expected in require.items():
        node = data
        for part in pointer.split("."):
            if not isinstance(node, dict) or part not in node:
                return False
            node = node[part]
        if node != expected:
            return False
    return True


def _materialize_class_b(art: dict, scope: str, bundle, marker: dict) -> str | None:
    """Classe B: o artefato é SUBPRODUTO do turno, então quem o escreve é código.

    P4/S-4.3. A exigência continua sendo um arquivo em disco — a Lei L3 fica
    intacta, nada de "eu digo que raciocinei". O que muda é o AUTOR: em vez de o
    modelo parar no meio do raciocínio para escrever um `.md`, este gate roda no
    Stop hook (quando o turno JÁ terminou) e deriva o registro do que o turno
    produziu. Custo de contexto: zero, por construção.

    Fail-open sem exceção. Se o executor falha, o artefato não vira cobrança —
    porque a cobrança seria exatamente a escrita manual que a classe B existe
    para eliminar. A falha aparece no relatório como `class_b_unmaterialized`,
    nunca some (E4: ausência exibida).

    Exercitado em 04/09/2026, não presumido: com `materializer` inexistente a
    avaliação real devolveu `missing=[]`, `complete=True`,
    `class_b_unmaterialized=["turn-record"]`; com o executor bom, um registro de
    10.817 B em disco. Guard permanente:
    `test_class_b_never_becomes_a_manual_write_when_the_executor_fails`.
    """
    if art.get("materializer") != "loop_turn_record":
        return None
    try:
        import sys

        here = str(Path(__file__).resolve().parent)
        if here not in sys.path:
            sys.path.insert(0, here)
        from loop_turn_record import default_out, materialize

        out = default_out(scope, bundle, marker)
        # Idempotente por ARMAÇÃO: a segunda avaliação do mesmo run encontra o
        # arquivo da primeira. Sem isto o gate reescrevia a cada avaliação — o
        # diretório crescia sem limite E a cláusula ficava vacuamente verdadeira,
        # porque o artefato "aparecia" só por ter acabado de ser criado.
        if out.exists():
            return str(out)
        res = materialize({**marker, "scope": scope, "bundle": bundle}, out)
        return str(out) if res.get("written") and out.exists() else None
    except Exception:  # noqa: BLE001 — um executor de gate jamais derruba o turno
        return None


def evaluate(marker: dict, manifests: dict) -> dict:
    """Check every manifest artifact against disk; deterministic, narrative-free."""
    flow = marker.get("flow") or "strategy-outer"
    manifest = manifests.get(flow)
    if not isinstance(manifest, dict):
        return {"applicable": False, "flow": flow, "reason": "unknown flow — fail-open"}
    scope = marker.get("scope") or marker.get("cwd") or "."
    bundle = marker.get("bundle")
    # `flow_armed_at` when present (when THIS flow started); `created_at` only as
    # fallback for markers written before 20/08/2026. Using `created_at` let the
    # floor age with the marker instead of with the work.
    floor = float(marker.get("flow_armed_at")
                  or marker.get("created_at") or 0) - MTIME_SLACK_SECONDS
    missing, present = [], []
    # P4/S-4.1 — a CLASSE decide se o artefato e' cobravel neste flow. Um sem
    # `class` conta como A e continua exigido: esquecer de classificar erra para
    # o lado da cobranca, nunca para o do buraco silencioso.
    enforced = set(manifest.get("enforced_classes", ["A", "B", "C"]))
    unmaterialized = []
    for art in manifest.get("artifacts", []):
        cls = art.get("class", "A")
        if cls not in enforced:
            continue
        pattern = _resolve_glob(str(art.get("glob", "")), scope, bundle)
        hits = []
        if pattern:
            try:
                hits = [p for p in globmod.glob(pattern)
                        if Path(p).stat().st_mtime >= floor
                        and _satisfies(p, art.get("require_json"))]
            except Exception:  # noqa: BLE001 — unreadable path = no hit
                hits = []
        if len(hits) >= int(art.get("min", 1)):
            present.append({"id": art.get("id"), "files": sorted(hits)[-3:]})
        elif cls == "B":
            # Não cobra: PRODUZ. O turno já escreveu o conteúdo; o executor só o
            # materializa. Se falhar, o artefato sai do caminho crítico em vez
            # de virar a escrita manual que a classe B existe para eliminar.
            made = _materialize_class_b(art, scope, bundle, marker)
            if made:
                present.append({"id": art.get("id"), "files": [made]})
            else:
                unmaterialized.append(art.get("id"))
        else:
            missing.append({"id": art.get("id"),
                            "next_action": _fill(art.get("next_action", ""), scope, bundle)})
    complete = not missing
    next_action = (missing[0]["next_action"] if missing
                   else _fill(manifest.get("preferred_next", ""), scope, bundle))
    return {
        "applicable": True, "flow": flow, "complete": complete,
        "expected": sum(
            1
            for a in manifest.get("artifacts", [])
            if a.get("class", "A")
            in set(manifest.get("enforced_classes", ["A", "B", "C"]))
        ),
        "present": present, "missing": missing, "next_action": next_action,
        # Classe B que o executor não conseguiu escrever. Não bloqueia (seria a
        # escrita manual de volta), mas aparece — uma ausência que some é a
        # forma mais cara de erro num instrumento (E4).
        "class_b_unmaterialized": unmaterialized,
        "max_continuations": int(manifest.get("max_continuations", 5)),
    }


def emit_compliance(marker: dict, report: dict) -> None:
    """Append the E3 measurement record; one line per evaluation, fail-open."""
    try:
        COMPLIANCE_LOG.parent.mkdir(parents=True, exist_ok=True)
        record = {
            "ts": time.time(),
            "cwd": marker.get("cwd"),
            # Without this, concurrent sessions on one project were
            # indistinguishable in the KPI and their records read as one
            # erratic flow (defect #4, loop_marker.py).
            "session_id": marker.get("session_id"),
            "flow": report.get("flow"),
            "complete": bool(report.get("complete")),
            "expected": report.get("expected", 0),
            "missing": [m.get("id") for m in report.get("missing", [])],
            # Per-artifact gradient alongside the boolean (03/09/2026). `complete` is
            # an AND over the manifest, so a single artifact that can never be
            # satisfied automatically drags the whole series to False and the KPI stops
            # discriminating — it would answer "was the OUTER done?" with "did anyone
            # consult an external source?", which is a different question. Concretely:
            # `explore-ledger` requires verdict.converged, whose `external` lens is
            # MANUAL by design and which the background executor deliberately does not
            # waive on a human's behalf. Keeping the boolean preserves the historical
            # series; the gradient is what stays readable once the boolean saturates.
            "present_ids": [p.get("id") for p in report.get("present", [])],
            "satisfied": len(report.get("present", [])),
        }
        with COMPLIANCE_LOG.open("a") as fh:
            fh.write(json.dumps(record) + "\n")
        _trim_log()
    except Exception:  # noqa: BLE001 — measurement must never break the gate
        pass


def _trim_log(max_lines: int = 2000, keep: int = 1000) -> None:
    """Bound the compliance log: past `max_lines`, keep only the newest `keep`.
    The KPI is a recent-behavior meter, not an archive — unbounded growth was
    cross-audit finding F4 (2026-07-23)."""
    try:
        lines = COMPLIANCE_LOG.read_text().splitlines(keepends=True)
        if len(lines) > max_lines:
            COMPLIANCE_LOG.write_text("".join(lines[-keep:]))
    except Exception:  # noqa: BLE001
        pass


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Deterministic OUTER-phase artifact gate.")
    ap.add_argument("--marker", default=None,
                    help="explicit marker path (test override; bypasses cwd scoping)")
    ap.add_argument("--manifests", default=str(MANIFESTS))
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--no-emit", action="store_true",
                    help="skip the compliance.jsonl record (dry evaluation)")
    args = ap.parse_args(argv)

    if args.marker:
        marker = _read(Path(args.marker))
    else:
        _, marker = active_marker()
    if not marker or marker.get("status") != "outer":
        print(json.dumps({"applicable": False, "reason": "no outer marker"}))
        return 0

    manifests = _read(Path(args.manifests)) or {}
    report = evaluate(marker, manifests)
    if report.get("applicable") and not args.no_emit:
        emit_compliance(marker, report)
    print(json.dumps(report, indent=None if args.json else 2))
    if report.get("applicable") and not report.get("complete"):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
