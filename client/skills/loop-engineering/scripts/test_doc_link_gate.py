#!/usr/bin/env python3
"""Testes do loop_doc_link_gate — primeira suíte do gate (REGRA #0) + as
cláusulas F2 Δ-Yetzirah (Mundos da Criação, 30/08/2026): Strategy deve
carregar a cadeia causal (≥5 elos se→então) e Plan deve ligar os 3 níveis
(estratégico=strategy doc, operacional=DAG). Advisory por padrão; blocking
sob --strict — a escada aprovada no gate humano."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import loop_doc_link_gate as gate

FM = "---\ntype: {t}\nplan_id: pid-x\n---\n"


def make_bundle(tmp: Path, docs: dict[str, str]) -> Path:
    refs = "\n".join(f"[{n}](/{n})" for n in docs)
    (tmp / "index.md").write_text(FM.format(t="LoopBundle") + refs, encoding="utf-8")
    for name, body in docs.items():
        (tmp / name).write_text(body, encoding="utf-8")
    return tmp


CADEIA_OK = "\n".join(
    f"- se {c} → então {e}" for c, e in [
        ("o rito roda", "as operações são praticadas"),
        ("as operações são medidas antes", "o rito pergunta só o ausente"),
        ("o feedback mede a forma", "o esquema se treina"),
        ("o esquema é nomeado e variado", "ele se descola do domínio"),
        ("a antecipação sobe em domínio novo", "a transferência está provada"),
    ]
)


class WorldRitesTests(unittest.TestCase):
    def _report(self, docs, strict=False):
        with tempfile.TemporaryDirectory() as tmp:
            bundle = make_bundle(Path(tmp), docs)
            return gate.validate_bundle(bundle, strict)

    def test_strategy_sem_cadeia_avisa_mas_nao_bloqueia(self):
        r = self._report({"strategy-x.md": FM.format(t="Strategy") + "sem elos"})
        self.assertEqual(len(r["world_rites"]), 1)
        self.assertIn("cadeia causal", r["world_rites"][0])
        self.assertTrue(r["ok"], "advisory nunca bloqueia sem --strict")

    def test_strategy_sem_cadeia_bloqueia_sob_strict(self):
        r = self._report({"strategy-x.md": FM.format(t="Strategy") + "sem elos"},
                         strict=True)
        self.assertFalse(r["ok"])

    def test_strategy_com_5_elos_passa_limpa(self):
        r = self._report({"strategy-x.md": FM.format(t="Strategy") + CADEIA_OK})
        self.assertEqual(r["world_rites"], [])

    def test_quatro_elos_nao_bastam(self):
        quase = "\n".join(CADEIA_OK.splitlines()[:4])
        r = self._report({"strategy-x.md": FM.format(t="Strategy") + quase})
        self.assertEqual(len(r["world_rites"]), 1)

    def test_plan_sem_os_dois_niveis_recebe_2_avisos(self):
        r = self._report({"plan.md": FM.format(t="Plan") + "corpo sem nada"})
        self.assertEqual(len(r["world_rites"]), 2)
        self.assertTrue(any("estratégico" in w for w in r["world_rites"]))
        self.assertTrue(any("operacional" in w for w in r["world_rites"]))

    def test_plan_ligando_estrategia_e_dag_passa_limpo(self):
        body = "ver [estratégia](/strategy-x.md) · DAG task_123456"
        r = self._report({
            "plan.md": FM.format(t="Plan") + body,
            "strategy-x.md": FM.format(t="Strategy") + CADEIA_OK,
        })
        self.assertEqual(r["world_rites"], [])

    def test_docs_de_outros_tipos_nao_sao_cobrados(self):
        r = self._report({"nota.md": FM.format(t="Note") + "sem elos, sem dag"})
        self.assertEqual(r["world_rites"], [])

    def test_o_proprio_bundle_dos_mundos_passa_no_gate(self):
        # Dogfood: o bundle real da estratégia deve cumprir o que ela cobra.
        bundle = Path.home() / "projects/touring/docs/plans/2026-08-30-mundos-da-criacao"
        if not bundle.is_dir():
            self.skipTest("bundle real ausente (out-of-repo run)")
        r = gate.validate_bundle(bundle, strict=False)
        self.assertEqual(
            r["world_rites"], [],
            "o bundle da estratégia dos Mundos viola o rito que ele mesmo define",
        )


if __name__ == "__main__":
    unittest.main()
