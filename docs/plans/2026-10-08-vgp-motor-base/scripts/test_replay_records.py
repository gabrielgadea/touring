"""replay._records over transcripts with a malformed line (J18, 10/11/2026).

A Claude Code transcript line can be cut mid-string with the next record glued to it (an interrupted
write: `…/analise/script{"parentUuid":…`). One such line among 1.214 transcripts made `json.loads`
raise and aborted the whole held-out construction. The malformed line is skipped, recorded in
`replay.MALFORMED` and reported on stderr; every valid line still comes through.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import replay


def test_a_malformed_line_is_skipped_counted_and_the_rest_survives(tmp_path, capsys):
    good = {"message": {"content": [{"type": "tool_use", "id": "a"}]}}
    cut = '{"message": {"content": [{"type": "tool_use", "input": {"command": "ls /x/script{"parentUuid": "z"}\n'
    f = tmp_path / "s.jsonl"
    f.write_text(json.dumps(good) + "\n" + cut + json.dumps(good) + "\n", encoding="utf-8")
    replay.MALFORMED.clear()
    records = list(replay._records([f]))
    assert records == [good, good]
    assert replay.MALFORMED == [f"{f}:2"]
    assert f"{f}:2" in capsys.readouterr().err
