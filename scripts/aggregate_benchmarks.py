#!/usr/bin/env python3
"""
Cross-language aggregation script for criterion benchmark results.

Discovers all *.csv files in target/criterion/reports/, parses timing data,
computes aggregates across benchmark groups, and outputs JSON report.

Usage:
    python3 aggregate_benchmarks.py [--baseline <path>] [--output <path>]
"""

import argparse
import csv
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional, Tuple

# Benchmark group classification patterns
KEYWORD_PATTERNS = [
    r"keyword_search_single",
    r"keyword_search_batch",
    r"keyword_latency_p95",
    r"hybrid_config_variations",  # Some hybrid configs test keyword variants
    r"rrf_fusion",  # RRF uses keyword as one leg
]

SEMANTIC_PATTERNS = [
    r"semantic_search",
    r"semantic_latency",
    r"semantic_throughput",
]

HYBRID_PATTERNS = [
    r"hybrid_search_single",
    r"hybrid_search_batch",
    r"hybrid_e2e",
    r"hybrid_latency",
    r"hybrid_throughput",
    r"hybrid_weight",
    r"hybrid_topk",
    r"hybrid_intent",
]

RRF_PATTERNS = [
    r"rrf_fusion",
    r"rrf_latency",
    r"rrf_weight",
]


def classify_benchmark(name: str) -> Optional[str]:
    """Classify a benchmark name into a category."""
    name_lower = name.lower()

    # Check RRF first (more specific)
    for pattern in RRF_PATTERNS:
        if re.search(pattern, name_lower):
            return "rrf_fusion"

    # Check keyword
    for pattern in KEYWORD_PATTERNS:
        if re.search(pattern, name_lower):
            return "keyword_only"

    # Check semantic
    for pattern in SEMANTIC_PATTERNS:
        if re.search(pattern, name_lower):
            return "semantic_only"

    # Check hybrid
    for pattern in HYBRID_PATTERNS:
        if re.search(pattern, name_lower):
            return "hybrid_e2e"

    return None


def parse_csv(csv_path: Path) -> Tuple[Optional[float], Optional[float], Optional[int]]:
    """
    Parse a criterion CSV file and extract mean_ns, p95_ns, and sample count.

    Returns: (mean_ns, p95_ns, sample_count)
    """
    try:
        with csv_path.open(newline="", encoding="utf-8") as f:
            reader = csv.DictReader(f)
            rows = list(reader)

        if not rows:
            return None, None, None

        # Find the data row (skip the header comment rows starting with '#')
        data_rows = [r for r in rows if r.get("method") and not str(r.get("method", "")).startswith("#")]

        if not data_rows:
            return None, None, None

        # Criterion CSV typically has one data row
        row = data_rows[0]

        mean_ns = _parse_float(row.get("mean_ns", ""))
        p95_ns = _parse_float(row.get("p95_ns", ""))

        # Sample count: estimate from stddev if not directly available
        stddev_ns = _parse_float(row.get("stddev_ns", ""))
        sample_count = _estimate_samples(row.get("sample_estimate", ""), stddev_ns)

        return mean_ns, p95_ns, sample_count

    except Exception:
        return None, None, None


def _parse_float(value: str) -> Optional[float]:
    """Parse a float value, handling empty/null cases."""
    if not value or value.strip() == "":
        return None
    try:
        return float(value.strip())
    except ValueError:
        return None


def _estimate_samples(sample_str: str, stddev_ns: Optional[float]) -> int:
    """Estimate sample count from CSV."""
    if sample_str and sample_str.strip():
        try:
            return int(float(sample_str.strip()))
        except ValueError:
            pass

    # Fallback: estimate from stddev if mean is available
    # Criterion typically runs 100-10000 samples depending on benchmark
    if stddev_ns is not None and stddev_ns > 0:
        # Rough heuristic: benchmarks with very low stddev often have more samples
        return 100  # conservative default
    return 1  # minimum


def aggregate_category(data: List[dict]) -> Dict:
    """
    Aggregate data for a category, computing mean and p95 across benchmarks.
    """
    if not data:
        return {"mean_ns": None, "p95_ns": None, "samples": 0}

    means = [d["mean_ns"] for d in data if d.get("mean_ns") is not None]
    p95s = [d["p95_ns"] for d in data if d.get("p95_ns") is not None]
    sample_counts = [d.get("sample_count", 1) for d in data if d.get("sample_count") is not None]

    result = {
        "mean_ns": sum(means) / len(means) if means else None,
        "p95_ns": sum(p95s) / len(p95s) if p95s else None,
        "samples": sum(sample_counts) if sample_counts else len(data),
    }

    return result


def compute_speedups(aggregates: Dict) -> Dict:
    """Compute speedup ratios between benchmark categories."""
    speedups = {}

    keyword_mean = aggregates.get("keyword_only", {}).get("mean_ns")
    semantic_mean = aggregates.get("semantic_only", {}).get("mean_ns")
    hybrid_mean = aggregates.get("hybrid_e2e", {}).get("mean_ns")

    if keyword_mean and hybrid_mean and keyword_mean > 0:
        speedups["hybrid_vs_keyword"] = keyword_mean / hybrid_mean

    if semantic_mean and hybrid_mean and semantic_mean > 0:
        speedups["hybrid_vs_semantic"] = semantic_mean / hybrid_mean

    return speedups


def compare(baseline: dict, current: dict) -> dict:
    """
    Compare two benchmark aggregation reports and compute delta percentages.

    Args:
        baseline: Baseline benchmark report JSON
        current: Current benchmark report JSON

    Returns:
        Delta report with percentage changes per category
    """
    deltas = {
        "benchmark_deltas": {},
        "speedup_deltas": {},
        "notes": [],
    }

    # Compare benchmark categories
    for category in ["keyword_only", "semantic_only", "hybrid_e2e", "rrf_fusion"]:
        baseline_val = baseline.get("benchmarks", {}).get(category, {}).get("mean_ns")
        current_val = current.get("benchmarks", {}).get(category, {}).get("mean_ns")

        if baseline_val is not None and current_val is not None and baseline_val > 0:
            pct_change = ((current_val - baseline_val) / baseline_val) * 100
            deltas["benchmark_deltas"][category] = {
                "baseline_ns": baseline_val,
                "current_ns": current_val,
                "delta_percent": round(pct_change, 2),
                "faster": pct_change < 0,  # Negative delta means faster (lower ns)
            }
        elif current_val is not None:
            deltas["benchmark_deltas"][category] = {
                "baseline_ns": None,
                "current_ns": current_val,
                "delta_percent": None,
                "faster": None,
            }
            deltas["notes"].append(f"{category}: no baseline data")

    # Compare speedups
    for speedup_key in ["hybrid_vs_keyword", "hybrid_vs_semantic"]:
        baseline_speedup = baseline.get("speedups", {}).get(speedup_key)
        current_speedup = current.get("speedups", {}).get(speedup_key)

        if baseline_speedup and current_speedup and baseline_speedup > 0:
            pct_change = ((current_speedup - baseline_speedup) / baseline_speedup) * 100
            deltas["speedup_deltas"][speedup_key] = {
                "baseline": baseline_speedup,
                "current": current_speedup,
                "delta_percent": round(pct_change, 2),
            }
        elif current_speedup is not None:
            deltas["speedup_deltas"][speedup_key] = {
                "baseline": None,
                "current": current_speedup,
                "delta_percent": None,
            }
            deltas["notes"].append(f"{speedup_key}: no baseline data")

    return deltas


def run_aggregation(
    reports_dir: Path,
    output_path: Optional[Path] = None,
    baseline_path: Optional[Path] = None,
) -> dict:
    """
    Main aggregation function.

    Args:
        reports_dir: Path to criterion reports directory (e.g., target/criterion/reports/)
        output_path: Optional path to write JSON report
        baseline_path: Optional path to baseline JSON for comparison

    Returns:
        Aggregation report dictionary
    """
    # Discover CSV files
    if not reports_dir.exists():
        raise FileNotFoundError(f"Reports directory not found: {reports_dir}")

    csv_files = list(reports_dir.glob("**/*.csv"))

    if not csv_files:
        raise ValueError(f"No CSV files found in {reports_dir}")

    # Parse and classify each CSV
    categorized: Dict[str, List[dict]] = {
        "keyword_only": [],
        "semantic_only": [],
        "hybrid_e2e": [],
        "rrf_fusion": [],
    }
    unclassified = []

    notes = []

    for csv_file in csv_files:
        # Extract benchmark name from filename (remove CSV extension)
        # Criterion generates names like: "keyword_search_single-5ab4f3/input.csv"
        # or just "benchmark_name.csv"
        benchmark_name = csv_file.stem

        # If in subdirectory, include parent name
        if csv_file.parent.name != "reports":
            # Include subdirectory for context
            benchmark_name = f"{csv_file.parent.name}/{benchmark_name}"

        category = classify_benchmark(benchmark_name)

        mean_ns, p95_ns, sample_count = parse_csv(csv_file)

        entry = {
            "file": str(csv_file.relative_to(reports_dir)),
            "benchmark": benchmark_name,
            "mean_ns": mean_ns,
            "p95_ns": p95_ns,
            "sample_count": sample_count,
        }

        if category:
            categorized[category].append(entry)
        else:
            unclassified.append(entry)

    # Aggregate each category
    aggregates = {}
    for category, entries in categorized.items():
        if entries:
            aggregates[category] = aggregate_category(entries)
        else:
            aggregates[category] = {"mean_ns": None, "p95_ns": None, "samples": 0}

    # Compute speedups
    speedups = compute_speedups(aggregates)

    # Build report
    report = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "benchmarks": aggregates,
        "speedups": speedups,
        "notes": notes,
        "_meta": {
            "csv_files_processed": len(csv_files),
            "unclassified_benchmarks": [u["benchmark"] for u in unclassified],
            "category_counts": {k: len(v) for k, v in categorized.items()},
        },
    }

    # Add comparison with baseline if provided
    if baseline_path and baseline_path.exists():
        try:
            with baseline_path.open() as f:
                baseline_data = json.load(f)
            report["comparison"] = compare(baseline_data, report)
        except Exception as e:
            report["notes"].append(f"Failed to load baseline: {e}")

    # Write output
    if output_path:
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with output_path.open("w", encoding="utf-8") as f:
            json.dump(report, f, indent=2, ensure_ascii=False)

    return report


def main():
    parser = argparse.ArgumentParser(
        description="Aggregate criterion benchmark results from CSV files."
    )
    parser.add_argument(
        "--reports",
        type=Path,
        default=Path("target/criterion/reports"),
        help="Path to criterion reports directory",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Path to output JSON report",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=None,
        help="Path to baseline JSON for comparison",
    )
    parser.add_argument(
        "--cwd",
        type=Path,
        default=None,
        help="Working directory (default: current directory)",
    )

    args = parser.parse_args()

    # Resolve reports directory relative to cwd if not absolute
    reports_dir = args.reports
    if not reports_dir.is_absolute():
        base = args.cwd or Path.cwd()
        reports_dir = base / reports_dir

    # Default output to reports/benchmark_aggregation.json relative to cwd
    if args.output is None:
        base = args.cwd or Path.cwd()
        args.output = base / "reports" / "benchmark_aggregation.json"

    output_path = args.output
    if not output_path.is_absolute():
        base = args.cwd or Path.cwd()
        output_path = base / output_path

    baseline_path = args.baseline
    if baseline_path and not baseline_path.is_absolute():
        base = args.cwd or Path.cwd()
        baseline_path = base / baseline_path

    try:
        report = run_aggregation(reports_dir, output_path, baseline_path)

        # Print summary to stdout
        print(f"Aggregated {report['_meta']['csv_files_processed']} benchmark files")
        for category in ["keyword_only", "semantic_only", "hybrid_e2e", "rrf_fusion"]:
            agg = report["benchmarks"].get(category, {})
            mean_ns = agg.get("mean_ns")
            p95_ns = agg.get("p95_ns")
            samples = agg.get("samples", 0)
            if mean_ns is not None:
                print(f"  {category}: mean={mean_ns:.0f}ns, p95={p95_ns:.0f}ns, n={samples}")
            else:
                print(f"  {category}: no data")

        if report.get("speedups"):
            print("\nSpeedups:")
            for k, v in report["speedups"].items():
                print(f"  {k}: {v:.2f}x")

        print(f"\nFull report written to: {output_path}")

    except Exception as e:
        print(f"Error: {e}", file=__import__("sys").stderr)
        raise


if __name__ == "__main__":
    main()