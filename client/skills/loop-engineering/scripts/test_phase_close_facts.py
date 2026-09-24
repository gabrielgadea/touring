#!/usr/bin/env python3
"""Guards for `validate_facts` and its position in the close sequence.

The defect these tests encode (analise-d4, verified by touring-36, 2026-09-24):
`update_dag` ran on line ~645 and `validate_facts` only on ~673, inside the
bundle block. A malformed `--facts` (a PATH — the natural mistake of whoever
saw `--gates`/`--abstract` accept files) left the DAG `done` and stopped right
there: no memory, no reward, no OKF report, no log — a silent partial effect,
and a bare JSONDecodeError without the format the docstring promised.

The contract after the fix: `--facts` accepts inline JSON OR a file path (the
`--gates`/`--abstract` shape); validation runs BEFORE any effect; every refusal
teaches the format. Each refusal test is mutation-sensitive: move the call
back below the effects or restore the bare `json.loads` and they fail.
"""
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "loop_phase_close", Path(__file__).with_name("loop_phase_close.py")
)
pc = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pc)

VALID = '[{"chave":"testes","valor":"546 pass","run_id":"run-123"}]'


class ValidateFactsTest(unittest.TestCase):
    def test_inline_list_parses(self):
        got = pc.validate_facts(VALID)
        self.assertEqual(got, [{"chave": "testes", "valor": "546 pass",
                                "run_id": "run-123"}])

    def test_empty_is_none(self):
        self.assertIsNone(pc.validate_facts(None))
        self.assertIsNone(pc.validate_facts(""))

    def test_a_dict_dies_naming_the_list(self):
        """The 25/08 lesson: 'fatos' sounds like a map; the refusal must say LIST."""
        with self.assertRaises(SystemExit) as ctx:
            pc.validate_facts('{"chave":"testes"}')
        msg = str(ctx.exception.code)
        self.assertIn("LISTA", msg)
        self.assertIn("dict", msg)
        self.assertIn("Formato:", msg)

    def test_a_path_to_nowhere_dies_teaching_both_forms(self):
        """The 24/09 defect: a PATH died as bare JSONDecodeError with no format."""
        with self.assertRaises(SystemExit) as ctx:
            pc.validate_facts("/tmp/onde-estao-os-fatos.json")
        msg = str(ctx.exception.code)
        self.assertIn("arquivo", msg, "the refusal must name the file form")
        self.assertIn("Formato", msg, "the refusal must teach the format")

    def test_broken_json_dies_teaching_the_format(self):
        with self.assertRaises(SystemExit) as ctx:
            pc.validate_facts('[{"chave": ')
        self.assertIn("Formato:", str(ctx.exception.code))

    def test_a_file_path_is_read_and_validated(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "facts.json"
            p.write_text(VALID, encoding="utf-8")
            got = pc.validate_facts(str(p))
        self.assertEqual(got[0]["run_id"], "run-123")

    def test_a_file_with_broken_json_dies_teaching(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "facts.json"
            p.write_text("{not json", encoding="utf-8")
            with self.assertRaises(SystemExit) as ctx:
                pc.validate_facts(str(p))
        self.assertIn("Formato:", str(ctx.exception.code))

    def test_a_file_with_a_dict_dies_naming_the_list(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "facts.json"
            p.write_text('{"chave":"x"}', encoding="utf-8")
            with self.assertRaises(SystemExit) as ctx:
                pc.validate_facts(str(p))
        self.assertIn("LISTA", str(ctx.exception.code))

    def test_items_must_be_objects(self):
        with self.assertRaises(SystemExit) as ctx:
            pc.validate_facts('["testes"]')
        self.assertIn("objeto", str(ctx.exception.code))


class OrderingTest(unittest.TestCase):
    """THE regression test: a bad --facts must die BEFORE any effect runs.

    Mutation-proven: with the validation back inside the bundle block (the
    24/09 shape), `update_dag` IS called here and this test fails.
    """

    def _stub_effects(self):
        calls: list[str] = []
        names = ["update_dag", "store_memory", "link_provenance",
                 "apply_derived_links", "harvest_candidates", "reward",
                 "credit_recalls", "resolve_subtask_id"]
        originals = {n: getattr(pc, n) for n in names}
        for n in names:
            # dag_updated/memory_stored feed the exit code — True keeps rc==0.
            setattr(pc, n, lambda *a, _n=n, **k: calls.append(_n) or True)
        return calls, originals

    def _restore(self, originals):
        for n, fn in originals.items():
            setattr(pc, n, fn)

    def test_malformed_facts_die_before_any_effect(self):
        calls, originals = self._stub_effects()
        try:
            with self.assertRaises(SystemExit) as ctx:
                pc.main(["--task", "task_x", "--phase", "P1",
                         "--facts", "/tmp/um-caminho-qualquer.json", "--quiet"])
            self.assertIn("Formato", str(ctx.exception.code))
        finally:
            self._restore(originals)
        self.assertEqual(calls, [],
                         f"nenhum efeito pode ter rodado; rodaram: {calls}")

    def test_valid_facts_from_a_file_reach_the_report(self):
        calls, originals = self._stub_effects()
        # register_artifact talks to the daemon; stub it too (fail-open shape).
        orig_register = pc.okf_emit.register_artifact
        pc.okf_emit.register_artifact = lambda *a, **k: {"memory_stored": False}
        try:
            with tempfile.TemporaryDirectory() as d:
                bundle = Path(d)
                (bundle / "plan.md").write_text("# plan\n", encoding="utf-8")
                facts_file = bundle / "facts.json"
                facts_file.write_text(VALID, encoding="utf-8")
                rc = pc.main(["--task", "task_x", "--phase", "P9",
                              "--summary", "fechamento com fatos",
                              "--bundle", str(bundle),
                              "--facts", str(facts_file), "--quiet"])
                report = (bundle / "phases" / "P9.md").read_text(encoding="utf-8")
            self.assertEqual(rc, 0, "fechamento persistido (stubs) sai 0")
            self.assertIn("Fatos com endereço", report)
            self.assertIn("run-123", report)
        finally:
            pc.okf_emit.register_artifact = orig_register
            self._restore(originals)


if __name__ == "__main__":
    unittest.main()
