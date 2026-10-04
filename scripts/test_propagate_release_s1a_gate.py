#!/usr/bin/env python3
"""Guards for the S1a gate in `scripts/propagate-release.sh` (r3, XA-08/XA-13,
2026-10-03; decision A approved by Gabriel).

The defect it closes (cross-audit 03/10/2026, finding D): the S1a benchmark
`r3_emission_cost_is_bounded` is `#[ignore]` (load-sensitive, so it stays out of
the ordinary `cargo test`), and nothing ran it. A cost regression in the shadow
emitter would have passed the next propagation unseen. `scripts/r3-s1a-gate`
runs it under `hw-slot` as part of gate 1/6.

Three properties, each provable only by running the gate:

  · **Not blind.** `cargo test -- --ignored <filter>` exits 0 when the filter
    matches NOTHING (the test was renamed). The gate must require the summary
    line `test result: ok. 1 passed`; exit 0 without it is a failure.
  · **One re-measurement, no more.** Thermal noise (the machine has sat at
    90-96 C) becomes a delay; a real regression fails twice. A second retry would
    turn the gate into a lottery.
  · **The limit is the test's own.** The gate never loosens it: this file holds no
    number, and the pre-registered criterion (Errata 5, E5.2) is untouched.

Meta by design: this file's name matches `test_propagate_release*.py`, so gate
1/6 runs it on every propagation.
"""

from __future__ import annotations

import os
import re
import stat
import subprocess
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
GATE = REPO / "scripts" / "r3-s1a-gate"
RELEASE = REPO / "scripts" / "propagate-release.sh"
SHADOW_RS = REPO / "crates" / "touring-hook-runtime" / "src" / "r3_shadow.rs"
TEST_NAME = "r3_emission_cost_is_bounded"

OK_LINE = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 55 filtered out"
BLIND_LINE = "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 56 filtered out"
FAIL_LINE = "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 55 filtered out"


def _fake_runner(tmp: Path, attempts: list[tuple[int, str]]) -> Path:
    """A stand-in for `hw-slot --class rust -- cargo`: attempt N prints and exits
    as `attempts[N-1]`, and every call is counted in `calls`."""
    plan = tmp / "plan"
    plan.write_text("\n".join(f"{rc}\t{line}" for rc, line in attempts) + "\n", encoding="utf-8")
    runner = tmp / "fake-runner"
    runner.write_text(
        "#!/usr/bin/env bash\n"
        f"calls='{tmp}/calls'\n"
        "n=$(( $(cat \"$calls\" 2>/dev/null || echo 0) + 1 ))\n"
        "echo \"$n\" > \"$calls\"\n"
        f"line=$(sed -n \"${{n}}p\" '{plan}')\n"
        "rc=${line%%$'\\t'*}\n"
        "echo \"${line#*$'\\t'}\"\n"
        "exit \"$rc\"\n",
        encoding="utf-8",
    )
    runner.chmod(runner.stat().st_mode | stat.S_IXUSR)
    return runner


def run_gate(attempts: list[tuple[int, str]]) -> tuple[int, int, str]:
    """-> (gate exit code, how many times the runner was invoked, combined output)."""
    tmp = Path(tempfile.mkdtemp(prefix="s1a-gate-"))
    runner = _fake_runner(tmp, attempts)
    env = {**os.environ, "S1A_RUNNER": str(runner), "WORKSPACE": str(tmp)}
    proc = subprocess.run(
        [str(GATE)], env=env, capture_output=True, text=True, timeout=60, check=False
    )
    calls_file = tmp / "calls"
    calls = int(calls_file.read_text().strip()) if calls_file.exists() else 0
    return proc.returncode, calls, proc.stdout + proc.stderr


# ── behavioral: the three properties ────────────────────────────────────────

def test_passes_on_the_first_attempt_without_a_second_run():
    rc, calls, _ = run_gate([(0, OK_LINE)])
    assert (rc, calls) == (0, 1)


def test_one_thermal_blip_is_absorbed_by_exactly_one_re_measurement():
    rc, calls, out = run_gate([(101, FAIL_LINE), (0, OK_LINE)])
    assert (rc, calls) == (0, 2), out


def test_a_real_regression_fails_twice_and_is_not_run_a_third_time():
    rc, calls, out = run_gate([(101, FAIL_LINE), (101, FAIL_LINE), (0, OK_LINE)])
    assert rc != 0, "two failures must fail the gate"
    assert calls == 2, f"retry-once means two runs, got {calls}: a third turns the gate into a lottery"
    assert "duas vezes" in out, "the refusal must say it failed twice"


def test_exit_zero_without_a_passed_test_is_a_blind_gate_and_fails():
    rc, calls, out = run_gate([(0, BLIND_LINE), (0, BLIND_LINE)])
    assert rc != 0, "cargo exits 0 when the filter matches nothing; that must not read as green"
    assert calls == 2
    assert "filtro não casou" in out, "the refusal must name the blindness"


def test_a_blind_first_run_that_recovers_passes():
    rc, calls, _ = run_gate([(0, BLIND_LINE), (0, OK_LINE)])
    assert (rc, calls) == (0, 2)


def test_a_passed_test_does_not_excuse_a_nonzero_exit():
    """hw-slot exits 70 when a machine check hit a job that was otherwise green (workspace
    rule 31). The summary line says `1 passed`, the exit code says the run is not trustworthy:
    the exit code wins, twice."""
    rc, calls, out = run_gate([(70, OK_LINE), (70, OK_LINE)])
    assert rc != 0, "a nonzero exit must fail the gate even when the output shows 1 passed"
    assert calls == 2
    assert "machine check" in out, "the refusal must name the machine check, not the emitter's cost"


# ── the refusal names the cause it saw, never one it did not measure ────────
# Cross-audit 04/10/2026, X6: the closing line said "o custo do emissor r3 passou do limite" for
# ANY nonzero exit. `S1A_CARGO="cargo --broken-flag"` failed twice without running the benchmark
# and the gate still blamed the emitter's cost.

COST_BLAME = "custo do emissor r3 passou do limite"


def test_a_run_that_never_ran_the_test_is_not_blamed_on_the_emitter():
    rc, calls, out = run_gate([(101, "error: could not compile `touring-hook-runtime`")] * 2)
    assert (rc, calls) == (1, 2), out
    assert COST_BLAME not in out, f"nothing was measured, so the cost was not exceeded: {out}"
    assert "não rodou" in out, f"the refusal must say the test did not run: {out}"


def test_a_real_regression_is_named_as_the_emitter_cost():
    rc, calls, out = run_gate([(101, FAIL_LINE), (101, FAIL_LINE)])
    assert (rc, calls) == (1, 2), out
    assert COST_BLAME in out, f"a benchmark that ran and failed IS the cost: {out}"


def test_no_hardware_slot_is_named_and_not_blamed_on_the_emitter():
    rc, calls, out = run_gate([(75, "hw-slot: sem slot"), (75, "hw-slot: sem slot")])
    assert (rc, calls) == (1, 2), out
    assert "sem slot" in out and COST_BLAME not in out, out


# ── content invariants: the wiring and the thing it points at ───────────────

def test_the_gate_script_is_executable():
    assert GATE.exists(), "scripts/r3-s1a-gate is missing"
    assert GATE.stat().st_mode & stat.S_IXUSR, "scripts/r3-s1a-gate must be executable"


def test_the_gate_runs_the_ignored_benchmark_by_name_under_hw_slot():
    # Comments mention these strings too; the contract is in the executable lines.
    body = "\n".join(
        l.strip() for l in GATE.read_text(encoding="utf-8").splitlines()
        if l.strip() and not l.strip().startswith("#")
    )
    assert re.search(rf"-- --ignored --nocapture {TEST_NAME}(?!\w)", body), \
        "the gate must run exactly this test, by its whole name, with --ignored"
    assert "-p touring-hook-runtime" in body
    assert "${S1A_RUNNER:-$HERE/hw-slot --class rust --}" in body, \
        "a heavy job passes through hw-slot by default (workspace rule 31)"


def test_the_benchmark_it_points_at_still_exists_and_is_still_ignored():
    """Rename the test and the gate goes blind: pin the target."""
    lines = SHADOW_RS.read_text(encoding="utf-8").splitlines()
    idx = [i for i, l in enumerate(lines) if f"fn {TEST_NAME}(" in l]
    assert len(idx) == 1, f"{TEST_NAME} must exist exactly once in r3_shadow.rs"
    window = "\n".join(lines[max(0, idx[0] - 4): idx[0]])
    assert "#[ignore" in window, f"{TEST_NAME} is no longer #[ignore]: the gate is redundant, revisit it"


def test_the_release_runs_the_gate_in_gate_1_and_announces_it_in_dry_run():
    body = "\n".join(
        l.strip() for l in RELEASE.read_text(encoding="utf-8").splitlines()
        if l.strip() and not l.strip().startswith("#")
    )
    assert body.count("scripts/r3-s1a-gate") >= 2, \
        "gate 1/6 must call scripts/r3-s1a-gate AND the dry-run branch must announce it"
    # The CALL and the ABORT as one pair. `"S1a" in body and "die" in body` held with the
    # `|| die "S1a …"` deleted (a mutant survived it, cross-audit 04/10/2026, X11): "die" is on every
    # other gate line and "S1a" in the dry-run announcement.
    raw = RELEASE.read_text(encoding="utf-8")
    assert re.search(r'"\$WORKSPACE/scripts/r3-s1a-gate" \) \\\n\s*\|\| die "S1a', raw), \
        "a failed S1a gate must abort the release with the S1a message"
