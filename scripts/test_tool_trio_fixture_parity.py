#!/usr/bin/env python3
"""The Python side of the tool-trio parity contract.

``scripts/fixtures/tool_trio_parity.jsonl`` is one replayable session; ``…expected.json`` is
what the canonical predicates (``measure_tool_trio.measure_session``) make of it. The Rust port
(``touring-hooks-shared::tool_trio``, test
``test_gate_metrics_trio_matches_measure_tool_trio_fixture``) replays the SAME two files, so a
predicate that drifts on either side breaks one of the two tests (S-11, rule D8).

Regenerate ``expected`` only when the Python predicates change ON PURPOSE, and change the Rust
port in the same commit.
"""

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import measure_tool_trio as m

FIXTURES = os.path.join(HERE, "fixtures")


def _records():
    with open(os.path.join(FIXTURES, "tool_trio_parity.jsonl"), encoding="utf-8") as fh:
        return [json.loads(line) for line in fh if line.strip()]


def _expected():
    with open(
        os.path.join(FIXTURES, "tool_trio_parity.expected.json"), encoding="utf-8"
    ) as fh:
        return json.load(fh)


def test_python_predicates_still_produce_the_shared_expected_file():
    stats = {}
    m.measure_session(_records(), stats)
    got = {
        cls: {
            "calls": s.calls,
            "retry": s.retry,
            "erro": s.outcomes["erro"],
            "vazio": s.outcomes["vazio"],
            "negado": s.outcomes["negado"],
            "ok": s.outcomes["ok"],
            "result_bytes": s.result_bytes,
        }
        for cls, s in stats.items()
    }
    assert got == _expected()


def test_the_fixture_exercises_every_outcome_and_the_retry_window():
    rows = _expected().values()
    for field in ("erro", "vazio", "negado", "ok", "retry"):
        assert sum(r[field] for r in rows) > 0, f"fixture never produces {field}"
    assert len(_expected()) >= 15, "fixture shrank: it must keep covering many classes"


if __name__ == "__main__":
    fails = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"PASS {name}")
            except AssertionError as e:
                fails += 1
                print(f"FAIL {name}: {e!r}")
    sys.exit(1 if fails else 0)
