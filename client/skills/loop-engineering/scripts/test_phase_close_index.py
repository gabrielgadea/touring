#!/usr/bin/env python3
"""The phase close hands every document it writes to the index (blind review of the 04/10/2026 fixes, R5).

X14 wired `okf_emit.index_written` into the diagnostic writer and the turn recorder, and missed the third
writer: `loop_phase_close.py` writes `phases/<phase>.md` and `log.md` outside the edit tool, so both
landed as `eligible_not_indexed`. An appended log counts too: the index holds the content, and the
entry just written is the part a search for this phase needs.
"""
from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
os.environ["OKF_EMIT_NO_MEMORY"] = "1"
import loop_phase_close  # noqa: E402

TS = "2026-10-04T18:00:00-03:00"


class PhaseCloseIndexTest(unittest.TestCase):
    def test_the_report_the_new_log_and_the_appended_log_are_all_indexed(self):
        bundle = Path(tempfile.mkdtemp(prefix="phase-index-")) / "bundle"
        bundle.mkdir()
        seen: list[Path] = []
        with mock.patch.object(loop_phase_close.okf_emit, "index_written",
                               side_effect=lambda p: seen.append(Path(p)) or {"indexed": True}):
            report = loop_phase_close.write_phase_report(
                bundle, "plan", "P1", "done", "closed", {}, TS)
            loop_phase_close.append_log(bundle, "P1", "done", "closed", TS)
            loop_phase_close.append_log(bundle, "P2", "done", "closed too", TS)
        self.assertEqual(seen, [Path(report), bundle / "log.md", bundle / "log.md"],
                         "each write, the append included, is handed to the index")


if __name__ == "__main__":
    unittest.main()
