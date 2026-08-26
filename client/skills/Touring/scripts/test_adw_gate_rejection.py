#!/usr/bin/env python3
"""Guards for `_store_gate_rejection` — a fonte de rótulo NEGATIVO do corpus.

A regressão que estes testes existem para impedir foi medida em 26/08/2026: das
225 memórias com `outcome_reward`, 209 valiam ≥ 0,5 e a média era 0,919 — 93%
positivas. O piso de exemplos do DSPy estava atingido e a DISCRIMINAÇÃO não, de
modo que otimizar contra esse trainset seria otimizar contra uma constante.

Uma reprovação de gate é falha real medida por código — independente de quem a
produziu, que é a condição que o survey MSR exige para evolução autônoma.
"""
from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

SPEC = importlib.util.spec_from_file_location(
    "adw", Path(__file__).with_name("adw.py"))
adw = importlib.util.module_from_spec(SPEC)
sys.modules["adw"] = adw
SPEC.loader.exec_module(adw)


class GateRejectionTest(unittest.TestCase):
    def setUp(self):
        self.calls = []
        self._real = adw.subprocess.run

        def fake_run(cmd, *a, **kw):
            self.calls.append(cmd)
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        adw.subprocess.run = fake_run
        self.ctx = SimpleNamespace(spec=SimpleNamespace(name="bugfix"))

    def tearDown(self):
        adw.subprocess.run = self._real

    def test_rejection_is_stored_as_a_negative_case(self):
        """0.0 explícito: sem ele o caso entra NULL e não é um rótulo negativo."""
        adw._store_gate_rejection(self.ctx, "verify", "cargo test: 3 failed")
        self.assertEqual(len(self.calls), 1)
        cmd = self.calls[0]
        self.assertIn("--reward", cmd)
        self.assertEqual(cmd[cmd.index("--reward") + 1], "0.0")
        self.assertIn("gate-reject:bugfix:verify", cmd)
        self.assertIn("cargo test: 3 failed", cmd)

    def test_the_rejecting_node_travels_in_the_context(self):
        """Sem o nó, o caso não diz QUEM reprovou — e vira ruído no corpus."""
        adw._store_gate_rejection(self.ctx, "conflict_check", "conflito em src/a.rs")
        cmd = self.calls[0]
        self.assertEqual(
            cmd[cmd.index("--outcome-context") + 1], "adw_gate_reject:conflict_check")

    def test_an_empty_verdict_stores_nothing(self):
        """Um gate que não disse nada não é evidência de nada."""
        adw._store_gate_rejection(self.ctx, "verify", "")
        adw._store_gate_rejection(self.ctx, "verify", "   ")
        self.assertEqual(len(self.calls), 1, "só o veredito com conteúdo grava")

    def test_a_store_failure_never_breaks_the_run(self):
        """Fail-open: um caso não gravado é um exemplo a menos, não um run quebrado."""
        def boom(*a, **kw):
            raise OSError("disk on fire")
        adw.subprocess.run = boom
        adw._store_gate_rejection(self.ctx, "verify", "algo reprovou")  # não levanta


if __name__ == "__main__":
    unittest.main(verbosity=2)
