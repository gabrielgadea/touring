#!/usr/bin/env python3
"""The armed OUTER contract must NOT move under the agent's feet.

Origin (03/09/2026, Gabriel): the Stop hook blocked a turn as `[work-outer] 1/2`;
the agent ran the very command `flow_manifests.json` prescribes as the remedy
(`touring adw run strategy-loop`); the next block came back `[strategy-outer] 3/5`,
now demanding a `strategy-doc` that `work-outer` explicitly waives. Obeying the gate
raised the bar — which is how a gate becomes an unwinnable toll booth. The compliance
ledger shows the damage: runs of up to 63 consecutive blocks, and `max_continuations`
never bites because the counter is per-marker while the contract moves.

TWO independent routes produced the promotion, so both are asserted here:

  route A — `strategy-loop.toml` passed a hard `--flow strategy-outer`. The same ADW is
            both the BODY of `strategy-outer` and the REMEDY for `work-outer`, so it
            must never overwrite an armed flow (now `--flow-if-absent`).
  route B — `write_marker` computed `new_flow` only to decide `starts_new_cycle` and
            never wrote it; `data` got `flow` solely from `data.update(extra)`. A
            refresh without an explicit flow therefore ERASED it — and a flowless
            marker is not neutral, because `loop_outer_gate.py` reads
            `marker.get("flow") or "strategy-outer"`, i.e. erasure == promotion.

The assertions are POSITIVE (the armed flow SURVIVES), never "the wrong flow is
absent": route B was invisible to an absence check because the field was simply gone.
"""
from __future__ import annotations

import json
import pathlib
import subprocess
import sys

import pytest

HOOKS = pathlib.Path(__file__).resolve().parent
MARKER_CLI = HOOKS / "loop_marker.py"
MANIFESTS = HOOKS / "flow_manifests.json"
LIBRARY_TOML = pathlib.Path.home() / ".claude/skills/Touring/adw-library/strategy-loop.toml"


def _write(home, *args):
    subprocess.run(
        [sys.executable, str(MARKER_CLI), "write", "--task", "OUTER",
         "--status", "outer", "--scope", "/tmp/s", "--cwd", "/tmp/s",
         "--session-id", "sess", *args],
        check=True, capture_output=True,
        env={"PATH": "/usr/bin:/bin", "HOME": str(home), "LOOP_ENGINEERING_HOME": str(home)},
    )


def _flow(home):
    files = [f for f in home.iterdir() if f.suffix == ".json"]
    assert len(files) == 1, f"expected exactly one marker, got {files}"
    return json.loads(files[0].read_text()).get("flow")


@pytest.mark.parametrize("armed", ["work-outer", "cross-audit"])
def test_remedy_never_promotes_an_armed_contract(tmp_path, armed):
    """Route A: the prescribed ADW must leave the armed flow untouched."""
    _write(tmp_path, "--flow", armed)
    _write(tmp_path, "--flow-if-absent", "strategy-outer")
    assert _flow(tmp_path) == armed


def test_flow_if_absent_still_arms_when_there_is_no_contract(tmp_path):
    """The flag must remain useful when the ADW runs cold (no marker yet)."""
    _write(tmp_path, "--flow-if-absent", "strategy-outer")
    assert _flow(tmp_path) == "strategy-outer"


def test_explicit_flow_still_wins(tmp_path):
    """A human typing /loop-engineering legitimately promotes — `--flow` is unaffected."""
    _write(tmp_path, "--flow", "work-outer")
    _write(tmp_path, "--flow", "strategy-outer")
    assert _flow(tmp_path) == "strategy-outer"


def test_refresh_without_any_flag_preserves_the_flow(tmp_path):
    """Route B: the erasure bug. A flowless marker silently means `strategy-outer`."""
    _write(tmp_path, "--flow", "work-outer")
    _write(tmp_path)
    assert _flow(tmp_path) == "work-outer"


def test_library_template_uses_flow_if_absent():
    """Route A at the source: the deployed library, not just the project instance.

    `touring adw from-template` copies from the LIBRARY, so a fix applied only to
    `<proj>/.touring/adw/` is re-broken by the next scaffold (the 25/07/2026 lesson:
    a fix reached the mirror and the instances but never the deployed library).
    """
    if not LIBRARY_TOML.exists():
        pytest.skip("adw-library not installed in this environment")
    body = LIBRARY_TOML.read_text()
    assert "--flow-if-absent strategy-outer" in body
    assert "--flow strategy-outer" not in body


def test_every_manifest_next_action_is_reachable():
    """D8 guard: a prescribed `next_action` must be able to satisfy its own artifact.

    `explore-ledger` requires `verdict.converged: true`, but the ledger's `external`
    lens is MANUAL — no number of `--until-dry` rounds ever converges (measured: 40
    rounds, 32 of them dry, still `converged:false`). The prescription must therefore
    carry `--mark-lens`. This artifact accounted for 395 of 518 recorded incompletions.
    """
    manifests = json.loads(MANIFESTS.read_text())
    for flow, spec in manifests.items():
        if flow.startswith("_"):
            continue
        for art in spec["artifacts"]:
            if art.get("require_json", {}).get("verdict.converged") is True:
                assert "--mark-lens" in art["next_action"], (
                    f"{flow}/{art['id']}: next_action demands converged:true but never "
                    f"marks the manual lens — unsatisfiable by its own prescription"
                )
