"""C1 of the compaction judge: the baseline is measured by the judge's OWN counter, never a constant.

Gabriel, 09/10/2026: "troque a base do C1". The 898 it replaces came from medir_conselho.py (three attachment
types, raw len) while the judge counts hook_additional_context with len(json.dumps) — a ratio with no common
denominator. Measured by the same counter: 1.034,1 before, 536,8 after (−48,1%).
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import verificar_compactacao as V  # noqa: E402

DEPLOY = "2026-10-09T01:11:50Z"


def _call(ts):
    return {
        "type": "assistant",
        "timestamp": ts,
        "message": {"content": [{"type": "tool_use", "name": "Bash", "input": {}}]},
    }


def _advice(ts, text):
    return {
        "type": "attachment",
        "timestamp": ts,
        "attachment": {"type": "hook_additional_context", "content": text},
    }


def _transcripts(tmp_path, records):
    root = tmp_path / "transcripts"
    root.mkdir()
    (root / "s.jsonl").write_text("".join(json.dumps(r) + "\n" for r in records))
    return root


def test_the_window_is_half_open(tmp_path):
    """[ini, fim): the record at ini counts, the one at fim belongs to the next window."""
    ini, fim = "2026-10-08T12:00:00Z", "2026-10-08T13:00:00Z"
    root = _transcripts(
        tmp_path,
        [
            _call("2026-10-08T11:59:59Z"),
            _call(ini),
            _call("2026-10-08T12:30:00.500Z"),
            _call(fim),
            _call("2026-10-08T13:00:00.250Z"),
            _advice("2026-10-08T12:10:00Z", "abc"),
        ],
    )
    assert V._advice_between(ini, fim, root=root) == (2, len(json.dumps("abc")))


def test_advice_since_is_the_open_window(tmp_path, monkeypatch):
    """The public reader other scripts import keeps its exact meaning."""
    root = _transcripts(
        tmp_path, [_call("2026-10-08T12:00:00Z"), _call("2026-10-09T09:00:00Z")]
    )
    monkeypatch.setattr(V, "PROJECT_TRANSCRIPTS", root)
    assert (
        V._advice_since("2026-10-08T12:00:00Z")
        == V._advice_between("2026-10-08T12:00:00Z", root=root)
        == (2, 0)
    )


def test_an_irreproducible_baseline_fails_the_clause_loudly(tmp_path):
    """Pruned transcripts must never move the bar: the recount is compared with the frozen one."""
    root = _transcripts(tmp_path, [_call("2026-10-09T02:00:00Z")] * 150)
    amostras = V._c1_amostras({"deploy_utc": DEPLOY}, root=root)
    ok, ev = V._c1_checks(amostras, cargo_rc=0)
    assert ev["linha de base reproduzível"] is False and ok is False


def test_the_drop_is_measured_against_the_same_counter(tmp_path, monkeypatch):
    """1000 chars/call before, 500 after → −50% passes; 700 after → −30% fails."""
    pre = [_call("2026-10-08T20:00:00Z")] * 10 + [
        _advice("2026-10-08T20:00:01Z", "x" * 9998)
    ]
    for depois, passa in ((498, True), (698, False)):
        pos = [_call("2026-10-09T05:00:00Z")] * 100 + [
            _advice("2026-10-09T05:00:01Z", "y" * (depois * 100 - 2))
        ]
        caso = tmp_path / str(depois)
        caso.mkdir()
        root = _transcripts(caso, pre + pos)
        amostras = V._c1_amostras({"deploy_utc": DEPLOY}, root=root)
        monkeypatch.setattr(V, "BASELINE_FROZEN", {"calls": 10, "chars": 10000})
        ok, ev = V._c1_checks(amostras, cargo_rc=0)
        assert ev["antes"] == 1000.0
        assert ev["queda ≥ 40%"] is passa and ok is passa, ev


def test_the_post_window_closes_at_the_next_release(tmp_path):
    """pos_fim_utc keeps the next release's advice out of this release's verdict."""
    root = _transcripts(
        tmp_path, [_call("2026-10-09T05:00:00Z"), _call("2026-10-09T15:00:00Z")]
    )
    aberta = V._c1_amostras({"deploy_utc": DEPLOY}, root=root)["pos"]
    fechada = V._c1_amostras(
        {"deploy_utc": DEPLOY, "pos_fim_utc": "2026-10-09T14:10:52Z"}, root=root
    )["pos"]
    assert aberta == (2, 0) and fechada == (1, 0)


def test_the_real_transcripts_reproduce_the_frozen_baseline():
    """Positive control on the live acervo: this counter, this window, the numbers frozen on 09/10."""
    base = V._c1_amostras({"deploy_utc": DEPLOY})["base"]
    assert base == (V.BASELINE_FROZEN["calls"], V.BASELINE_FROZEN["chars"])
