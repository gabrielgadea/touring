#!/usr/bin/env python3
"""ground_truth_collector — Stage-1 unified Touring sweep.

Executes the canonical sequence of Touring commands in parallel and merges
their JSON outputs into a single `ground_truth.json` envelope consumed by
every later script in the toolkit.

When the daemon is down, the collector falls back to grep / cargo / ast.parse
and flags `daemon_degraded: true`. The plan is still authored, but every
affected claim gets downgraded by confidence_tagger.

Usage
-----
    python3 ground_truth_collector.py --intent "implement async write-back cache"
    python3 ground_truth_collector.py --intent "..." --output data/ground_truth.json
    python3 ground_truth_collector.py --intent "..." --no-cache -j
"""

from __future__ import annotations

import argparse
import json
import logging
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from lib import (  # noqa: E402  pylint: disable=wrong-import-position
    EXIT_FAIL,
    EXIT_INTERRUPTED,
    EXIT_OK,
    EXIT_STRUCTURAL,
    cache_get,
    cache_put,
    compute_intent_cache_key,
    extract_paths_from_intent,
    extract_symbols_from_intent,
    run_touring,
    touring_available,
    utcnow_iso,
    write_json_atomic,
)


def _collect_doctor() -> tuple[dict[str, Any], bool]:
    """Returns (parsed_doctor, daemon_degraded_flag)."""
    parsed = run_touring(["doctor", "-j"]) or {}
    if not isinstance(parsed, list):
        return {"raw": parsed}, True
    degraded = any(component.get("status") != "ok" for component in parsed)
    return {"components": parsed}, degraded


def _collect_status() -> dict[str, Any]:
    return run_touring(["status", "-j"]) or {}


def _collect_e2e() -> dict[str, Any]:
    return run_touring(["e2e", "--depth", "standard", "-j"]) or {}


def _collect_wiring_audit() -> dict[str, Any]:
    return run_touring(["wiring", "audit", "-j"]) or {}


def _collect_wiring_orphans() -> list[dict[str, Any]]:
    res = run_touring(["wiring", "orphans", "-j"]) or {}
    if isinstance(res, list):
        return res
    if isinstance(res, dict):
        return res.get("orphans", []) or []
    return []


def _collect_evolution_drift() -> dict[str, Any]:
    return run_touring(["evolution", "drift", "-j"]) or {}


def _collect_memory_lessons(keywords: str, limit: int = 10) -> list[dict[str, Any]]:
    if not keywords.strip():
        return []
    res = run_touring(["memory", "recall", keywords], timeout=10) or {}
    entries = res.get("entries", []) if isinstance(res, dict) else []
    return entries[:limit]


def _collect_gotchas(paths: list[str]) -> dict[str, list[dict[str, Any]]]:
    out: dict[str, list[dict[str, Any]]] = {}
    for path in paths:
        res = run_touring(["gotcha", "match", path, "-j"], timeout=5)
        if isinstance(res, dict) and res.get("matches"):
            out[path] = res["matches"]
        elif isinstance(res, list):
            out[path] = res
    return out


def _verify_symbol(symbol: str) -> dict[str, Any]:
    """Run `touring index find <S>` and reduce to a VerifiedSymbol dict."""
    res = run_touring(["index", "find", symbol, "-j"], timeout=5)
    if not res:
        return {"name": symbol, "verified": False, "suggestion": ""}
    hits: list[dict[str, Any]] = []
    if isinstance(res, list):
        hits = res
    elif isinstance(res, dict):
        hits = res.get("hits", []) or res.get("symbols", []) or []
    if not hits:
        return {"name": symbol, "verified": False, "suggestion": ""}
    first = hits[0]
    return {
        "name": symbol,
        "verified": True,
        "file": str(first.get("file_path", first.get("file", ""))),
        "line": int(first.get("line", 0)),
        "signature": str(first.get("signature", "")),
    }


def _grep_fallback(symbol: str, root: Path) -> dict[str, Any]:
    """Daemon-down fallback for symbol verification."""
    try:
        result = subprocess.run(
            ["grep", "-rn", "-m", "1",
             "--include=*.rs", "--include=*.py", "--include=*.ts",
             symbol, str(root)],
            capture_output=True, text=True, timeout=5, check=False,
        )
        if result.returncode == 0 and result.stdout.strip():
            first_line = result.stdout.split("\n", 1)[0]
            parts = first_line.split(":", 2)
            if len(parts) >= 2:
                return {
                    "name": symbol,
                    "verified": True,
                    "file": parts[0],
                    "line": int(parts[1]) if parts[1].isdigit() else 0,
                    "signature": parts[2].strip() if len(parts) > 2 else "",
                }
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass
    return {"name": symbol, "verified": False, "suggestion": ""}


def _collect_ast_overview(path: str) -> dict[str, Any]:
    res = run_touring(["ast", "overview", path, "-j"], timeout=10) or {}
    if isinstance(res, dict):
        return res
    return {}


def _collect_ast_blast(path: str) -> dict[str, Any]:
    res = run_touring(["ast", "blast", path, "-j"], timeout=10) or {}
    if isinstance(res, dict):
        return res
    return {}


def collect_ground_truth(
    intent: str,
    *,
    workers: int = 8,
    timeout_seconds: int = 30,
    workspace_root: Path | None = None,
) -> dict[str, Any]:
    """Execute the full Stage-1 sweep in parallel.

    Returns the full envelope ready to be written as ``ground_truth.json``.
    """
    started = time.monotonic()
    symbols = extract_symbols_from_intent(intent)
    paths = extract_paths_from_intent(intent)
    keywords = " ".join(symbols + paths)[:200]
    workspace = workspace_root or Path.cwd()
    daemon_up = touring_available()

    doctor_data, daemon_degraded = _collect_doctor() if daemon_up else (
        {"error": "touring CLI unavailable"}, True,
    )

    futures_map: dict[Any, str] = {}
    collected: dict[str, Any] = {
        "doctor": doctor_data,
        "status_snapshot": {},
        "e2e": {},
        "wiring_audit": {},
        "wiring_orphans": [],
        "evolution_drift": {},
        "memory_lessons": [],
        "gotcha_per_file": {},
        "vgp_verifications": [],
        "ast_overviews": {},
        "ast_blasts": {},
    }

    if not daemon_up:
        # Fallback: only grep-based VGP. Skip Touring commands entirely.
        collected["vgp_verifications"] = [
            _grep_fallback(s, workspace) for s in symbols
        ]
    else:
        with ThreadPoolExecutor(max_workers=workers) as executor:
            futures_map[executor.submit(_collect_status)] = "status_snapshot"
            futures_map[executor.submit(_collect_e2e)] = "e2e"
            futures_map[executor.submit(_collect_wiring_audit)] = "wiring_audit"
            futures_map[executor.submit(_collect_wiring_orphans)] = "wiring_orphans"
            futures_map[executor.submit(_collect_evolution_drift)] = "evolution_drift"
            futures_map[executor.submit(_collect_memory_lessons, keywords)] = "memory_lessons"
            futures_map[executor.submit(_collect_gotchas, paths)] = "gotcha_per_file"
            sym_futures = {executor.submit(_verify_symbol, s): s for s in symbols}
            overview_futures = {executor.submit(_collect_ast_overview, p): p for p in paths}
            blast_futures = {executor.submit(_collect_ast_blast, p): p for p in paths}

            for future in as_completed(list(futures_map.keys())):
                try:
                    collected[futures_map[future]] = future.result(timeout=timeout_seconds)
                except Exception as exc:  # noqa: BLE001
                    logging.getLogger(__name__).debug("collect failed: %s", exc)

            vgp: list[dict[str, Any]] = []
            for future in as_completed(sym_futures):
                try:
                    vgp.append(future.result(timeout=timeout_seconds))
                except Exception:  # noqa: BLE001
                    vgp.append({"name": sym_futures[future],
                                "verified": False, "suggestion": ""})
            collected["vgp_verifications"] = vgp

            ast_overviews: dict[str, Any] = {}
            for future in as_completed(overview_futures):
                try:
                    ast_overviews[overview_futures[future]] = future.result(timeout=timeout_seconds)
                except Exception:  # noqa: BLE001
                    ast_overviews[overview_futures[future]] = {}
            collected["ast_overviews"] = ast_overviews

            ast_blasts: dict[str, Any] = {}
            for future in as_completed(blast_futures):
                try:
                    ast_blasts[blast_futures[future]] = future.result(timeout=timeout_seconds)
                except Exception:  # noqa: BLE001
                    ast_blasts[blast_futures[future]] = {}
            collected["ast_blasts"] = ast_blasts

    duration_ms = int((time.monotonic() - started) * 1000)
    verified_count = sum(1 for v in collected["vgp_verifications"] if v.get("verified"))
    summary = {
        "verified_symbols": verified_count,
        "unverified_symbols": len(collected["vgp_verifications"]) - verified_count,
        "gotchas_found": sum(len(v) for v in collected["gotcha_per_file"].values()),
        "lessons_applied": len(collected["memory_lessons"]),
        "orphan_count": len(collected["wiring_orphans"]),
        "paths_inspected": len(paths),
    }

    envelope: dict[str, Any] = {
        "status": "OK" if not daemon_degraded else "DEGRADED",
        "script": "ground_truth_collector",
        "timestamp": utcnow_iso(),
        "intent": intent,
        "duration_ms": duration_ms,
        "daemon_degraded": daemon_degraded,
        "extracted": {"symbols": symbols, "paths": paths},
        "summary": summary,
        **collected,
    }
    return envelope


# ── CLI ───────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="ground_truth_collector", description=__doc__)
    parser.add_argument("--intent", required=True,
                        help="Free-form intent string (drives symbol/path extraction).")
    parser.add_argument("--output", type=Path, default=Path("data/ground_truth.json"),
                        help="Destination for the JSON envelope.")
    parser.add_argument("--workspace", type=Path, default=None,
                        help="Workspace root (used by grep fallback). Default: cwd.")
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--per-command-timeout", type=int, default=30)
    parser.add_argument("--cache-ttl", type=int, default=600,
                        help="Cache TTL in seconds (set 0 to disable caching).")
    parser.add_argument("--no-cache", action="store_true",
                        help="Skip cache lookup; always sweep fresh.")
    parser.add_argument("--apply", action="store_true",
                        help="No-op (collector is read-only by design).")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Cache lookup → sweep → write envelope."""
    cache_key = compute_intent_cache_key(args.intent)
    if not args.no_cache and args.cache_ttl > 0:
        cached = cache_get(cache_key, ttl_seconds=args.cache_ttl)
        if cached:
            cached["_from_cache"] = True
            write_json_atomic(args.output, cached)
            return cached

    envelope = collect_ground_truth(
        args.intent,
        workers=args.workers,
        timeout_seconds=args.per_command_timeout,
        workspace_root=args.workspace,
    )
    write_json_atomic(args.output, envelope)
    if args.cache_ttl > 0:
        cache_put(cache_key, envelope)

    envelope["output_path"] = str(args.output)
    return envelope


def main() -> int:
    """CLI entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False, default=str) + "\n")
        return EXIT_OK if result.get("status") in {"OK", "DEGRADED"} else EXIT_FAIL
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except FileNotFoundError as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("ground_truth_collector failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
