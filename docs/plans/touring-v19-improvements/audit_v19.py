#!/usr/bin/env python3
"""
audit_v19.py — Cross-audit script for Touring v19 Improvements.

Verifies all 10 insights are implemented. Reports ROI evidence.
Usage: python audit_v19.py [--verbose]
"""
import argparse
import shlex
import subprocess
import sys
from pathlib import Path

WORKSPACE = Path("/home/gabrielgadea/.claude/rust")
PLAN_DIR = Path(__file__).parent

PASS = "\033[92mPASS\033[0m"
FAIL = "\033[91mFAIL\033[0m"


def run_cmd(cmd: str) -> tuple[int, str]:
    # OWASP A03 guard: never invoke subprocess with shell=True on a
    # user-controlled string. Here `cmd` is a hard-coded literal at the
    # call-site (cargo test, cargo check, etc.), but the proper idiom is
    # `shlex.split` → list-of-args, which is what shell=True was being
    # used as a shortcut for. Switch to that explicitly so future
    # edits cannot accidentally interpolate user input.
    args = shlex.split(cmd)
    r = subprocess.run(args, shell=False, capture_output=True, text=True, cwd=WORKSPACE)
    return r.returncode, r.stdout + r.stderr


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    print("=== Touring v19 Cross-Audit ===")
    print(f"Workspace: {WORKSPACE}")
    print()

    # Run cargo test once to get test output
    print("Running cargo test (this may take ~30s)...")
    _, cargo_test_output = run_cmd(
        "cargo test --workspace --exclude touring-python 2>&1"
    )

    results: dict[str, dict] = {}

    # INS-1: TemplateLibrary with Learning (ROI=2.00)
    results["INS-1"] = {
        "title": "TemplateLibrary with Learning",
        "crate": "touring-learning",
        "module": "src/aco/template_library.rs",
        "file_exists": Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/template_library.rs").exists(),
        "test_present": "test_record_template_new" in cargo_test_output if "test_record_template_new" else True,
        "roi": 2.0,
    }
    results["INS-1"]["pass"] = (
        results["INS-1"]["file_exists"] and
        results["INS-1"]["test_present"]
    )

    # INS-2: GoalTracker 9×9 Computational (ROI=1.60)
    results["INS-2"] = {
        "title": "GoalTracker 9×9 Computational",
        "crate": "touring-learning",
        "module": "src/aco/tracker.rs",
        "file_exists": Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/tracker.rs").exists(),
        "test_present": "test_9x9_matrix_completeness" in cargo_test_output if "test_9x9_matrix_completeness" else True,
        "roi": 1.6,
    }
    results["INS-2"]["pass"] = (
        results["INS-2"]["file_exists"] and
        results["INS-2"]["test_present"]
    )

    # INS-3: Plugin/Phase Registry Dynamic (ROI=1.50)
    results["INS-3"] = {
        "title": "Plugin/Phase Registry Dynamic",
        "crate": "touring-learning",
        "module": "src/aco/registry.rs",
        "file_exists": Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/registry.rs").exists(),
        "test_present": "test_registry_register_and_execute" in cargo_test_output if "test_registry_register_and_execute" else True,
        "roi": 1.5,
    }
    results["INS-3"]["pass"] = (
        results["INS-3"]["file_exists"] and
        results["INS-3"]["test_present"]
    )

    # INS-4: Deterministic Topological Sort + Graph Analytics (ROI=1.67)
    results["INS-4"] = {
        "title": "Deterministic Topological Sort + Graph Analytics",
        "crate": "touring-learning",
        "module": "src/aco/graph.rs",
        "file_exists": Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/graph.rs").exists(),
        "test_present": "test_topo_sort_deterministic_simple" in cargo_test_output if "test_topo_sort_deterministic_simple" else True,
        "roi": 1.67,
    }
    results["INS-4"]["pass"] = (
        results["INS-4"]["file_exists"] and
        results["INS-4"]["test_present"]
    )

    # INS-5: CQRS Read Model (ROI=1.40)
    results["INS-5"] = {
        "title": "CQRS Read Model",
        "crate": "touring-learning",
        "module": "src/aco/read_model.rs",
        "file_exists": Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/read_model.rs").exists(),
        "test_present": "test_read_model_apply_and_get" in cargo_test_output if "test_read_model_apply_and_get" else True,
        "roi": 1.4,
    }
    results["INS-5"]["pass"] = (
        results["INS-5"]["file_exists"] and
        results["INS-5"]["test_present"]
    )

    # INS-6: Saga Pattern with Compensating Transactions (ROI=1.40)
    results["INS-6"] = {
        "title": "Saga Pattern with Compensating Transactions",
        "crate": "touring-learning",
        "module": "src/aco/saga.rs",
        "file_exists": Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/saga.rs").exists(),
        "test_present": "test_saga_all_steps_succeed" in cargo_test_output if "test_saga_all_steps_succeed" else True,
        "roi": 1.4,
    }
    results["INS-6"]["pass"] = (
        results["INS-6"]["file_exists"] and
        results["INS-6"]["test_present"]
    )

    # INS-7: Parallel Generator Engine (ROI=1.33)
    results["INS-7"] = {
        "title": "Parallel Generator Engine",
        "crate": "touring-learning",
        "module": "src/aco/graph.rs",
        "file_exists": Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/graph.rs").exists(),
        "test_present": "test_parallel_execute_independent_nodes" in cargo_test_output if "test_parallel_execute_independent_nodes" else True,
        "roi": 1.33,
    }
    results["INS-7"]["pass"] = (
        results["INS-7"]["file_exists"] and
        results["INS-7"]["test_present"]
    )

    # INS-8: ESAA Complete 24 Subsystems (ROI=1.13)
    results["INS-8"] = {
        "title": "ESAA Complete 24 Subsystems",
        "crate": "touring-learning",
        "module": "src/aco/esaa.rs",
        "file_exists": Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/esaa.rs").exists(),
        "test_present": "test_esaa_coordinator_register_all_24" in cargo_test_output if "test_esaa_coordinator_register_all_24" else True,
        "roi": 1.13,
    }
    results["INS-8"]["pass"] = (
        results["INS-8"]["file_exists"] and
        results["INS-8"]["test_present"]
    )

    # INS-9: Time-Travel Debugging (ROI=1.17)
    results["INS-9"] = {
        "title": "Time-Travel Debugging",
        "crate": "touring-learning",
        "module": "src/aco/time_travel.rs",
        "file_exists": Path("/home/gabrielgadea/.claude/rust/crates/touring-learning/src/aco/time_travel.rs").exists(),
        "test_present": "test_capture_state_returns_epoch" in cargo_test_output if "test_capture_state_returns_epoch" else True,
        "roi": 1.17,
    }
    results["INS-9"]["pass"] = (
        results["INS-9"]["file_exists"] and
        results["INS-9"]["test_present"]
    )

    # INS-10: Agent State Machine Complete (ROI=1.20)
    results["INS-10"] = {
        "title": "Agent State Machine Complete",
        "crate": "touring-cognitive",
        "module": "src/agent_state_machine.rs",
        "file_exists": Path("/home/gabrielgadea/.claude/rust/crates/touring-cognitive/src/agent_state_machine.rs").exists(),
        "test_present": "test_state_machine_starts_idle" in cargo_test_output if "test_state_machine_starts_idle" else True,
        "roi": 1.2,
    }
    results["INS-10"]["pass"] = (
        results["INS-10"]["file_exists"] and
        results["INS-10"]["test_present"]
    )


    # Print results
    print("\n--- Insight Audit ---")
    all_pass = True
    for ins_id, r in results.items():
        status = PASS if r["pass"] else FAIL
        print(f"  {status}: {ins_id} — {r['title']} (ROI={r['roi']:.2f})")
        if not r["pass"] and args.verbose:
            print(f"    file_exists: {r['file_exists']}")
            print(f"    test_present: {r['test_present']}")
        if not r["pass"]:
            all_pass = False

    # Test count check
    print("\n--- Test Suite ---")
    test_line = [l for l in cargo_test_output.splitlines() if "test result" in l]
    if test_line:
        print(f"  {test_line[-1].strip()}")
        if "0 failed" in test_line[-1]:
            print(f"  {PASS}: 0 failed")
        else:
            print(f"  {FAIL}: failures detected")
            all_pass = False
    else:
        print(f"  {FAIL}: Could not parse test output")
        all_pass = False

    # Clippy check
    print("\n--- Clippy ---")
    code, _ = run_cmd("cargo clippy --workspace -- -D warnings 2>&1")
    clippy_status = PASS if code == 0 else FAIL
    print(f"  {clippy_status}: cargo clippy --workspace -D warnings")
    if code != 0:
        all_pass = False

    # Summary
    passed = sum(1 for r in results.values() if r["pass"])
    total = len(results)
    print(f"\n==================================================")
    print(f"INSIGHTS: {passed}/{total} PASS")
    print(f"OVERALL: {'ALL PASS' if all_pass else 'FAILURES DETECTED'}")
    return 0 if all_pass else 1


if __name__ == "__main__":
    sys.exit(main())
