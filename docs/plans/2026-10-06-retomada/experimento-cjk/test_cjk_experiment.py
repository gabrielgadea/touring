"""Tests for cjk_experiment.py — no model call is ever made here."""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import cjk_experiment as ex  # noqa: E402

CJK_A, CJK_B = chr(0x8DEF), chr(0x51B3)  # two CJK ideographs, built so this file carries none literally


class Statistics(unittest.TestCase):
    def test_fisher_greater_matches_the_hypergeometric_tail(self) -> None:
        # [[3, 1], [1, 3]]: P(X >= 3) = (16 + 1) / 70, the textbook value (R fisher.test, greater: 0.2429)
        self.assertAlmostEqual(ex.fisher_greater(3, 1, 1, 3), 17 / 70, places=12)

    def test_fisher_extreme_table_is_one_over_the_combinations(self) -> None:
        self.assertAlmostEqual(ex.fisher_greater(4, 0, 0, 4), 1 / 70, places=12)

    def test_fisher_reversed_direction_is_not_significant(self) -> None:
        self.assertGreater(ex.fisher_greater(1, 3, 3, 1), 0.9)

    def test_wilson_agrees_with_the_gate_criterion(self) -> None:
        # the gate's own function produced the 0,391 of the 04/10 replay (n 25, LEGIT 5)
        self.assertAlmostEqual(ex.wilson(5, 25)[1], ex.gate.wilson_upper(5, 25), places=12)
        self.assertAlmostEqual(round(ex.wilson(5, 25)[1], 3), 0.391)

    def test_wilson_zero_successes_starts_at_zero(self) -> None:
        lo, hi = ex.wilson(0, 30)
        self.assertEqual(lo, 0.0)
        self.assertAlmostEqual(hi, 0.1135, places=3)

    def test_holm_stops_at_the_first_failure(self) -> None:
        self.assertEqual(ex.holm([0.01, 0.04, 0.03, 0.2]), [True, False, False, False])
        self.assertEqual(ex.holm([0.001, 0.002, 0.003]), [True, True, True])

    def test_kappa_perfect_chance_and_undefined(self) -> None:
        self.assertEqual(ex.cohen_kappa([("CORRUPT", "CORRUPT"), ("LEGIT", "LEGIT")]), 1.0)
        self.assertAlmostEqual(ex.cohen_kappa([("CORRUPT", "LEGIT"), ("LEGIT", "CORRUPT")]), -1.0)
        self.assertIsNone(ex.cohen_kappa([("CORRUPT", "CORRUPT")] * 3))
        self.assertIsNone(ex.cohen_kappa([]))


class Occurrences(unittest.TestCase):
    def test_contiguous_runs_are_one_occurrence(self) -> None:
        text = f"a {CJK_A}{CJK_B}x e {CJK_B}"
        self.assertEqual(ex.occurrences(text), [(2, 4), (8, 9)])

    def test_typography_and_greek_are_not_occurrences(self) -> None:
        self.assertEqual(ex.occurrences("tau τ → « ok »"), [])


class Blinding(unittest.TestCase):
    def test_blind_file_carries_no_cell_model_or_rep(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            recs = [{"ok": True, "rep": 0, "cell": "opus/curto", "model_key": "opus", "context": "curto",
                     "result": f"texto com {CJK_A} no meio"},
                    {"ok": True, "rep": 1, "cell": "minimax/longo", "model_key": "minimax", "context": "longo",
                     "result": "texto limpo"},
                    {"ok": False, "rep": 2, "cell": "minimax/curto", "result": CJK_B}]
            (d / "runs.jsonl").write_text("".join(json.dumps(r) + "\n" for r in recs), encoding="utf-8")
            n = ex.blind(d / "runs.jsonl", d / "blind.jsonl", d / "key.jsonl")
            self.assertEqual(n, 1)  # the failed call is not an outcome
            item = json.loads((d / "blind.jsonl").read_text(encoding="utf-8"))
            self.assertEqual(set(item), {"id", "chars", "context"})
            raw = (d / "blind.jsonl").read_text(encoding="utf-8")
            for leak in ("opus", "minimax", "curto", "longo", "rep"):
                self.assertNotIn(leak, raw)
            key = json.loads((d / "key.jsonl").read_text(encoding="utf-8"))
            self.assertEqual((key["cell"], key["rep"], key["id"]), ("opus/curto", 0, item["id"]))

    def test_analyze_counts_primary_only_when_both_raters_say_corrupt(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            recs = [{"ok": True, "rep": i, "cell": "opus/curto", "result": f"x {CJK_A} y"} for i in range(2)]
            (d / "runs.jsonl").write_text("".join(json.dumps(r) + "\n" for r in recs), encoding="utf-8")
            ex.blind(d / "runs.jsonl", d / "blind.jsonl", d / "key.jsonl")
            ids = [json.loads(l)["id"] for l in (d / "key.jsonl").read_text(encoding="utf-8").splitlines()]
            (d / "a.jsonl").write_text("".join(json.dumps({"id": i, "label": "CORRUPT"}) + "\n" for i in ids),
                                       encoding="utf-8")
            (d / "b.jsonl").write_text(json.dumps({"id": ids[0], "label": "CORRUPT"}) + "\n"
                                       + json.dumps({"id": ids[1], "label": "UNCLEAR"}) + "\n", encoding="utf-8")
            res = ex.analyze(d / "runs.jsonl", d / "key.jsonl", d / "a.jsonl", d / "b.jsonl")
            cell = res["cells"]["opus/curto"]
            self.assertEqual((cell["n"], cell["primary"], cell["either"], cell["raw"]), (2, 1, 2, 2))
            self.assertFalse(res["base_rate_ok"])
            self.assertTrue(all("taxa-base" in t["verdict"] for t in res["tests"]))

    def test_analyze_refuses_an_unlabelled_occurrence(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "runs.jsonl").write_text(json.dumps({"ok": True, "rep": 0, "cell": "opus/curto",
                                                      "result": CJK_A}) + "\n", encoding="utf-8")
            ex.blind(d / "runs.jsonl", d / "blind.jsonl", d / "key.jsonl")
            (d / "a.jsonl").write_text("", encoding="utf-8")
            with self.assertRaises(ex.ExperimentError):
                ex.analyze(d / "runs.jsonl", d / "key.jsonl", d / "a.jsonl", d / "a.jsonl")


class Harness(unittest.TestCase):
    BASE = {"PATH": "/usr/bin", "MINIMAX_API_KEY": "fake-minimax-key-123", "ANTHROPIC_API_KEY": "leftover-key-9",
            "ANTHROPIC_BASE_URL": "https://example.invalid"}

    def test_opus_env_never_carries_an_api_key_or_base_url(self) -> None:
        env = ex.build_env("opus", self.BASE)
        self.assertNotIn("ANTHROPIC_API_KEY", env)
        self.assertNotIn("ANTHROPIC_BASE_URL", env)
        self.assertEqual(env["TOURING_WORK_OUTER_DISABLED"], "1")

    def test_minimax_env_points_at_the_endpoint_with_its_own_key(self) -> None:
        env = ex.build_env("minimax", self.BASE)
        self.assertEqual(env["ANTHROPIC_BASE_URL"], ex.MINIMAX_BASE_URL)
        self.assertEqual(env["ANTHROPIC_API_KEY"], "fake-minimax-key-123")
        self.assertEqual(env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], ex.MODELS["minimax"])

    def test_minimax_without_key_fails_loud(self) -> None:
        with self.assertRaises(ex.ExperimentError):
            ex.build_env("minimax", {"PATH": "/usr/bin"})

    def test_both_checkpoints_get_the_same_flags_except_the_model(self) -> None:
        a, b = ex.claude_command("opus"), ex.claude_command("minimax")
        self.assertEqual([x for x in a if x != ex.MODELS["opus"]], [x for x in b if x != ex.MODELS["minimax"]])
        self.assertIn("--no-session-persistence", a)

    def test_redact_removes_the_key_from_an_error_tail(self) -> None:
        env = ex.build_env("minimax", self.BASE)
        self.assertEqual(ex.redact("auth failed for fake-minimax-key-123", env), "auth failed for <redacted>")

    def test_parse_reply_ok_and_failure_shapes(self) -> None:
        good = json.dumps({"result": "relatorio", "is_error": False, "usage": {"output_tokens": 3},
                           "modelUsage": {"claude-opus-5-5": {}}, "total_cost_usd": 0.01})
        self.assertTrue(ex.parse_reply(0, good, "", {})["ok"])
        self.assertFalse(ex.parse_reply(0, json.dumps({"result": "", "is_error": False}), "", {})["ok"])
        self.assertFalse(ex.parse_reply(1, "not json", "boom", {})["ok"])

    def test_schedule_is_seeded_complete_and_interleaved(self) -> None:
        plan = ex.schedule()
        self.assertEqual(len(plan), 6 * ex.N_PER_CELL)
        self.assertEqual(plan, ex.schedule())
        self.assertEqual(len(set(plan)), len(plan))
        first_cells = {(m, c) for m, c, _ in plan[:30]}
        self.assertGreater(len(first_cells), 3)  # no cell runs as a block


class Sources(unittest.TestCase):
    def test_prefix_families(self) -> None:
        self.assertTrue(ex.eligible_prefix_path("docs/audits/cross-audit-2026-10-04.md"))
        self.assertFalse(ex.eligible_prefix_path("docs/audits/sub/x.md"))
        self.assertTrue(ex.eligible_prefix_path("crates/touring-cli/src/cli/context_budget.rs"))
        self.assertTrue(ex.eligible_prefix_path("crates/touring-server/src/cli/common.rs"))
        self.assertFalse(ex.eligible_prefix_path("crates/touring-server/src/cli/sub/x.rs"))

    def test_snippet_sources_exclude_docs_mirror_and_the_gate(self) -> None:
        self.assertFalse(ex.eligible_snippet_path("docs/plans/x.md"))
        self.assertFalse(ex.eligible_snippet_path("client/skills/Touring/x.md"))
        self.assertFalse(ex.eligible_snippet_path("scripts/hooks/prose_integrity_gate.py"))
        self.assertTrue(ex.eligible_snippet_path("crates/touring-code/tests/unicode.rs"))

    def test_interleave_keeps_order_and_never_leads(self) -> None:
        out = ex.interleave(["A", "B", "C", "D"], ["x", "y"])
        self.assertEqual([s for s in out if s in "ABCD"], ["A", "B", "C", "D"])
        self.assertEqual([s for s in out if s in "xy"], ["x", "y"])
        self.assertEqual(out[0], "A")


class Preregistration(unittest.TestCase):
    def _repo(self, d: Path) -> Path:
        git = ["git", "-c", "user.name=t", "-c", "user.email=t@t", "-C", str(d)]
        subprocess.run(["git", "init", "-q", str(d)], check=True)
        (d / "pre.md").write_text("v1\n", encoding="utf-8")
        return git

    def test_untracked_dirty_and_clean(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            git = self._repo(d)
            self.assertIn("não está commitado", ex.prereg_problem(d, d / "pre.md"))
            subprocess.run([*git, "add", "pre.md"], check=True)
            subprocess.run([*git, "commit", "-q", "-m", "pre"], check=True)
            self.assertIsNone(ex.prereg_problem(d, d / "pre.md"))
            (d / "pre.md").write_text("v2\n", encoding="utf-8")
            self.assertIn("difere do HEAD", ex.prereg_problem(d, d / "pre.md"))


if __name__ == "__main__":
    unittest.main()
