#!/usr/bin/env python3
"""Guards for the gate's artifact predicate (`require_json`).

The regression: on 2026-08-25 the `explore-ledger` artifact was satisfied by
ANY fresh `*.ledger.json` in scope. A ledger for a different question, whose
exploration had NOT converged, reported `present` while the converged ledger
for the real topic sat one filename away — the gate would have let the turn end
with the exploration still open. Freshness and shape are not a verdict.
"""
from __future__ import annotations

import json
import sys
import time
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

sys.path.insert(0, str(Path(__file__).parent))
import loop_outer_gate as gate  # noqa: E402

CONVERGED = {"verdict.converged": True}


class RequireJsonTest(unittest.TestCase):
    def _ledger(self, tmp, name, converged):
        p = Path(tmp) / name
        p.write_text(json.dumps({"topic": "t", "verdict": {"converged": converged}}))
        return str(p)

    def test_converged_ledger_satisfies(self):
        with TemporaryDirectory() as tmp:
            self.assertTrue(gate._satisfies(self._ledger(tmp, "a.json", True), CONVERGED))

    def test_unconverged_ledger_does_not_satisfy(self):
        """The measured bug: a fresh but open exploration counted as done."""
        with TemporaryDirectory() as tmp:
            self.assertFalse(gate._satisfies(self._ledger(tmp, "b.json", False), CONVERGED))

    def test_missing_pointer_does_not_satisfy(self):
        """A ledger with no verdict has not stated one — absence is not success."""
        with TemporaryDirectory() as tmp:
            p = Path(tmp) / "c.json"
            p.write_text(json.dumps({"topic": "t"}))
            self.assertFalse(gate._satisfies(str(p), CONVERGED))

    def test_unparseable_file_fails_closed(self):
        with TemporaryDirectory() as tmp:
            p = Path(tmp) / "d.json"
            p.write_text("{not json")
            self.assertFalse(gate._satisfies(str(p), CONVERGED))

    def test_no_requirement_keeps_the_old_behaviour(self):
        """Artifacts without a predicate must not become harder to satisfy."""
        with TemporaryDirectory() as tmp:
            self.assertTrue(gate._satisfies(self._ledger(tmp, "e.json", False), None))

    def test_evaluate_rejects_an_unconverged_ledger_end_to_end(self):
        """The predicate must actually reach `evaluate`, not just exist."""
        with TemporaryDirectory() as tmp:
            explore = Path(tmp) / ".touring-explore"
            explore.mkdir()
            self._ledger(str(explore), "open.ledger.json", False)
            marker = {"flow": "f", "scope": tmp, "cwd": tmp,
                      "flow_armed_at": time.time() - 5}
            manifests = {"f": {"artifacts": [{
                "id": "explore-ledger",
                "glob": "{scope}/.touring-explore/*.ledger.json",
                "min": 1, "next_action": "run explore",
                "require_json": {"verdict.converged": True},
            }]}}
            report = gate.evaluate(marker, manifests)
            self.assertFalse(report["complete"])
            self.assertEqual([m["id"] for m in report["missing"]], ["explore-ledger"])

            self._ledger(str(explore), "done.ledger.json", True)
            self.assertTrue(gate.evaluate(marker, manifests)["complete"])


if __name__ == "__main__":
    unittest.main(verbosity=2)

# ── P4/S-4.1 (2026-09-04): o contrato da CLASSE de custo ─────────────────────

def _manifests():
    import json, pathlib
    return json.loads((pathlib.Path(__file__).parent / "flow_manifests.json").read_text())


def test_every_artifact_declares_its_cost_class():
    """Sem `class`, o gate assume A e cobra — mas um artefato caro nao declarado
    seria cobrado do default, que e' exatamente o que P4 desfaz. A declaracao e'
    obrigatoria, e este teste e' quem a torna obrigatoria."""
    for flow, m in _manifests().items():
        if not isinstance(m, dict) or "artifacts" not in m:
            continue
        for art in m["artifacts"]:
            assert art.get("class") in {"A", "B", "C"}, (
                f"{flow}/{art['id']} sem classe de custo declarada"
            )


def test_the_default_flow_never_charges_a_class_c_artifact():
    """O achado que originou P4: 965 avaliacoes, 74% dos runs nunca completaram,
    e o que faltava era sempre classe C — artefato que exige parar o raciocinio.
    O flow DEFAULT (`work-outer`, 392 das 965) cobra so' o que codigo produz."""
    m = _manifests()["work-outer"]
    assert m["enforced_classes"] == ["A"], m["enforced_classes"]
    cobrados = [a["id"] for a in m["artifacts"] if a["class"] in m["enforced_classes"]]
    assert cobrados, "o default ainda precisa cobrar ALGO — senao nao e' gate"
    assert "explore-ledger" not in cobrados, (
        "explore-ledger faltou 418x e 56 de 79 bundles nunca o tiveram"
    )


def test_an_explicitly_invoked_flow_still_charges_everything():
    """Quem digitou /loop-engineering PEDIU a exploracao: ali o documento e' o
    entregavel, nao um desvio. O alivio e' do default, nunca do pedido."""
    m = _manifests()["strategy-outer"]
    assert set(m["enforced_classes"]) == {"A", "B", "C"}
    assert any(a["id"] == "explore-ledger" for a in m["artifacts"])
