#!/usr/bin/env python3
"""
audit_implementation.py — Cross-implementation Audit for Touring Improvements.

Verifies that ALL 13 insights from the plan are implemented correctly.
For each insight, checks: file exists, symbol exists, tests pass.

Usage:
    python audit_implementation.py              # Full audit
    python audit_implementation.py --verbose    # Detailed output
    python audit_implementation.py --dry-run    # Show what would be checked
    python audit_implementation.py --insight ACO-1  # Audit single insight
"""
from __future__ import annotations

import argparse
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import NamedTuple

WORKSPACE = Path("/home/gabrielgadea/.claude/rust")


class InsightCheck(NamedTuple):
    """Definition of what to check for a single insight."""

    id: str
    title: str
    crate: str
    file: str  # relative to crate dir
    symbols: list[str]  # functions/structs/enums to verify
    test_filter: str  # cargo test filter


# All 13 insights with their verification targets.
# File paths and symbols verified via VGP (generate_plan.py VGP_SCHEMAS).
INSIGHT_CHECKS: list[InsightCheck] = [
    InsightCheck(
        "ACO-1", "4-Layer Diagnostic Engine",
        "touring-learning", "src/aco/diagnostics.rs",
        ["DiagnosticLayer", "classify_error", "suggest_retry_strategy"],
        "diagnostics",
    ),
    InsightCheck(
        "ACO-2", "EvolutionPackage Population Logic",
        "touring-learning", "src/aco/evolution.rs",
        ["populate_evolution_package", "extract_learned_patterns",
         "discover_anti_patterns", "propose_system_upgrades"],
        "evolution",
    ),
    InsightCheck(
        "ACO-3", "Auto-decompose by Complexity",
        "touring-learning", "src/aco/graph.rs",
        ["auto_decompose"],
        "auto_decompose",
    ),
    InsightCheck(
        "ACO-4", "Execution Tracking State Machine",
        "touring-learning", "src/aco/graph.rs",
        ["transition_status", "ready_to_execute", "execution_report"],
        "transition_status",
    ),
    InsightCheck(
        "ACO-5", "ObjectiveHash verify_invariant()",
        "touring-learning", "src/aco/graph.rs",
        ["verify_invariant", "compute_objective_hash"],
        "verify_invariant",
    ),
    InsightCheck(
        "AST-1", "FileHeat Prioritized Indexing",
        "touring-ast", "src/file_heat.rs",
        ["FileHeat", "HeatMap", "record_edit", "get_priority_order"],
        "file_heat",
    ),
    InsightCheck(
        "AST-2", "find_symbols_fused() with RRF",
        "touring-ast", "src/semantic_search.rs",
        ["find_symbols_fused"],
        "find_symbols_fused",
    ),
    InsightCheck(
        "AST-3", "EnrichedBlastRadius with Impact Categories",
        "touring-ast", "src/graph.rs",
        ["EnrichedBlastRadius", "ImpactCategory", "compute_enriched_blast_radius"],
        "enriched_blast_radius",
    ),
    InsightCheck(
        "COG-1", "CognitiveMCTS",
        "touring-cognitive", "src/cognitive_mcts.rs",
        ["CognitiveMCTS", "cognitive_search"],
        "cognitive_mcts",
    ),
    InsightCheck(
        "COG-2", "RefinementCycle",
        "touring-cognitive", "src/refinement.rs",
        ["RefinementCycle", "run_refinement", "RefinementOutcome"],
        "refinement",
    ),
    InsightCheck(
        "COG-3", "Adaptive Temporal Decay",
        "touring-cognitive", "src/semantic_graph.rs",
        ["adaptive_half_life"],
        "adaptive",
    ),
    InsightCheck(
        "COG-4", "GoT Multi-dimensional Evaluation",
        "touring-cognitive", "src/got.rs",
        ["evaluate_multidimensional"],
        "multidim",
    ),
    InsightCheck(
        "COG-5", "TransitionMatrix + QTable Persistence",
        "touring-cognitive", "src/persistence.rs",
        ["transition_matrix", "q_values"],
        "snapshot",
    ),
]


@dataclass
class AuditResult:
    """Result of auditing a single insight."""

    insight_id: str
    title: str
    file_exists: bool
    symbols_found: dict[str, bool]
    tests_pass: bool | None  # None = not run (dry-run or file missing)
    detail: str

    @property
    def passed(self) -> bool:
        return (
            self.file_exists
            and all(self.symbols_found.values())
            and self.tests_pass is not False
        )


def check_file_exists(check: InsightCheck) -> tuple[bool, str]:
    """Check if the target file exists."""
    path = WORKSPACE / "crates" / check.crate / check.file
    if path.exists():
        return True, f"EXISTS: {path.relative_to(WORKSPACE)}"
    return False, f"NOT FOUND: {path.relative_to(WORKSPACE)}"


def check_symbols(check: InsightCheck) -> dict[str, bool]:
    """Check if all expected symbols exist in the file."""
    path = WORKSPACE / "crates" / check.crate / check.file
    results: dict[str, bool] = {}
    if not path.exists():
        return {s: False for s in check.symbols}

    try:
        content = path.read_text(encoding="utf-8")
    except OSError:
        return {s: False for s in check.symbols}

    for symbol in check.symbols:
        results[symbol] = symbol in content

    return results


def run_tests(check: InsightCheck) -> tuple[bool, str]:
    """Run cargo test for the insight's test filter."""
    try:
        result = subprocess.run(
            ["cargo", "test", "-p", check.crate, "--", check.test_filter],
            capture_output=True, text=True,
            cwd=str(WORKSPACE), timeout=300,
        )
        if result.returncode == 0:
            # Extract test count from output
            for line in result.stdout.split("\n"):
                if "test result:" in line:
                    return True, line.strip()
            return True, "PASSED"
        # Extract failure info
        stderr_tail = result.stderr[-500:] if result.stderr else ""
        stdout_tail = result.stdout[-500:] if result.stdout else ""
        return False, f"FAILED:\n{stdout_tail}\n{stderr_tail}"
    except subprocess.TimeoutExpired:
        return False, "TIMEOUT (300s)"
    except Exception as e:
        return False, f"ERROR: {e}"


def audit_insight(check: InsightCheck, *, dry_run: bool = False, verbose: bool = False) -> AuditResult:
    """Audit a single insight."""
    file_ok, file_detail = check_file_exists(check)
    symbols_found = check_symbols(check)
    tests_pass: bool | None = None
    test_detail = ""

    if not dry_run and file_ok:
        tests_pass, test_detail = run_tests(check)
    elif dry_run:
        test_detail = "[DRY RUN] would run: cargo test -p {} -- {}".format(
            check.crate, check.test_filter
        )

    # Build combined detail
    parts = [f"  FILE: {file_detail}"]
    for sym, found in symbols_found.items():
        if verbose or not found:
            status = "FOUND" if found else "MISSING"
            parts.append(f"  SYMBOL: {sym} -> {status}")
    if test_detail:
        parts.append(f"  TEST: {test_detail}")

    return AuditResult(
        insight_id=check.id,
        title=check.title,
        file_exists=file_ok,
        symbols_found=symbols_found,
        tests_pass=tests_pass,
        detail="\n".join(parts),
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Audit implementation of 13 Touring insights"
    )
    parser.add_argument("--verbose", "-v", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--insight", type=str, default=None, help="Audit single insight (e.g. ACO-1)")
    parser.add_argument("--json", action="store_true", help="Output as JSON")
    args = parser.parse_args()

    # Filter insights if requested
    checks = INSIGHT_CHECKS
    if args.insight:
        checks = [c for c in INSIGHT_CHECKS if c.id == args.insight.upper()]
        if not checks:
            print(f"ERROR: Insight '{args.insight}' not found.", file=sys.stderr)
            sys.exit(1)

    print("=" * 60)
    print("  Touring Improvements — Implementation Audit")
    print("=" * 60)

    results: list[AuditResult] = []
    for check in checks:
        result = audit_insight(check, dry_run=args.dry_run, verbose=args.verbose)
        results.append(result)

        status = "PASS" if result.passed else "FAIL"
        marker = "+" if result.passed else "X"
        print(f"\n  [{marker}] {result.insight_id}: {result.title} -> {status}")
        if args.verbose or not result.passed:
            print(result.detail)

    # Summary
    passed = sum(1 for r in results if r.passed)
    total = len(results)
    print("\n" + "=" * 60)
    print(f"  SCORE: {passed}/{total}")

    if passed == total:
        print("  VERDICT: ALL INSIGHTS IMPLEMENTED")
    else:
        failed = [r.insight_id for r in results if not r.passed]
        print(f"  VERDICT: {len(failed)} INSIGHTS PENDING: {', '.join(failed)}")

    print("=" * 60)

    if args.json:
        import json
        data = {
            "score": f"{passed}/{total}",
            "passed": passed,
            "total": total,
            "results": [
                {
                    "id": r.insight_id,
                    "title": r.title,
                    "passed": r.passed,
                    "file_exists": r.file_exists,
                    "symbols_found": r.symbols_found,
                    "tests_pass": r.tests_pass,
                }
                for r in results
            ],
        }
        print(json.dumps(data, indent=2))

    sys.exit(0 if passed == total else 1)


if __name__ == "__main__":
    main()
