"""The Python side of the injection policy journals its cuts (28/10/2026).

Same file and schema as `touring_foundation::injection_journal` (`ts`, `level`, `site`,
`cuts[{family, bytes}]`), so the dose KPI and the A3 analysis see both sides. Also pins
the envelope collapse that the Rust side already did: after a cut, a
`hookSpecificOutput` left with only `hookEventName` leaves, and the envelope is `{}`.
"""
import json
import os
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import injection_policy as ip

ADVISORY = "[delegation-nudge] 6 leituras em 3 min — delegue a um subagente."


@pytest.fixture
def home(tmp_path, monkeypatch):
    monkeypatch.setenv("HOME", str(tmp_path))
    monkeypatch.delenv(ip.JOURNAL_DISABLED_ENV, raising=False)
    return tmp_path


def _lines(home):
    path = home / ".claude" / "touring" / ip.JOURNAL_FILE
    if not path.exists():
        return []
    return [json.loads(x) for x in path.read_text().splitlines() if x.strip()]


def test_filter_cut_is_journaled_with_its_bytes(home):
    out = ip.filter_output(
        "no-advisory",
        json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": ADVISORY}}),
    )
    assert out == "{}"
    (line,) = _lines(home)
    assert line["level"] == "no-advisory" and line["site"] == ip.SITE_PY_FILTER
    assert line["cuts"] == [{"family": "delegation-nudge", "bytes": len(ADVISORY)}]
    assert isinstance(line["ts"], int)


def test_full_level_never_journals(home):
    ip.filter_output("full", json.dumps({"additionalContext": ADVISORY}))
    assert ip.allows("delegation-nudge", "full")
    assert _lines(home) == []


def test_allows_refusal_is_counted_with_zero_bytes(home):
    assert not ip.allows("delegation-nudge", "no-advisory")
    (line,) = _lines(home)
    assert line["site"] == ip.SITE_PY_ALLOWS
    assert line["cuts"] == [{"family": "delegation-nudge", "bytes": 0}]


def test_decision_fields_keep_the_envelope(home):
    out = json.loads(
        ip.filter_output(
            "no-advisory",
            json.dumps(
                {
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "additionalContext": ADVISORY,
                    }
                }
            ),
        )
    )
    assert out == {"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny"}}


def test_bare_event_without_a_cut_is_left_as_is(home):
    raw = json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse"}})
    assert json.loads(ip.filter_output("no-advisory", raw)) == json.loads(raw)
    assert _lines(home) == []


def test_journal_opt_out_writes_nothing(home, monkeypatch):
    monkeypatch.setenv(ip.JOURNAL_DISABLED_ENV, "1")
    ip.allows("delegation-nudge", "no-advisory")
    assert not (home / ".claude").exists()


def test_unwritable_journal_never_raises(home):
    (home / ".claude").write_text("not a directory")
    assert not ip.allows("delegation-nudge", "no-advisory")
    assert os.path.isfile(home / ".claude")
