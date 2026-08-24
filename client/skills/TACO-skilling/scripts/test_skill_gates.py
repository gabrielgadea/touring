#!/usr/bin/env python3
"""test_skill_gates.py — regression suite for the two skill gates.

Both gates shipped without one, and both had already regressed silently:

  * `skill_pin_gate.ghost_commands` compared only the FIRST level, so
    `touring search symbols` — which does not exist — passed for as long as
    `touring search` did. The extracting regex had captured the second level all
    along; only the verification ignored it (20/08/2026, found by VGP while
    complementing `touring-search`).
  * `skill_principle_gate` first keyed "this skill audits" off the WORD
    `cross-audit` anywhere in the body, which accused a document-drafting skill
    and a process-management skill: 10 findings, 7 false.

Run: python3 -m pytest test_skill_gates.py -q   (from this directory)
"""
from __future__ import annotations

import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import skill_pin_gate as pin  # noqa: E402
import skill_principle_gate as principle  # noqa: E402

CLAP_HELP = """Unified search: unified (default), exact, fuzzy, bm25

Usage: touring search [COMMAND]

Commands:
  unified  Multi-backend RRF-fused search (default)
  exact    Exact symbol search
  bm25     BM25 document search

Options:
  -h, --help  Print help
"""

ARGPARSE_HELP = "usage: touring adw [-h] {run,lint,test,explain,new} ...\n"

NO_SUBCOMMANDS_HELP = """Prior-art by purpose from the capability portfolio

Usage: touring portfolio <INTENT>

Arguments:
  <INTENT>  what you want to build
"""


class _P:
    def __init__(self, stdout: str):
        self.stdout, self.stderr, self.returncode = stdout, "", 0


@pytest.fixture(autouse=True)
def _clear_cache():
    pin._SUBCMD_CACHE.clear()
    yield
    pin._SUBCMD_CACHE.clear()


def _stub_help(monkeypatch, text: str):
    monkeypatch.setattr(pin.subprocess, "run", lambda *a, **k: _P(text))


# ── live_subcommands ─────────────────────────────────────────────────────────

def test_clap_commands_block_is_parsed(monkeypatch):
    _stub_help(monkeypatch, CLAP_HELP)
    assert pin.live_subcommands("search") == frozenset({"unified", "exact", "bm25"})


def test_argparse_choices_are_parsed(monkeypatch):
    _stub_help(monkeypatch, ARGPARSE_HELP)
    assert pin.live_subcommands("adw") == frozenset({"run", "lint", "test", "explain", "new"})


def test_a_command_family_without_subcommands_is_unknowable(monkeypatch):
    """It takes ARGUMENTS. Accusing its arguments is the false accusation this
    gate must never make — so the answer is None, not an empty set."""
    _stub_help(monkeypatch, NO_SUBCOMMANDS_HELP)
    assert pin.live_subcommands("portfolio") is None


def test_unreachable_binary_is_unknowable(monkeypatch):
    monkeypatch.setattr(pin.subprocess, "run",
                        lambda *a, **k: (_ for _ in ()).throw(OSError("no binary")))
    assert pin.live_subcommands("search") is None


def test_result_is_memoised(monkeypatch):
    calls = []
    monkeypatch.setattr(pin.subprocess, "run",
                        lambda *a, **k: (calls.append(1), _P(CLAP_HELP))[1])
    pin.live_subcommands("search")
    pin.live_subcommands("search")
    assert len(calls) == 1


# ── ghost_commands ───────────────────────────────────────────────────────────

def _skill(tmp_path: Path, body: str) -> Path:
    p = tmp_path / "SKILL.md"
    p.write_text(body, encoding="utf-8")
    return p


def test_second_level_ghost_is_caught(tmp_path, monkeypatch):
    """THE regression: `touring search` exists, `touring search symbols` does not."""
    _stub_help(monkeypatch, CLAP_HELP)
    md = _skill(tmp_path, 'Run `touring search symbols "<query>"` to find things.\n')
    ghosts = pin.ghost_commands(md, {"search"})
    assert [g["command"] for g in ghosts] == ["search symbols"]


def test_a_real_second_level_command_is_not_accused(tmp_path, monkeypatch):
    _stub_help(monkeypatch, CLAP_HELP)
    md = _skill(tmp_path, 'Run `touring search exact "Symbol"`.\n')
    assert pin.ghost_commands(md, {"search"}) == []


def test_unknowable_family_never_accuses_its_arguments(tmp_path, monkeypatch):
    _stub_help(monkeypatch, NO_SUBCOMMANDS_HELP)
    md = _skill(tmp_path, "Run `touring portfolio intent` before creating.\n")
    assert pin.ghost_commands(md, {"portfolio"}) == []

def test_first_level_ghost_still_caught(tmp_path, monkeypatch):
    _stub_help(monkeypatch, CLAP_HELP)
    md = _skill(tmp_path, "Run `touring evolvefoo bar`.\n")
    assert [g["command"] for g in pin.ghost_commands(md, {"search"})] == ["evolvefoo"]


def test_a_warning_about_a_ghost_is_not_usage(tmp_path, monkeypatch):
    """Five skills WARN that `touring quality` does not exist; counting the
    warning as usage inverted the finding completely."""
    _stub_help(monkeypatch, CLAP_HELP)
    md = _skill(tmp_path, "⚠ `touring quality score` NÃO existe — use touring-quality.\n")
    assert pin.ghost_commands(md, {"search"}) == []


def test_no_live_command_set_means_no_accusation(tmp_path):
    md = _skill(tmp_path, "Run `touring anything at all`.\n")
    assert pin.ghost_commands(md, set()) == []


# ── skill_principle_gate ─────────────────────────────────────────────────────

DECOMPOSER = """---
name: fake-dispatcher
description: Runs plans by dispatching a fresh subagent per task.
---
Use `touring decompose ready <task>` then dispatch a fresh subagent.
"""

AUDITOR_DELEGATING = """---
name: fake-auditor
description: Cross-audit a code directory for purpose fidelity.
---
Delegate the deep per-symbol audit to the `touring-auditor` subagent.
"""

AUDITOR_DETERMINISTIC = """---
name: fake-scorer
description: Cross-audit scores via touring-quality with CLI evidence.
---
Run `touring-quality score <target> --fail-below 0.80` and report the tier.
"""

VOCABULARY_ONLY = """---
name: fake-documents
description: Elaboração de documentos licitatórios para o DETRAN-DF.
---
Depois do parecer, faça um cross-audit das minutas com `touring memory recall`.
"""


def _root(tmp_path: Path, **skills) -> Path:
    for name, body in skills.items():
        d = tmp_path / name
        d.mkdir()
        (d / "SKILL.md").write_text(body, encoding="utf-8")
    return tmp_path


def test_decomposer_without_claim_is_flagged(tmp_path):
    _, findings = principle.audit(_root(tmp_path, dispatcher=DECOMPOSER))
    assert [f["principle"] for f in findings] == ["P5 (Wayfinder)"]


def test_decomposer_with_claim_passes(tmp_path):
    body = DECOMPOSER + "\nTake it with `touring decompose claim <task> --owner x`.\n"
    _, findings = principle.audit(_root(tmp_path, dispatcher=body))
    assert findings == []


def test_delegating_auditor_without_a_panel_is_flagged(tmp_path):
    _, findings = principle.audit(_root(tmp_path, auditor=AUDITOR_DELEGATING))
    assert [f["principle"] for f in findings] == ["P3 (the gauntlet)"]


def test_deterministic_scorer_has_nothing_to_blind(tmp_path):
    """Its verdict is already counted by code — that IS P3, satisfied."""
    _, findings = principle.audit(_root(tmp_path, scorer=AUDITOR_DETERMINISTIC))
    assert findings == []


def test_the_word_in_the_body_is_not_a_role(tmp_path):
    """The false accusation that made 7 of 10 findings wrong."""
    _, findings = principle.audit(_root(tmp_path, docs=VOCABULARY_ONLY))
    assert findings == []


def test_population_excludes_skills_that_never_touch_touring(tmp_path):
    body = "---\nname: x\ndescription: Cross-audit a code directory.\n---\nNo CLI here.\n"
    scoped, findings = principle.audit(_root(tmp_path, x=body))
    assert (scoped, findings) == (0, [])


def test_missing_root_is_a_usage_error(tmp_path):
    assert principle.main(["--root", str(tmp_path / "nope")]) == 2
