#!/usr/bin/env python3
"""Bundled system health diagnostic for the /Touring skill (TACO FASE 0 gate).

Aggregates the five Tier-2 diagnostic commands into a single traffic-light
report: ``doctor``, ``status``, ``gate-metrics``, ``learning status``, and
``evolution drift``. Used as the FASE 0 health gate by the TACO orchestrator
and as a quick standalone diagnostic by the user.

Exit codes:
  0  GREEN  — all components healthy, RL converging, drift level ``none``
  1  YELLOW — degraded but operable (warnings present)
  2  RED    — blocking issue (daemon down, db unhealthy, structural drift)

Usage:
  diagnose_health.py                   # human report
  diagnose_health.py --json            # machine JSON
  diagnose_health.py --metrics         # include full gate-metrics dump
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_json, emit_kv, emit_result, emit_section, emit_table, mark_degraded,
    require_touring, touring_run, traffic_light,
)

CRITICAL_COMPONENTS = {"knowledge_db", "symbol_store"}


def gather(timeout: float, *, include_metrics: bool) -> dict[str, Any]:
    report: dict[str, Any] = {"signals": {}}

    doctor = touring_run(["doctor", "-j"], timeout=timeout)
    report["signals"]["doctor"] = {"ok": doctor.ok, "data": doctor.parsed}

    status = touring_run(["status", "-j"], timeout=timeout)
    report["signals"]["status"] = {"ok": status.ok, "data": status.parsed}

    learning = touring_run(["learning", "status"], timeout=timeout, parse_json=False)
    report["signals"]["learning"] = {"ok": learning.ok, "text": learning.stdout}

    drift = touring_run(["evolution", "drift", "-j"], timeout=timeout)
    report["signals"]["drift"] = {"ok": drift.ok, "data": drift.parsed}

    if include_metrics:
        metrics = touring_run(["gate-metrics", "-j"], timeout=timeout)
        report["signals"]["gate_metrics"] = {"ok": metrics.ok, "data": metrics.parsed}

    report["verdict"] = score_verdict(report["signals"])
    return report


def score_verdict(signals: dict[str, Any]) -> dict[str, Any]:
    reasons: list[str] = []
    warnings: list[str] = []

    doctor_data = (signals.get("doctor") or {}).get("data") or []
    blocking_components: list[str] = []
    degraded_components: list[str] = []
    if isinstance(doctor_data, list):
        for c in doctor_data:
            if not isinstance(c, dict):
                continue
            name = c.get("name", "?")
            stat = c.get("status", "?")
            if stat in ("error", "unhealthy", "down", "fail"):
                if name in CRITICAL_COMPONENTS:
                    blocking_components.append(name)
                    reasons.append(f"doctor: critical component '{name}' is {stat}")
                else:
                    degraded_components.append(name)
                    warnings.append(f"doctor: '{name}' is {stat}")
            elif stat == "warning":
                degraded_components.append(name)
                warnings.append(f"doctor: '{name}' warning")
    else:
        reasons.append("doctor: no JSON returned — daemon likely down")
        blocking_components.append("daemon")

    status_data = (signals.get("status") or {}).get("data") or {}
    composite = float(status_data.get("composite_health_score") or 0.0) if isinstance(status_data, dict) else 0.0
    if composite > 0 and composite < 0.5:
        reasons.append(f"composite_health_score={composite:.3f} < 0.5")
    elif composite > 0 and composite < 0.7:
        warnings.append(f"composite_health_score={composite:.3f} below 0.7 target")

    drift_data = (signals.get("drift") or {}).get("data") or {}
    drift_level = drift_data.get("alert_level") if isinstance(drift_data, dict) else None
    if drift_level == "structural":
        reasons.append("evolution drift: structural alert level")
    elif drift_level == "degraded":
        warnings.append("evolution drift: degraded alert level")

    if blocking_components or reasons:
        decision = "RED"
        exit_code = 2
    elif warnings or degraded_components:
        decision = "YELLOW"
        exit_code = 1
    else:
        decision = "GREEN"
        exit_code = 0

    return {
        "decision": decision,
        "exit_code": exit_code,
        "composite_health_score": composite,
        "drift_alert_level": drift_level,
        "blocking_components": blocking_components,
        "degraded_components": degraded_components,
        "reasons": reasons,
        "warnings": warnings,
        "light": traffic_light(composite or 0.0),
    }


def emit_human(report: dict[str, Any]) -> None:
    v = report["verdict"]
    emit_section(f"TOURING SYSTEM HEALTH  ({v['decision']})")
    emit_kv("decision", v["decision"])
    emit_kv("exit_code", v["exit_code"])
    emit_kv("composite_health_score", f"{v['composite_health_score']:.3f}")
    emit_kv("drift_alert_level", v.get("drift_alert_level") or "(unknown)")

    doctor_data = (report["signals"].get("doctor") or {}).get("data") or []
    if isinstance(doctor_data, list) and doctor_data:
        emit_section("doctor components", char="-")
        rows = [[c.get("name", "?"), c.get("status", "?"),
                 (c.get("detail") or "")[:60]]
                for c in doctor_data if isinstance(c, dict)]
        emit_table(rows, headers=["component", "status", "detail"])

    status_data = (report["signals"].get("status") or {}).get("data") or {}
    if isinstance(status_data, dict):
        emit_section("status signals", char="-")
        index = status_data.get("index") or {}
        wiring = status_data.get("wiring") or {}
        learning = status_data.get("learning") or {}
        emit_kv("index.symbol_count", index.get("symbol_count", "?"))
        emit_kv("wiring.orphan_count", wiring.get("orphan_count", "?"))
        emit_kv("learning.ema_reward", learning.get("ema_reward", "?"))
        if "health_delta" in status_data:
            hd = status_data["health_delta"]
            if isinstance(hd, dict):
                emit_kv("health_delta.regressions", hd.get("regression_count", 0))

    if v["reasons"]:
        emit_section("blockers", char="-")
        for r in v["reasons"]:
            print(f"  RED   {r}")
    if v["warnings"]:
        emit_section("warnings", char="-")
        for w in v["warnings"]:
            print(f"  YEL   {w}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--metrics", action="store_true",
                        help="Include full gate-metrics dump (verbose)")
    add_common_args(parser)
    args = parser.parse_args()

    require_touring()
    report = gather(timeout=args.timeout, include_metrics=args.metrics)
    # I-fail-soft: a top-level degraded flag so a consumer never reads a
    # degraded-daemon health verdict as authoritative.
    mark_degraded(report, bool(report["verdict"].get("degraded_components"))
                  or report["verdict"]["decision"] == "RED")
    # I-density: --brief (compact digest) > --json (full); else human fallback.
    if not emit_result(report, args):
        emit_human(report)
    return int(report["verdict"]["exit_code"])


if __name__ == "__main__":
    sys.exit(main())
