#!/usr/bin/env python3
"""The diagnostic writer hands both documents it writes to the index (cross-audit 04/10/2026, X14).

`write_okf_diagnostic` writes `diagnostics/<scope>-<ts>.md` and, on a new bundle, `log.md`. Both are
written outside the edit tool, so 18 diagnostics and 3 logs sat in the touring workspace as
`eligible_not_indexed`. The helper is `okf_emit.index_written`; this pins that the writer calls it for
each document, so the next writer that forgets is a failing test, not a drifting metric.
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
import loop_diagnose  # noqa: E402


class DiagnosticIndexTest(unittest.TestCase):
    def test_the_diagnostic_and_the_new_log_are_both_indexed(self):
        bundle = Path(tempfile.mkdtemp(prefix="diag-index-")) / "bundle"
        digest = {
            "scope": "/tmp/x",
            "quality50": {"composite": 0.9, "tier": "Platinum", "blockers": 0, "warnings": 0},
            "health": {"composite_health": 0.8, "symbols": 1},
            "wiring": {"orphans": 0},
            "memory": {"hits": []},
        }
        seen: list[Path] = []
        with mock.patch.object(loop_diagnose.okf_emit, "index_written",
                               side_effect=lambda p: seen.append(Path(p)) or {"indexed": True}), \
                mock.patch.object(loop_diagnose.okf_emit, "register_artifact", return_value={}):
            loop_diagnose.write_okf_diagnostic(bundle, "plan", digest, "2026-10-04T16:00:00-03:00")
        names = sorted(p.name if p.parent.name != "diagnostics" else "diagnostic" for p in seen)
        self.assertEqual(names, ["diagnostic", "log.md"], f"each written document is indexed: {seen}")


if __name__ == "__main__":
    unittest.main()
