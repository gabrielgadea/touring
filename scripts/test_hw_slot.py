"""Contract tests for scripts/hw-slot — the machine-wide heavy-job semaphore.

Every guard is proven against the REAL script with real flock + real child
processes; only the three hardware readings (temperature, MemAvailable, the
kernel journal) are overridden through env so the tests are deterministic.
"""
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "hw-slot"


@pytest.fixture
def env(tmp_path):
    e = dict(os.environ)
    for k in list(e):
        if k.startswith("HW_SLOT_") or k in ("CARGO_BUILD_JOBS", "RUST_TEST_THREADS"):
            del e[k]
    e.update(
        HW_SLOT_DIR=str(tmp_path),
        HW_SLOT_LOG=str(tmp_path / "hw-slot.jsonl"),
        HW_SLOT_TEMP_C="60",
        HW_SLOT_MEM_AVAIL_GB="30",
        HW_SLOT_JOURNAL_CMD="true",
        HW_SLOT_POLL_SECS="0.1",
    )
    return e


def run(env, *args, timeout=20):
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args],
        env=env, capture_output=True, text=True, timeout=timeout,
    )


def hold(env, secs, tmp_path, *opts):
    """Start a holder that keeps its slot(s) for `secs`; return once it owns them."""
    p = subprocess.Popen(
        [sys.executable, str(SCRIPT), *opts, "--", "sleep", str(secs)],
        env=env, start_new_session=True,
    )
    deadline = time.time() + 5
    while time.time() < deadline:
        if list(Path(env["HW_SLOT_DIR"]).glob("hw-slot-*.info")):
            return p
        time.sleep(0.05)
    p.kill()
    pytest.fail("holder never acquired a slot")


def test_exit_code_is_transparent(env):
    assert run(env, "--", "bash", "-c", "exit 7").returncode == 7
    assert run(env, "--", "true").returncode == 0


def test_cargo_defaults_exported_but_caller_value_wins(env):
    r = run(env, "--", "bash", "-c", "echo $CARGO_BUILD_JOBS $RUST_TEST_THREADS")
    assert r.stdout.split() == ["4", "4"]
    env["CARGO_BUILD_JOBS"] = "2"
    r = run(env, "--", "bash", "-c", "echo $CARGO_BUILD_JOBS $RUST_TEST_THREADS")
    assert r.stdout.split() == ["2", "4"]


def test_single_slot_is_exclusive_and_names_the_holder(env, tmp_path):
    env["HW_SLOT_COUNT"] = "1"
    p = hold(env, 4, tmp_path)
    try:
        r = run(env, "--wait", "1", "--", "true")
        assert r.returncode == 75
        assert "sleep" in r.stderr and "slot" in r.stderr
    finally:
        os.killpg(p.pid, signal.SIGKILL)


def test_two_slots_run_concurrently(env, tmp_path):
    env["HW_SLOT_COUNT"] = "2"
    p = hold(env, 4, tmp_path)
    try:
        assert run(env, "--wait", "1", "--", "true").returncode == 0
    finally:
        os.killpg(p.pid, signal.SIGKILL)


def test_slot_is_released_by_the_kernel_when_the_holder_dies(env, tmp_path):
    env["HW_SLOT_COUNT"] = "1"
    p = hold(env, 30, tmp_path)
    os.killpg(p.pid, signal.SIGKILL)  # wrapper AND child gone — no cleanup code runs
    p.wait()
    assert run(env, "--wait", "3", "--", "true").returncode == 0


def test_a_long_lived_process_started_inside_the_job_never_keeps_the_slot(env):
    """Incident 02/10/2026: update-touring restarts the GLOBAL daemon inside a job; if the
    lock fd is inherited, the daemon holds the slot for as long as it lives (days) and
    the next `hw-slot` waits forever on a job that already finished."""
    env["HW_SLOT_COUNT"] = "1"
    job = run(env, "--", "bash", "-c", "setsid sleep 25 >/dev/null 2>&1 & echo $!")
    stray = int(job.stdout.strip())
    try:
        assert job.returncode == 0
        # the job is over; the daemon-like stray is still alive — the slot must be free
        os.kill(stray, 0)
        assert run(env, "--wait", "2", "--", "true").returncode == 0
    finally:
        try:
            os.kill(stray, signal.SIGKILL)
        except ProcessLookupError:
            pass


def test_temperature_gate_blocks_and_names_the_reason(env):
    env["HW_SLOT_TEMP_C"] = "95"
    r = run(env, "--wait", "1", "--max-temp", "88", "--", "true")
    assert r.returncode == 75
    assert "temp" in r.stderr and "95" in r.stderr
    env["HW_SLOT_TEMP_C"] = "80"
    assert run(env, "--wait", "1", "--max-temp", "88", "--", "true").returncode == 0


def test_memory_gate_blocks_and_names_the_reason(env):
    env["HW_SLOT_MEM_AVAIL_GB"] = "3"
    r = run(env, "--wait", "1", "--min-mem-gb", "12", "--", "true")
    assert r.returncode == 75
    assert "mem" in r.stderr and "3" in r.stderr


def test_unmeasured_temperature_proceeds_with_a_warning(env):
    del env["HW_SLOT_TEMP_C"]
    env["HW_SLOT_TEMP_FILE"] = "/nonexistent"
    r = run(env, "--wait", "1", "--", "true")
    assert r.returncode == 0
    assert "unmeasured" in r.stderr


def test_machine_check_during_a_green_job_invalidates_it(env):
    env["HW_SLOT_JOURNAL_CMD"] = "printf 'mce: [Hardware Error]: Machine check events logged\\n'"
    r = run(env, "--", "true")
    assert r.returncode == 70
    assert "MCE" in r.stderr
    # a job that already failed keeps its own exit code — MCE is reported, not masked
    r = run(env, "--", "bash", "-c", "exit 3")
    assert r.returncode == 3 and "MCE" in r.stderr


def test_every_run_appends_one_json_line(env):
    run(env, "--label", "unit", "--", "true")
    lines = Path(env["HW_SLOT_LOG"]).read_text().splitlines()
    rec = json.loads(lines[-1])
    assert rec["rc"] == 0 and rec["label"] == "unit" and rec["slots"] == ["heavy-1"]
    assert rec["class"] == "heavy" and rec["passthrough"] == []
    assert {"waited_s", "secs", "temp_c", "mem_gb", "mce"} <= rec.keys()


def test_status_reports_hardware_and_every_slot(env, tmp_path):
    env["HW_SLOT_COUNT"] = "2"
    p = hold(env, 4, tmp_path)
    try:
        s = json.loads(run(env, "--status").stdout)
        assert s["temp_c"] == 60 and s["mem_avail_gb"] == 30
        assert [x["held"] for x in s["slots"]].count(True) == 1
        held = next(x for x in s["slots"] if x["held"])
        assert "sleep" in held["holder"]["cmd"]
    finally:
        os.killpg(p.pid, signal.SIGKILL)


def test_usage_errors_exit_64(env):
    assert run(env).returncode == 64
    assert run(env, "--wait", "1").returncode == 64
    assert run(env, "--class", "gpu", "--", "true").returncode == 64


def kill_group(p):
    os.killpg(p.pid, signal.SIGKILL)
    p.wait()


def test_rust_class_is_exclusive_and_still_leaves_a_heavy_slot(env, tmp_path):
    p = hold(env, 4, tmp_path, "--class", "rust")
    try:
        r = run(env, "--class", "rust", "--wait", "1", "--", "true")
        assert r.returncode == 75 and "rust" in r.stderr
        # a rust job also takes one heavy slot, so a plain heavy job still fits...
        assert run(env, "--wait", "1", "--", "true").returncode == 0
    finally:
        kill_group(p)


def test_rust_job_counts_against_the_heavy_budget(env, tmp_path):
    env["HW_SLOT_COUNT"] = "1"
    p = hold(env, 4, tmp_path, "--class", "rust")
    try:
        assert run(env, "--wait", "1", "--", "true").returncode == 75
    finally:
        kill_group(p)


def test_nested_call_passes_through_the_holders_slot(env):
    """The judge chains scorer + pytest: with ONE slot a nested hw-slot must not deadlock."""
    env["HW_SLOT_COUNT"] = "1"
    r = run(env, "--", sys.executable, str(SCRIPT), "--wait", "1", "--", "echo", "inner-ran")
    assert r.returncode == 0 and "inner-ran" in r.stdout


def test_rust_inside_heavy_takes_only_the_missing_rust_slot(env):
    env["HW_SLOT_COUNT"] = "1"
    r = run(env, "--", sys.executable, str(SCRIPT), "--class", "rust", "--wait", "1", "--", "true")
    assert r.returncode == 0
    recs = [json.loads(line) for line in Path(env["HW_SLOT_LOG"]).read_text().splitlines()]
    inner = next(x for x in recs if x["class"] == "rust")
    assert inner["slots"] == ["rust-1"] and inner["passthrough"] == ["heavy-1"]


def test_stale_held_env_never_bypasses_a_busy_slot(env, tmp_path):
    env["HW_SLOT_COUNT"] = "1"
    p = hold(env, 4, tmp_path)
    try:
        env["HW_SLOT_HELD"] = "heavy-1@424242"  # not the real holder's pid
        assert run(env, "--wait", "1", "--", "true").returncode == 75
    finally:
        kill_group(p)


def test_stale_held_env_with_a_free_slot_just_acquires_normally(env):
    env["HW_SLOT_HELD"] = "heavy-1@424242"
    assert run(env, "--wait", "1", "--", "true").returncode == 0
    rec = json.loads(Path(env["HW_SLOT_LOG"]).read_text().splitlines()[-1])
    assert rec["slots"] == ["heavy-1"] and rec["passthrough"] == []


def test_no_hold_and_wait_between_classes(env, tmp_path):
    """heavy holder wanting rust vs rust holder wanting heavy must not wedge each other.

    Scenario that makes hold-and-wait observable: rust is FREE, heavy is FULL. A waiting
    rust job gets rust-1, fails on heavy, and must hand rust-1 back instead of squatting
    on it while it polls — otherwise a heavy holder that needs rust can never proceed.
    """
    env["HW_SLOT_COUNT"] = "1"
    p = hold(env, 6, tmp_path)  # heavy-1 busy, rust-1 free
    waiter = subprocess.Popen(
        [sys.executable, str(SCRIPT), "--class", "rust", "--wait", "3", "--", "true"],
        env=env, start_new_session=True, stderr=subprocess.DEVNULL,
    )
    try:
        time.sleep(0.8)  # the waiter has polled several times (poll = 0.1s)
        held = {s["slot"]: s["held"] for s in json.loads(run(env, "--status").stdout)["slots"]}
        assert held == {"rust-1": False, "heavy-1": True}
    finally:
        kill_group(waiter)
        kill_group(p)


def test_status_lists_both_classes(env):
    names = [s["slot"] for s in json.loads(run(env, "--status").stdout)["slots"]]
    assert names == ["rust-1", "heavy-1", "heavy-2"]
