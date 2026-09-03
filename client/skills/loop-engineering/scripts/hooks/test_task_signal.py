#!/usr/bin/env python3
"""The structural signal: the gate arms on MEASURED impact, not on prompt wording.

Opção D (Gabriel, 03/09/2026). This file carries the coverage that used to live in
`test_flow_guard.py::test_substantive_work_arms_default_flow` — "substantive work gets
gated" — moved to the mechanism that can actually decide it. The heuristic it replaced
scored 0/2 on the turns that mattered: it armed on a QUESTION about the gate and stayed
silent through the turn that wrote ten files.

The verdict now comes from `touring route`, Touring's own CILA classifier, fed with the
real blast radius of the file about to change. `route` shipped with NO consumers — the
only callers of `cli/route.rs` were its own unit tests — so this is also the wiring of
a finished orphan (REGRA #0).
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

HOOKS = Path(__file__).resolve().parent
sys.path.insert(0, str(HOOKS))
import loop_task_signal as ts  # noqa: E402

REPO = Path("/home/gabrielgadea/projects/touring")


def _fake_cli(blast=None, meta=None, route=None):
    """Stub of the three CLI calls, so the unit tests never need a live daemon."""
    def _cli(args, cwd=None):
        if args[:2] == ["ast", "blast"]:
            return blast
        if args[:2] == ["ast", "meta"]:
            return meta
        if args[0] == "route":
            return route
        return None
    return _cli


def test_a_trivial_file_never_arms(monkeypatch):
    monkeypatch.setattr(ts, "_cli", _fake_cli(
        blast={"blast_radius": 0, "consumers": []},
        meta={"enrichment_source": "index", "cognitive_score": 0.1,
              "integration_score": 0.0, "pub_symbols": []},
        route={"level": "L0", "routing_mode": "Solo", "composite": 0.08}))
    v = ts.assess("scratch.md", "/tmp")
    assert v["arm"] is False and v["route"]["level"] == "L0"


def test_a_module_with_real_consumers_arms(monkeypatch):
    monkeypatch.setattr(ts, "_cli", _fake_cli(
        blast={"blast_radius": 19, "consumers": [f"c{i}.rs" for i in range(19)]},
        meta={"enrichment_source": "index", "cognitive_score": 0.35,
              "integration_score": 0.4, "pub_symbols": ["a"] * 10},
        route={"level": "L3", "routing_mode": "Orchestrated", "composite": 0.5}))
    v = ts.assess("core.rs", "/tmp")
    assert v["arm"] is True and v["route"]["level_n"] == 3


@pytest.mark.parametrize("blast,meta,label", [
    (None, {}, "index silent"),
    ({"blast_radius": 5, "consumers": ["a"] * 5}, {}, "route silent"),
])
def test_unknown_never_means_small(monkeypatch, blast, meta, label):
    """A silent instrument must not be read as "nothing there".

    Absence has two causes (`ausencia-de-sinal-tem-duas-causas`) and only one of them
    is "the change is trivial". When the index or `route` cannot answer, the gate steps
    aside rather than guessing — fail-open, and honest about WHY.
    """
    monkeypatch.setattr(ts, "_cli", _fake_cli(blast=blast, meta=meta, route=None))
    v = ts.assess("x.rs", "/tmp")
    assert v["arm"] is False
    assert v["reason"] in ("index-silent", "route-silent"), label


def test_derived_scores_never_reach_the_vector(monkeypatch):
    """`cognitive_score` / `integration_score` are excluded — direction unverified.

    Measured 03/09/2026, both files INDEXED (no fallback involved):
        MEMORY.md        cognitive 1.000  coupling 1.000  blast 0
        cli_suggester.rs cognitive 0.352  coupling 0.733  blast 19
    A list of notes scoring higher than the busiest hook in the workspace means the
    scale does not run the way I read it. Feeding a classifier a number whose DIRECTION
    is unknown is worse than feeding it nothing — it produced a confident L3 on a
    markdown file with zero consumers, arming the gate hardest where it is least useful.
    """
    captured = {}

    def _cli(args, cwd=None):
        if args[:2] == ["ast", "blast"]:
            return {"blast_radius": 12, "consumers": [f"c{i}" for i in range(12)]}
        if args[:2] == ["ast", "meta"]:
            return {"enrichment_source": "index", "cognitive_score": 1.0,
                    "integration_score": 1.0, "pub_symbols": ["a", "b"]}
        captured["route_args"] = args
        return {"level": "L1", "routing_mode": "Solo", "composite": 0.2}

    monkeypatch.setattr(ts, "_cli", _cli)
    v = ts.assess("m.rs", "/tmp")
    assert v["vector"]["cognitive"] == 0.0, "the unverified score must not vote"
    assert v["vector"]["coupling"] == 0.0
    a = captured["route_args"]
    assert a[a.index("--cognitive") + 1] == "0.0"
    assert a[a.index("--coupling") + 1] == "0.0"


def test_out_of_index_is_unknown_not_small(monkeypatch):
    """`ast blast` answers 0 for BOTH "leaf module" and "not indexed at all".

    They must not collapse: the first is a measurement, the second is its absence. A
    skill under ~/.claude or a doc in another project is unmeasurable here, and calling
    it "small" would be inventing a verdict. `enrichment_source` separates them.
    """
    monkeypatch.setattr(ts, "_cli", _fake_cli(
        blast={"blast_radius": 0, "consumers": []},
        meta={"enrichment_source": "on_disk_fallback", "cognitive_score": 1.0,
              "pub_symbols": ["Log"]},
        route={"level": "L0", "routing_mode": "Solo", "composite": 0.0}))
    v = ts.assess("SKILL.md", "/tmp")
    assert v["arm"] is False and v["reason"] == "index-silent"
    assert v["vector"] is None, "no vector may be fabricated for an unmeasured file"

    # An INDEXED leaf, by contrast, is a real measurement of "nothing depends on it".
    monkeypatch.setattr(ts, "_cli", _fake_cli(
        blast={"blast_radius": 0, "consumers": []},
        meta={"enrichment_source": "index", "pub_symbols": ["a"]},
        route={"level": "L0", "routing_mode": "Solo", "composite": 0.0}))
    leaf = ts.assess("leaf.rs", "/tmp")
    assert leaf["arm"] is False and leaf["vector"]["no_consumers"] is True


def test_counts_come_from_blast_not_from_the_normalised_signals(monkeypatch):
    """`ast meta` reports fan-in/fan-out as SIGNALS in [0,1], not as counts.

    Feeding a ratio into `--files` would flatten every file in the repo to the same
    size — a plausible number that survives observation. The counts must come from
    `ast blast`, which returns the consumer list itself.
    """
    captured = {}

    def _cli(args, cwd=None):
        if args[:2] == ["ast", "blast"]:
            return {"blast_radius": 42, "consumers": [f"c{i}" for i in range(42)]}
        if args[:2] == ["ast", "meta"]:
            return {"enrichment_source": "index", "fan_in_signal": 0.7,
                    "fan_out_signal": 0.9, "cognitive_score": 0.5,
                    "integration_score": 0.6, "pub_symbols": ["s"] * 7}
        captured["args"] = args
        return {"level": "L3", "routing_mode": "Orchestrated", "composite": 0.5}

    monkeypatch.setattr(ts, "_cli", _cli)
    ts.assess("m.rs", "/tmp")
    a = captured["args"]
    assert a[a.index("--files") + 1] == "42", "files must be the consumer COUNT"
    assert a[a.index("--symbols") + 1] == "7"
    # 0.7 / 0.9 are fan-in/fan-out SIGNALS. They must reach neither --files (a count)
    # nor --cognitive (excluded entirely, see test_derived_scores_never_reach_the_vector).
    assert "0.7" not in a and "0.9" not in a


def test_floor_is_tunable(monkeypatch):
    monkeypatch.setattr(ts, "_cli", _fake_cli(
        blast={"blast_radius": 3, "consumers": ["a", "b", "c"]},
        meta={"enrichment_source": "index", "pub_symbols": ["x"]},
        route={"level": "L2", "routing_mode": "Solo", "composite": 0.3}))
    monkeypatch.delenv(ts.LEVEL_ENV, raising=False)
    assert ts.assess("m.rs", "/tmp")["arm"] is True      # default floor L2
    monkeypatch.setenv(ts.LEVEL_ENV, "4")
    assert ts.assess("m.rs", "/tmp")["arm"] is False     # raised above the level
    monkeypatch.setenv(ts.LEVEL_ENV, "não-é-número")
    assert ts.assess("m.rs", "/tmp")["arm"] is True      # garbage → documented default


@pytest.mark.skipif(not REPO.exists(), reason="touring workspace not present")
def test_live_calibration_against_real_files():
    """End-to-end against the real index — the calibration that chose the L2 floor.

    Measured 03/09/2026, 6/6 correct with no false positive in either direction:
        log.md / scratch (blast 0) → L0   not armed
        cli/route.rs     (blast 1) → L1   not armed
        cli_suggester.rs (blast 19)→ L3   ARMED
        verifications/mod.rs (144) → L3   ARMED
    Asserted as a RELATION, not as fixed levels: blast radius grows, so must the
    verdict. Pinning exact levels would make this test a hostage of index churn.
    """
    trivial = ts.assess(str(REPO / "docs/plans/2026-09-03-work-outer/log.md"), str(REPO))
    heavy = ts.assess("crates/touring-cli/src/cli_suggester.rs", str(REPO))
    if not (trivial.get("route") and heavy.get("route")):
        pytest.skip("daemon/index unavailable — the signal is fail-open by design")
    assert trivial["arm"] is False, "a 13-line markdown must never arm the gate"
    assert heavy["arm"] is True, "a module with real consumers must arm it"
    assert heavy["route"]["level_n"] > trivial["route"]["level_n"]
    assert heavy["vector"]["blast_radius"] > trivial["vector"]["blast_radius"]


def test_prompt_wording_is_no_longer_consulted_anywhere():
    """Structural guard: the retired heuristic must not creep back into the arm path.

    `loop_outer_effect` is the only thing that arms `work-outer` now, and it must reach
    that decision through the measured signal. A future edit that reintroduces
    `is_default_work` there would silently restore the 0/2 oracle.
    """
    effect = (HOOKS / "loop_outer_effect.py").read_text()
    assert "assess(" in effect, "the effect hook must consult the structural signal"
    assert "is_default_work" not in effect, "the retired text heuristic must stay out"
    arm_src = (HOOKS / "loop_outer_arm.py").read_text()
    detect = arm_src.split("def detect_flow")[1].split("\ndef ")[0]
    assert "DEFAULT_FLOW" not in detect.split('"""')[-1], (
        "detect_flow must no longer return the default flow from prompt text")
