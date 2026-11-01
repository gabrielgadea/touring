#!/usr/bin/env python3
"""Guards for indexing the turn record the hook writes (cross-audit 04/10/2026, X14).

The record is written by this script, outside the edit tool, so the reindex hook never sees it. In
the touring workspace 82 of them sat on disk as `eligible_not_indexed`: they counted in the index's
`disk_files` and not in `indexed_files`, and the `index` component of `touring e2e` fell from 0,852 to
0,847. A record written inside a Touring project is now handed to `touring index ingest`; outside one
there is no index, and nothing is called (which keeps every other test here hermetic).
"""
from __future__ import annotations

import json
import os
import stat
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

sys.path.insert(0, str(Path(__file__).parent))
import loop_turn_record as record  # noqa: E402


def _transcript(dirpath: Path) -> Path:
    p = dirpath / "t.jsonl"
    turns = [
        {"type": "user", "message": {"content": [{"type": "text", "text": "a tarefa"}]}},
        {"type": "assistant", "message": {"content": [{"type": "text", "text": "feito: **2 verdes**"}]}},
    ]
    p.write_text("\n".join(json.dumps(t) for t in turns), encoding="utf-8")
    return p


def _fake_touring(bindir: Path, log: Path) -> None:
    """A `touring` that records each call's argv, one JSON line per call, and answers ok."""
    exe = bindir / "touring"
    exe.write_text(
        "#!/usr/bin/env python3\n"
        "import json, sys\n"
        f"open({str(log)!r}, 'a').write(json.dumps(sys.argv[1:]) + '\\n')\n"
        "print(json.dumps({'status': 'ok'}))\n",
        encoding="utf-8",
    )
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)


class TurnRecordIngestTest(unittest.TestCase):
    def _run(self, project: bool) -> tuple[dict, list[list[str]]]:
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            if project:
                (root / ".touring").mkdir()
                (root / ".touring" / "touring.toml").write_text("[index]\n", encoding="utf-8")
            bindir = root / "bin"
            bindir.mkdir()
            log = root / "calls.jsonl"
            _fake_touring(bindir, log)
            out = root / "docs" / "plans" / "b" / "turns" / "turn-x.md"
            env = {**os.environ, "PATH": f"{bindir}{os.pathsep}{os.environ.get('PATH', '')}"}
            with mock.patch.dict(os.environ, env, clear=True):
                res = record.materialize({"cwd": tmp, "scope": tmp}, out, _transcript(root))
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            return res, calls

    def test_a_record_written_inside_a_touring_project_is_indexed(self):
        res, calls = self._run(project=True)
        self.assertTrue(res["written"], res)
        self.assertEqual(len(calls), 1, f"exactly one ingest per write: {calls}")
        self.assertEqual(calls[0][:2], ["index", "ingest"], calls)
        self.assertTrue(calls[0][2].endswith("turns/turn-x.md"), calls)
        self.assertTrue(res.get("indexed"), res)

    def test_outside_a_touring_project_nothing_is_called(self):
        res, calls = self._run(project=False)
        self.assertTrue(res["written"], res)
        self.assertEqual(calls, [], "no index to feed, so no call")
        self.assertFalse(res.get("indexed"), res)


if __name__ == "__main__":
    unittest.main()
