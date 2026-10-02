#!/usr/bin/env python3
"""w45_production_report — relatório de produção do intent→CILA (S-4.5).

Lê os journals de produção (intent_journal do S-3.2, jev_journal do adapter,
recall_journal do S-3.4) e produz o relatório do W4.4: distribuição de intent,
rotas (laya × keyword × skipped), fallback rate, confiança por fonte, latência.
Determinístico e fail-open: qualquer journal ausente vira seção "unmeasured",
nunca zero disfarçado. Regenerável a qualquer dia — o relatório de 7 dias é
este mesmo comando rodado sobre uma janela maior.
"""

from __future__ import annotations

import json
import statistics
import sys
import time
from collections import Counter
from pathlib import Path

LOGS = Path("~/.claude/touring/logs").expanduser()
OUT = Path("docs/plans/2026-09-30-jev-harness-potencializacao/experiments/w45-production")
DAYS = 7


def read_jsonl(path: Path) -> list[dict]:
    if not path.exists():
        return []
    out = []
    for line in path.read_text().splitlines():
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return out


def ms_to_day(ms: float) -> str:
    return time.strftime("%Y-%m-%d", time.localtime(ms / 1000))


def intent_section(lines: list[dict]) -> dict:
    cutoff = time.time() - DAYS * 86400
    window = [l for l in lines if (l.get("ts_ms", 0) / 1000) >= cutoff]
    if not window:
        return {"status": "unmeasured", "reason": "intent_journal vazio ou fora da janela"}
    routes = Counter(l.get("source", "?") for l in window)
    intents = Counter(l.get("intent") for l in window if not l.get("skipped_automated"))
    skipped = sum(1 for l in window if l.get("skipped_automated"))
    per_day = Counter(ms_to_day(l.get("ts_ms", 0)) for l in window)
    laya_conf = [l["confidence"] for l in window if l.get("source") == "laya" and isinstance(l.get("confidence"), (int, float))]
    return {
        "status": "measured",
        "n": len(window),
        "routes": dict(routes),
        "keyword_fallback_rate": round(routes.get("keyword", 0) / max(1, routes.get("keyword", 0) + routes.get("laya", 0)), 4),
        "intent_distribution": dict(intents),
        "skipped_automated": skipped,
        "per_day": dict(sorted(per_day.items())),
        "laya_confidence": {
            "n": len(laya_conf),
            "mean": round(statistics.mean(laya_conf), 4) if laya_conf else None,
            "p50": round(statistics.median(laya_conf), 4) if laya_conf else None,
        },
    }


def jev_section(lines: list[dict]) -> dict:
    cutoff = time.time() - DAYS * 86400
    api = [l for l in lines if l.get("route_taken") == "api" and _ts(l) >= cutoff]
    if not lines:
        return {"status": "unmeasured", "reason": "jev_journal vazio"}
    lat = sorted(l.get("latency_ms", 0) for l in api) or [0]
    confs = [
        d.get("confidence") or d.get("max_prob")
        for l in lines
        for d in (l.get("answers_digest") or {}).values()
        if isinstance(d.get("confidence") or d.get("max_prob"), (int, float))
    ]
    return {
        "status": "measured",
        "calls_api_window": len(api),
        "calls_total": sum(1 for l in lines if l.get("route_taken") == "api"),
        "cache_hits": sum(1 for l in lines if l.get("route_taken") == "cache"),
        "p50_ms": round(statistics.median(lat), 1),
        "p99_ms": round(lat[max(0, int(0.99 * len(lat)) - 1)], 1) if len(lat) > 1 else lat[0],
        "confidence_mean": round(statistics.mean(confs), 4) if confs else None,
    }


def _ts(line: dict) -> float:
    raw = line.get("ts", "")
    try:
        return time.mktime(time.strptime(raw[:19], "%Y-%m-%dT%H:%M:%S"))
    except (ValueError, TypeError):
        return 0.0


def recall_section(lines: list[dict]) -> dict:
    if not lines:
        return {"status": "unmeasured", "reason": "recall_journal vazio (S-3.4 entrou em produção com a 30.4.89)"}
    cand = sum(l.get("candidates", 0) for l in lines)
    inj = sum(l.get("injected", 0) for l in lines)
    return {
        "status": "measured",
        "sessions": len(lines),
        "candidates": cand,
        "injected": inj,
        "curated_share": round(inj / cand, 4) if cand else None,
    }


def main() -> int:
    intent = intent_section(read_jsonl(LOGS / "intent_journal.jsonl"))
    jev = jev_section(read_jsonl(LOGS / "jev_journal.jsonl"))
    recall = recall_section(read_jsonl(LOGS / "recall_journal.jsonl"))
    report = {
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "window_days": DAYS,
        "flag_state": "TOURING_LAYA_INTENT opt-in — o braço laya só existe quando a flag liga em produção",
        "intent": intent,
        "jev": jev,
        "recall": recall,
        "notes": [
            "day-0: o braço laya entra em produção com o opt-in de Gabriel; até lá, source=keyword (baseline honesto)",
            "o relatório de 7 dias é este mesmo comando — regenerar em 08/10/2026 para a janela completa",
        ],
    }
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "results.json").write_text(json.dumps(report, ensure_ascii=False, indent=1))
    print(json.dumps(report, ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
