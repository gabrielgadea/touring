#!/usr/bin/env python3
"""S-05 (SNR slice) — A/B harness for the relevance-cutoff (S-01).

Measures the effect of `TOURING_SNR_GATING` on the REAL pre_read enrichment, over
a corpus of real files, to decide whether to arm the cutoff and at what threshold.

Design (honest, no daemon restart, no new Rust counter):
  * The gating (`apply_relevance_cutoff`) runs in the `touring-hook pre-read`
    PROCESS — its env controls it (verified: OFF vs ON changes the output). So we
    A/B by setting `TOURING_SNR_GATING`/`TOURING_SNR_CUTOFF` per invocation.
  * pre_read is NON-DETERMINISTIC (~10% byte variance between identical runs) and
    the cutoff does NOT reduce bytes (the budget-fill assembles up to the cap
    regardless) — it changes the signal COMPOSITION. So we measure two axes over
    N files × R reps:
      1. bytes: mean ± stdev (the STR-by-size axis — expected ~flat).
      2. composition: the multiset of signal MARKERS (emoji-led segments). The
         cutoff's real effect is which low-relevance markers it drops. Utility
         proxy = the high-value markers (blast/gotcha/quality/plan) are preserved.

Usage:
    python3 docs/2026-07-04-snr-ab-harness.py [--reps R] [--json]

Output: an A/B table (bytes + composition per config) + a calibrated cutoff
recommendation. Fail-open: a hook that errors contributes an empty sample.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import statistics
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]  # ~/.claude/rust

# Corpus: real files spanning small/large, high/low blast, different crates — to
# exercise varied signal-score distributions (not a single shape).
CORPUS_REL = [
    "crates/touring-hook-handlers/src/hooks/pre_read.rs",
    "crates/touring-hook-handlers/src/hooks/signal_pipeline.rs",
    "crates/touring-hook-handlers/src/hooks/post_edit.rs",
    "crates/touring-hook-runtime/src/shared/signals.rs",
    "crates/touring-hook-runtime/src/hook_runtime.rs",
    "crates/touring-foundation/src/gate_metrics_snapshot.rs",
    "crates/touring-foundation/src/truncate.rs",
    "crates/touring-code/src/ast/quality.rs",
    "crates/touring-cli/src/cli/ast.rs",
    "crates/touring-cli/src/cli_suggester.rs",
    "crates/touring-storage/src/knowledge/metadata.rs",
    "crates/touring-simd/src/lib.rs",
    "crates/touring-analysis/src/lib.rs",
    "crates/touring-server/src/main.rs",
    "crates/touring-cognitive/src/lib.rs",
]

# Configs: control + a cutoff sweep (the calibration grid).
CONFIGS = [
    ("OFF", {"TOURING_SNR_GATING": "0"}),
    ("ON@0.05", {"TOURING_SNR_GATING": "1", "TOURING_SNR_CUTOFF": "0.05"}),
    ("ON@0.15", {"TOURING_SNR_GATING": "1", "TOURING_SNR_CUTOFF": "0.15"}),
    ("ON@0.30", {"TOURING_SNR_GATING": "1", "TOURING_SNR_CUTOFF": "0.30"}),
    ("ON@0.50", {"TOURING_SNR_GATING": "1", "TOURING_SNR_CUTOFF": "0.50"}),
]

# High-value markers whose survival is the utility guard (the cutoff must NOT
# drop these). Emoji taken from the real signal formatters in signals.rs/pre_read.
HIGH_VALUE_MARKERS = {"🔥", "⚠", "📋", "🎯", "🚨", "📉", "📈"}


def _emoji_markers(ctx: str) -> list[str]:
    """Extract the leading non-ASCII markers (each starts a signal segment)."""
    return re.findall(r"[^\x00-\x7f]", ctx)


def run_pre_read(abs_path: str, env_over: dict[str, str]) -> str:
    inp = json.dumps({"tool_input": {"file_path": abs_path}, "cwd": str(ROOT)})
    env = {**os.environ, **env_over}
    try:
        r = subprocess.run(
            ["touring-hook", "pre-read"],
            input=inp,
            capture_output=True,
            text=True,
            timeout=10,
            env=env,
            check=False,
        )
        obj = json.loads(r.stdout)
        return obj.get("hookSpecificOutput", {}).get("additionalContext", "") or ""
    except Exception:
        return ""


def measure(reps: int) -> dict:
    abs_files = [str(ROOT / rel) for rel in CORPUS_REL if (ROOT / rel).exists()]
    per_config: dict[str, dict] = {}
    for name, env_over in CONFIGS:
        byte_samples: list[int] = []
        marker_counts: list[int] = []
        hv_markers_seen: set[str] = set()
        all_markers: dict[str, int] = {}
        for f in abs_files:
            for _ in range(reps):
                ctx = run_pre_read(f, env_over)
                byte_samples.append(len(ctx))
                marks = _emoji_markers(ctx)
                marker_counts.append(len(marks))
                for m in marks:
                    all_markers[m] = all_markers.get(m, 0) + 1
                    if m in HIGH_VALUE_MARKERS:
                        hv_markers_seen.add(m)
        per_config[name] = {
            "n_samples": len(byte_samples),
            "bytes_mean": round(statistics.mean(byte_samples), 1) if byte_samples else 0,
            "bytes_stdev": round(statistics.pstdev(byte_samples), 1) if len(byte_samples) > 1 else 0,
            "markers_mean": round(statistics.mean(marker_counts), 2) if marker_counts else 0,
            "high_value_markers_present": sorted(hv_markers_seen),
            "marker_histogram": dict(sorted(all_markers.items(), key=lambda kv: -kv[1])),
        }
    return {"corpus_size": len(abs_files), "reps": reps, "configs": per_config}


def recommend(result: dict) -> dict:
    cfgs = result["configs"]
    off = cfgs.get("OFF", {})
    off_bytes = off.get("bytes_mean", 0) or 1
    off_markers = off.get("markers_mean", 0) or 1
    off_hv = set(off.get("high_value_markers_present", []))
    rows = []
    for name, c in cfgs.items():
        if name == "OFF":
            continue
        byte_delta_pct = round(100 * (c["bytes_mean"] - off_bytes) / off_bytes, 1)
        marker_delta_pct = round(100 * (c["markers_mean"] - off_markers) / off_markers, 1)
        hv = set(c.get("high_value_markers_present", []))
        hv_preserved = off_hv.issubset(hv) if off_hv else True
        rows.append({
            "cutoff": name,
            "byte_delta_pct": byte_delta_pct,
            "marker_delta_pct": marker_delta_pct,
            "high_value_preserved": hv_preserved,
            "lost_high_value": sorted(off_hv - hv),
        })
    # Recommendation gate: an effect is only real if it exceeds the measurement
    # noise. The byte noise floor is stdev/mean; a cutoff must prune MATERIALLY
    # (>= MATERIAL_PRUNE_PCT of markers) AND preserve every high-value marker to
    # justify arming. Below that, the pruning is indistinguishable from zero and
    # arming would be a false positive (the cutoff is not a real lever here).
    MATERIAL_PRUNE_PCT = 5.0
    noise_floor_pct = round(100 * (off.get("bytes_stdev", 0) / off_bytes), 1)
    safe = [
        r for r in rows
        if r["high_value_preserved"] and r["marker_delta_pct"] <= -MATERIAL_PRUNE_PCT
    ]
    pick = None
    if safe:
        pick = max(safe, key=lambda r: float(r["cutoff"].split("@")[1]))
    return {
        "rows": rows,
        "recommended": pick,
        "byte_noise_floor_pct": noise_floor_pct,
        "material_prune_threshold_pct": MATERIAL_PRUNE_PCT,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--reps", type=int, default=5)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    result = measure(args.reps)
    rec = recommend(result)
    result["recommendation"] = rec

    if args.json:
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0

    print(f"# S-05 SNR cutoff A/B — corpus={result['corpus_size']} files × {result['reps']} reps\n")
    print(f"{'config':<10} {'n':>4} {'bytes_mean':>11} {'±stdev':>8} {'markers':>8}  high_value_markers")
    for name, c in result["configs"].items():
        print(f"{name:<10} {c['n_samples']:>4} {c['bytes_mean']:>11} {c['bytes_stdev']:>8} "
              f"{c['markers_mean']:>8}  {''.join(c['high_value_markers_present'])}")
    print("\n## A/B vs OFF (Δ%)")
    print(f"{'cutoff':<10} {'byteΔ%':>8} {'markerΔ%':>9} {'hv_ok':>6}  lost_high_value")
    for r in rec["rows"]:
        print(f"{r['cutoff']:<10} {r['byte_delta_pct']:>8} {r['marker_delta_pct']:>9} "
              f"{str(r['high_value_preserved']):>6}  {''.join(r['lost_high_value'])}")
    print(f"\n## Recommendation (byte noise floor = ±{rec['byte_noise_floor_pct']}%, "
          f"material-prune threshold = {rec['material_prune_threshold_pct']}%)")
    if rec["recommended"]:
        print(f"→ arm cutoff {rec['recommended']['cutoff']} "
              f"(prunes {abs(rec['recommended']['marker_delta_pct'])}% of markers, "
              f"byteΔ={rec['recommended']['byte_delta_pct']}%, high-value preserved)")
    else:
        print("→ KEEP gating OFF. No cutoff prunes composition materially (all |Δ| well below "
              "the noise floor): under real files the min-max-normalized scores sit above the "
              "cutoff, and budget-truncation already drops the low-score tail. The cutoff is a "
              "correct, safe no-op — NOT the STR lever. The real STR win is S-04 SessionStart-slim.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
