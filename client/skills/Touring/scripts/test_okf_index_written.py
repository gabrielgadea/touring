#!/usr/bin/env python3
"""`okf_emit.index_written`: every document a script emits inside a Touring project reaches the index.

Scripts write outside the edit tool, so the reindex hook never sees what they write. In the touring
workspace the turn records, the loop diagnostics and the bundle `log.md` sat on disk as
`eligible_not_indexed`, counted in the index's `disk_files` and not in `indexed_files`, and the `index`
component of `touring e2e` fell from 0,852 to 0,847 (cross-audit 04/10/2026, X14). One helper, in the
module every OKF writer already imports; outside a Touring project there is no index and nothing runs.
"""
from __future__ import annotations

import json
import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import okf_emit  # noqa: E402

os.environ["OKF_EMIT_NO_MEMORY"] = "1"


def _fake_touring(bindir: Path, log: Path, answer: str = '{"status":"ok"}', rc: int = 0) -> None:
    exe = bindir / "touring"
    exe.write_text(
        "#!/usr/bin/env python3\n"
        "import json, sys\n"
        f"open({str(log)!r}, 'a').write(json.dumps(sys.argv[1:]) + '\\n')\n"
        f"print({answer!r})\n"
        f"sys.exit({rc})\n",
        encoding="utf-8",
    )
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)


class IndexWrittenTest(unittest.TestCase):
    def _world(self, project: bool, **fake):
        tmp = Path(tempfile.mkdtemp(prefix="okf-index-"))
        if project:
            (tmp / ".touring").mkdir()
            (tmp / ".touring" / "touring.toml").write_text("[index]\n", encoding="utf-8")
        (tmp / "bin").mkdir()
        log = tmp / "calls.jsonl"
        _fake_touring(tmp / "bin", log, **fake)
        env = {**os.environ, "PATH": f"{tmp / 'bin'}{os.pathsep}{os.environ.get('PATH', '')}"}
        return tmp, log, env

    @staticmethod
    def _calls(log: Path) -> list[list[str]]:
        return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []

    def test_a_file_inside_a_touring_project_is_ingested(self):
        tmp, log, env = self._world(project=True)
        doc = tmp / "docs" / "plans" / "b" / "diagnostics" / "d.md"
        doc.parent.mkdir(parents=True)
        doc.write_text("# d\n", encoding="utf-8")
        with mock.patch.dict(os.environ, env, clear=True):
            res = okf_emit.index_written(doc)
        self.assertEqual(res, {"indexed": True})
        self.assertEqual(self._calls(log), [["index", "ingest", str(doc)]])

    def test_a_relative_path_is_ingested_as_the_file_it_names(self):
        """The ingest runs with cwd at the PROJECT ROOT, so a path relative to the caller's cwd named
        another file there (blind review of the 04/10 fixes): it is resolved before it leaves."""
        tmp, log, env = self._world(project=True)
        sub = tmp / "docs" / "plans" / "b"
        sub.mkdir(parents=True)
        (sub / "log.md").write_text("# log\n", encoding="utf-8")
        here = os.getcwd()
        try:
            os.chdir(sub)
            with mock.patch.dict(os.environ, env, clear=True):
                res = okf_emit.index_written("log.md")
        finally:
            os.chdir(here)
        self.assertEqual(res, {"indexed": True})
        self.assertEqual(self._calls(log), [["index", "ingest", str((sub / "log.md").resolve())]])

    def test_outside_a_touring_project_nothing_runs(self):
        tmp, log, env = self._world(project=False)
        doc = tmp / "d.md"
        doc.write_text("# d\n", encoding="utf-8")
        with mock.patch.dict(os.environ, env, clear=True):
            res = okf_emit.index_written(doc)
        self.assertFalse(res["indexed"])
        self.assertEqual(self._calls(log), [])

    def test_a_refused_ingest_is_reported_never_raised(self):
        tmp, log, env = self._world(project=True, answer='{"status":"refused"}', rc=0)
        doc = tmp / "d.md"
        doc.write_text("# d\n", encoding="utf-8")
        with mock.patch.dict(os.environ, env, clear=True):
            res = okf_emit.index_written(doc)
        self.assertFalse(res["indexed"])
        self.assertIn("refused", res["index_reason"])

    def test_emit_indexes_the_document_it_writes(self):
        tmp, log, env = self._world(project=True)
        out = tmp / "docs" / "audits" / "r.md"
        with mock.patch.dict(os.environ, env, clear=True):
            res = okf_emit.emit(out, "AuditReport", "t", "d")
        self.assertTrue(res.get("indexed"), res)
        self.assertEqual(self._calls(log), [["index", "ingest", str(out)]])


if __name__ == "__main__":
    unittest.main()
