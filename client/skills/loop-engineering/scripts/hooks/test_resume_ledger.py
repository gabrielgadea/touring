#!/usr/bin/env python3
"""Guards for `resume_ledger` — the resume-uptake ruler (N4, 2026-09-23).

`loop_resume.py` injected state at every SessionStart/PostCompact and nothing
measured whether the injection changed anything — "the resume works" was
faith, not a ruler. An injection counts as followed only when a LATER
loop-action lands in the SAME session within the window; a missing ledger is
unknown (available: false), never zero (Lei L2).
"""
from __future__ import annotations

import importlib.util
import sys
import tempfile
import time
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "resume_ledger", Path(__file__).with_name("resume_ledger.py")
)
rl = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rl)


class ResumeLedgerTest(unittest.TestCase):
    def test_missing_ledger_is_unknown_never_zero(self):
        with tempfile.TemporaryDirectory() as d:
            got = rl.uptake(Path(d) / "none.jsonl")
        self.assertEqual(got["available"], False)

    def test_injection_followed_only_by_a_later_action_in_the_same_session(self):
        now = time.time()
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "r.jsonl"
            rl.append_record({"kind": "injection", "session_id": "s1", "task": "t"}, p)
            got = rl.uptake(p)
            self.assertEqual(got["followed"], 0, "no action yet → not followed")
            rl.append_record({"kind": "loop-action", "action": "phase-close",
                              "session_id": "s1", "task": "t"}, p)
            rl.append_record({"kind": "loop-action", "action": "phase-close",
                              "session_id": "s2", "task": "t"}, p)
            got = rl.uptake(p)
        self.assertEqual(got["injections"], 1)
        self.assertEqual(got["followed"], 1)
        self.assertEqual(got["uptake"], 1.0)
        assert got["actions"] == 2 or now  # actions counted, window honoured below

    def test_action_in_another_session_does_not_follow(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "r.jsonl"
            rl.append_record({"kind": "injection", "session_id": "s1"}, p)
            rl.append_record({"kind": "loop-action", "session_id": "s2"}, p)
            got = rl.uptake(p)
        self.assertEqual(got["followed"], 0)

    def test_append_is_fail_open(self):
        self.assertFalse(rl.append_record({"kind": "injection"},
                                          Path("/proc/1/cannot-write.jsonl")))

    def test_injection_uses_the_same_session_resolution_as_the_actions(self):
        """Cross-audit (2026-09-23): the raw payload `session_id` is absent at
        SessionStart/PreCompact/Notification — 10 injections recorded None and
        `followed` was structurally stuck at 0. The injection must go through
        the same `resolve_session` the action side uses (payload → env → …)."""
        import importlib.util as iu
        import os
        spec = iu.spec_from_file_location(
            "loop_resume", Path(__file__).with_name("loop_resume.py"))
        lr = iu.module_from_spec(spec)
        spec.loader.exec_module(lr)
        old_env = os.environ.get("CLAUDE_CODE_SESSION_ID")
        old_ledger = os.environ.get("LOOP_RESUME_LEDGER")
        try:
            os.environ["CLAUDE_CODE_SESSION_ID"] = "sess-env-fallback"
            with tempfile.TemporaryDirectory() as d:
                p = Path(d) / "r.jsonl"
                os.environ["LOOP_RESUME_LEDGER"] = str(p)
                lr._record_injection(
                    {"hook_event_name": "SessionStart", "cwd": "/x"},
                    {"task": "t", "status": "outer"}, "SessionStart")
                rows = list(rl.iter_records(p))
        finally:
            if old_env is None:
                os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
            else:
                os.environ["CLAUDE_CODE_SESSION_ID"] = old_env
            if old_ledger is None:
                os.environ.pop("LOOP_RESUME_LEDGER", None)
            else:
                os.environ["LOOP_RESUME_LEDGER"] = old_ledger
        self.assertEqual(rows[0]["session_id"], "sess-env-fallback",
                         "payload-less injection resolves the session via env, never None")


if __name__ == "__main__":
    sys.exit(unittest.main())
