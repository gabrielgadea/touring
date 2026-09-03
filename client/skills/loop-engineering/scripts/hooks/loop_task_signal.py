#!/usr/bin/env python3
"""loop_task_signal.py — the STRUCTURAL signal: how big is the change about to happen?

Opção D (Gabriel, 03/09/2026): *"buscar solução definitiva que potencialize o projeto
é sempre uma das melhores opções"*. The OUTER used to be armed by a regex over the
prompt text, which is an oracle nobody can calibrate. Measured on this very session,
that regex got it exactly backwards:

    "estou achando que a estratégia de escrever um .md…"  (a QUESTION)  → armed
    "Execute D5 e D6 …"        (the turn that wrote ~10 files)          → did NOT arm

The fix is not a better regex. Touring already ships a deterministic task-level
classifier — `touring route`, which maps a scope vector (depth/files/symbols/cognitive/
coupling) to a CILA level L0..L5 with a routing mode and a composite. It discriminates
cleanly:

    depth 1  files 1   symbols 1   → L0 Solo          composite 0.085
    depth 3  files 12  symbols 40  → L3 Orchestrated  composite 0.505
    depth 5  files 60  symbols 300 → L5 FullTaco      composite 0.780

…and it had NO consumers: the only callers of `cli/route.rs` were its own unit tests.
A finished classifier wired to nothing (REGRA #0 — an orphan is a wiring opportunity,
never dead weight). This module is that consumer: it measures the REAL vector of the
file about to be edited, asks `route` for the level, and lets the gate arm in
proportion to actual impact instead of to the wording of a sentence.

WHY THIS IS THE DEFINITIVE FORM
    The prompt is a CLAIM about the work; the index KNOWS the work. Editing a scratch
    note and rewriting a module with 19 consumers are different acts, and only the
    second deserves an interruption. A text oracle cannot tell them apart; a blast
    radius can, and it is already computed.
"""
from __future__ import annotations

import json
import os
import subprocess

CLI_TIMEOUT = 8
# The floor was CALIBRATED, not guessed, and it moved once during calibration — the
# honest record of that matters more than the number.
#
# L2 was the first pick, on the reasoning that it is the first level `route` marks as
# more than Solo. But `route` is being fed a deliberately reduced vector (the two
# derived scores were dropped once measurement showed their direction was not what I
# assumed), and with counts alone L2 turned out to need ~100+ consumers:
#
#     cli/route.rs          blast   1  → L0
#     cli_suggester.rs      blast  19  → L1
#     verifications/mod.rs  blast 144  → L2
#
# A module with nineteen dependents is unambiguously work that a recall should precede,
# so a floor that lets it through is the wrong floor. L1 is where the measured
# separation actually falls: blast 0-1 stays silent, ~10+ consumers arms. The
# alternative — stretching the `depth` proxy until L2 lit up — would have been tuning
# the instrument to reach a predetermined answer, which is the failure this whole
# session has been about.
DEFAULT_MIN_LEVEL = 1
LEVEL_ENV = "TOURING_OUTER_MIN_LEVEL"


def _cli(args, cwd=None):
    """Best-effort JSON from a touring subcommand; None on any failure."""
    try:
        proc = subprocess.run(["touring", *args], capture_output=True, text=True,
                              timeout=CLI_TIMEOUT, cwd=cwd)
        if proc.returncode != 0 or not proc.stdout.strip():
            return None
        return json.loads(proc.stdout)
    except Exception:  # noqa: BLE001 — fail-open everywhere in this subsystem
        return None


def scope_vector(file_path: str, cwd=None):
    """The real scope of a change to `file_path`, taken from the index.

    Two TIER-1 calls (<10ms each per the CLI ranks): `ast blast` for the quantities
    `route` wants as COUNTS, `ast meta` for the ones it wants as RATIOS. Mixing the two
    sources is deliberate — `ast meta` reports fan-in/fan-out as SIGNALS in [0,1], not
    as counts, and feeding a ratio into `--files` would silently flatten every file to
    the same size.

    Returns None when the index cannot answer, which the caller must read as "unknown"
    and never as "small". On a cache miss `ast meta` falls back to `on_disk_fallback`,
    where `cognitive_score` comes back as 1.0 for a 13-line markdown file; trusting
    that number would arm the gate hardest on the most trivial edits.
    """
    if not file_path:
        return None
    blast = _cli(["ast", "blast", file_path, "-j"], cwd)
    if blast is None:
        return None
    consumers = blast.get("consumers") or []
    files = len(consumers) if isinstance(consumers, list) else int(blast.get("blast_radius") or 0)
    radius = int(blast.get("blast_radius") or files)

    meta = _cli(["ast", "meta", file_path, "--depth", "summary", "-j"], cwd) or {}
    indexed = bool(meta) and meta.get("enrichment_source") != "on_disk_fallback"

    # `ast blast` answers 0 for BOTH "nothing depends on this" and "this file is not in
    # the index at all" — absence with two causes again, this time in the instrument
    # itself. They must not collapse: the first is a real measurement (a leaf module),
    # the second is no measurement (a skill under ~/.claude, a doc, a file in another
    # project). `enrichment_source` is what separates them, so it decides here.
    if radius == 0:
        if not indexed:
            return None  # unknown — the caller reports `index-silent`, never "small"
        return {"depth": 1, "files": 0, "symbols": 0, "cognitive": 0.0, "coupling": 0.0,
                "blast_radius": 0, "indexed": True, "no_consumers": True}
    symbols = len(meta.get("pub_symbols") or []) if indexed else 0

    # `cognitive_score` and `integration_score` are DELIBERATELY not in the vector.
    #
    # The calibration run over 108 historical edit-turns (P3, 03/09/2026) caught this
    # and it is worth stating plainly: I assumed what `cognitive_score` means and used
    # it backwards. Measured, indexed, no fallback involved:
    #
    #     MEMORY.md          cognitive 1.000  coupling 1.000  blast 0   → came out L3
    #     cli_suggester.rs   cognitive 0.352  coupling 0.733  blast 19  → L3
    #
    # A plain list of notes scoring HIGHER than the busiest hook in the workspace means
    # the scale does not run the way I read it — plausibly "simplicity", plausibly a
    # default for unparsed languages; I did not verify, so it does not get a vote.
    # Feeding a classifier a number whose DIRECTION is unknown is worse than feeding it
    # nothing: it produced a confident L3 on a markdown file with zero consumers, which
    # would have made the gate fire hardest exactly where it is least useful.
    #
    # What survives is what was verified by execution: counts of real consumers and
    # real public symbols. When the semantics of these two scores are established, they
    # can come back — with a test that pins the direction.
    cognitive = 0.0
    coupling = 0.0

    # `depth` is the one field the index does not hand over directly (it would need a
    # per-symbol `wiring impact` walk, several calls deep). Declared proxy: one level
    # per decade of blast radius, capped at 5 — monotonic in the quantity that matters,
    # and the same shape `route`'s own documented examples use.
    depth = 1 + min(4, radius // 10)
    return {"depth": depth, "files": files, "symbols": symbols,
            "cognitive": round(cognitive, 3), "coupling": round(coupling, 3),
            "blast_radius": radius, "indexed": indexed}


def classify(vector, cwd=None):
    """Ask `touring route` for the CILA level of this vector."""
    if not vector:
        return None
    out = _cli(["route",
                "--depth", str(vector["depth"]),
                "--files", str(vector["files"]),
                "--symbols", str(vector["symbols"]),
                "--cognitive", str(vector["cognitive"]),
                "--coupling", str(vector["coupling"]),
                "-j"], cwd)
    if not out or not str(out.get("level", "")).startswith("L"):
        return None
    try:
        level_n = int(str(out["level"])[1:])
    except ValueError:
        return None
    return {"level": out["level"], "level_n": level_n,
            "mode": out.get("routing_mode"), "composite": out.get("composite"),
            "phases": out.get("phases") or []}


def min_level() -> int:
    try:
        return int(os.environ.get(LEVEL_ENV) or DEFAULT_MIN_LEVEL)
    except ValueError:
        return DEFAULT_MIN_LEVEL


def assess(file_path: str, cwd=None) -> dict:
    """Full verdict for one about-to-be-edited file.

    `arm` is True only when the index actually answered AND the level clears the floor.
    Unknown never means small: if the index cannot answer, the gate stays out of the way
    rather than guessing — the fail-open invariant this subsystem is held to, and the
    honest reading of a missing signal (lesson `ausencia-de-sinal-tem-duas-causas`:
    absence has two causes and only one of them is "nothing there").
    """
    vec = scope_vector(file_path, cwd)
    if vec is None:
        return {"arm": False, "reason": "index-silent", "vector": None, "route": None}
    route = classify(vec, cwd)
    if route is None:
        return {"arm": False, "reason": "route-silent", "vector": vec, "route": None}
    floor = min_level()
    clears = route["level_n"] >= floor
    return {"arm": clears,
            "reason": f"{route['level']} {'>=' if clears else '<'} L{floor}",
            "vector": vec, "route": route, "min_level": floor}


if __name__ == "__main__":
    import sys
    target = sys.argv[1] if len(sys.argv) > 1 else ""
    print(json.dumps(assess(target, os.getcwd()), indent=2, ensure_ascii=False))
