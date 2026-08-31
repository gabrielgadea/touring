#!/usr/bin/env python3
"""Testes do painel_emanacao (F0 — Atziluth, o Painel de Emanação).

O contrato central: o painel JAMAIS bloqueia uma sessão (fail-open em toda
coleta) e sua adesão é medida desde o dia 1 (journal de emissão). Subprocess
é mockado — nenhum teste chama o touring real.
"""

from __future__ import annotations

import io
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import painel_emanacao as pe


def _kpi_result(checks):
    r = mock.Mock()
    r.stdout = json.dumps({"checks": checks})
    return r


class GutTests(unittest.TestCase):
    def test_fail_ordena_acima_de_retomar_acima_de_stub(self):
        itens = [
            {"titulo": "KPI STUB: x", "fonte": "kpi", "g": 2, "u": 2, "t": 2, "extra": ""},
            {"titulo": "KPI FAIL: y", "fonte": "kpi", "g": 5, "u": 5, "t": 4, "extra": ""},
            {"titulo": "RETOMAR: z", "fonte": "f.md", "g": 4, "u": 3, "t": 3, "extra": ""},
        ]
        texto = pe.render(itens, {})
        pos_fail = texto.index("KPI FAIL")
        pos_ret = texto.index("RETOMAR")
        pos_stub = texto.index("KPI STUB")
        self.assertLess(pos_fail, pos_ret)
        self.assertLess(pos_ret, pos_stub)

    def test_kpi_fail_advisory_nao_vira_item_grave(self):
        checks = [
            {"id": "a", "status": "FAIL", "advisory": True, "actual": 0.1, "threshold": 0.9},
            {"id": "b", "status": "FAIL", "advisory": False, "actual": 0.2, "threshold": 0.9},
        ]
        with mock.patch.object(pe.subprocess, "run", return_value=_kpi_result(checks)):
            itens = pe.coletar_kpi()
        titulos = [i["titulo"] for i in itens]
        self.assertIn("KPI FAIL: b", titulos)
        self.assertNotIn("KPI FAIL: a", titulos)

    def test_stub_carrega_a_causa_quando_existe(self):
        checks = [{"id": "c", "status": "STUB", "stub_reason": "never measured — drop the file"}]
        with mock.patch.object(pe.subprocess, "run", return_value=_kpi_result(checks)):
            itens = pe.coletar_kpi()
        self.assertIn("never measured", itens[0]["extra"])


class RetomarTests(unittest.TestCase):
    def test_parser_le_titulo_e_idade_do_arquivo(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp) / "docs" / "plans" / "2026-01-01-slug"
            d.mkdir(parents=True)
            (d / "RETOMAR-AQUI.md").write_text("# Retomar: afordância X\ncorpo",
                                               encoding="utf-8")
            itens = pe.coletar_retomar(Path(tmp))
        self.assertEqual(len(itens), 1)
        self.assertIn("afordância X", itens[0]["titulo"])
        self.assertIn("d parado", itens[0]["extra"])
        self.assertEqual(itens[0]["g"], 4)

    def test_frontmatter_okf_nunca_vira_titulo(self):
        # O smoke da estreia mostrou "---" e "<!-- OKF document -->" como
        # títulos — o parser deve pular frontmatter e achar o heading real.
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp) / "docs" / "plans" / "2026-01-02-okf"
            d.mkdir(parents=True)
            (d / "RETOMAR-AQUI.md").write_text(
                "---\ntype: Note\n---\n<!-- OKF document -->\n\n# Retomar: o plano Y\n",
                encoding="utf-8")
            itens = pe.coletar_retomar(Path(tmp))
        self.assertIn("o plano Y", itens[0]["titulo"])

    def test_retomar_sem_heading_usa_o_nome_do_bundle(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp) / "docs" / "plans" / "2026-01-03-sem-heading"
            d.mkdir(parents=True)
            (d / "RETOMAR.md").write_text("---\nsó frontmatter\n", encoding="utf-8")
            itens = pe.coletar_retomar(Path(tmp))
        self.assertIn("2026-01-03-sem-heading", itens[0]["titulo"])


class FailOpenTests(unittest.TestCase):
    def test_coleta_kpi_quebrada_devolve_vazio_nunca_explode(self):
        with mock.patch.object(pe.subprocess, "run",
                               side_effect=subprocess.TimeoutExpired("touring", 4)):
            self.assertEqual(pe.coletar_kpi(), [])

    def test_painel_emite_mesmo_com_todas_as_coletas_quebradas(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = {"COGNICAO_JOURNAL": str(Path(tmp) / "j.jsonl")}
            with mock.patch.dict(os.environ, env), \
                 mock.patch.object(pe.subprocess, "run", side_effect=OSError("boom")), \
                 mock.patch.object(pe.sys, "stdin",
                                   io.StringIO(json.dumps({"cwd": tmp, "session_id": "s1"}))), \
                 mock.patch.object(pe.sys, "stdout", new_callable=io.StringIO) as out:
                rc = pe.main()
            self.assertEqual(rc, 0)
            payload = json.loads(out.getvalue())
            ctx = payload["hookSpecificOutput"]["additionalContext"]
            self.assertIn("ATZILUTH", ctx)
            self.assertIn("campo limpo", ctx)

    def test_kill_switch_silencia_sem_erro(self):
        with mock.patch.dict(os.environ, {"PAINEL_EMANACAO_DISABLED": "1"}), \
             mock.patch.object(pe.sys, "stdout", new_callable=io.StringIO) as out:
            self.assertEqual(pe.main(), 0)
        self.assertEqual(out.getvalue(), "")


class AdesaoTests(unittest.TestCase):
    def test_toda_emissao_grava_no_journal_o_medidor_do_dia_1(self):
        with tempfile.TemporaryDirectory() as tmp:
            journal = Path(tmp) / "j.jsonl"
            env = {"COGNICAO_JOURNAL": str(journal)}
            with mock.patch.dict(os.environ, env), \
                 mock.patch.object(pe.subprocess, "run",
                                   return_value=_kpi_result([])), \
                 mock.patch.object(pe.sys, "stdin",
                                   io.StringIO(json.dumps({"cwd": tmp, "session_id": "sess-abc"}))), \
                 mock.patch.object(pe.sys, "stdout", new_callable=io.StringIO):
                pe.main()
            linhas = [json.loads(x) for x in
                      journal.read_text(encoding="utf-8").splitlines()]
        emitidos = [x for x in linhas if x["kind"] == "painel_emitido"]
        self.assertEqual(len(emitidos), 1)
        self.assertEqual(emitidos[0]["session"], "sess-abc")

    def test_o_espelho_aparece_quando_ha_serie(self):
        espelho = {"n": 12, "ratio_medio": 0.43, "ratio_ultimos5": 0.6,
                   "mais_ausentes": ["fronteira", "pronto"], "registros": 3}
        texto = pe.render([], espelho)
        self.assertIn("Espelho (F4)", texto)
        self.assertIn("0.43", texto)
        self.assertIn("fronteira", texto)

    def test_o_par_cross_dominio_vira_oferta_de_isomorfismo(self):
        espelho = {"n": 4, "ratio_medio": 0.5, "ratio_ultimos5": 0.5,
                   "mais_ausentes": [], "registros": 0,
                   "par_cross_dominio": {
                       "a": {"dominio": "negocio", "hash": "h1",
                             "ausentes": ["imagem"]},
                       "b": {"dominio": "codigo", "hash": "h2",
                             "ausentes": ["fronteira", "pronto"]}}}
        texto = pe.render([], espelho)
        self.assertIn("Isomorfismo?", texto)
        self.assertIn("negocio", texto)
        self.assertIn("mesma forma?", texto)


class DagTests(unittest.TestCase):
    def _marker(self, tmp: Path, cwd: str, task="task_9", status="active"):
        d = tmp / "markers"
        d.mkdir(exist_ok=True)
        (d / "active-abc-def.json").write_text(json.dumps(
            {"task": task, "cwd": cwd, "status": status}), encoding="utf-8")
        return d

    def test_dag_do_projeto_vira_item_com_etapa_e_proxima(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmpp = Path(tmp)
            markers = self._marker(tmpp, cwd=str(tmpp / "proj"))
            dag = {"description": "Grande criação", "subtasks": [
                {"subtask_id": "a", "status": "completed", "description": "feita"},
                {"subtask_id": "b", "status": "pending", "description": "a próxima"},
            ]}
            r = mock.Mock()
            r.stdout = json.dumps(dag)
            with mock.patch.object(pe.Path, "home", return_value=tmpp), \
                 mock.patch.object(pe.subprocess, "run", return_value=r), \
                 mock.patch.object(pe, "TIMEOUT_COLETA", 1):
                # o glob procura em <home>/.claude/loop-engineering
                real = tmpp / ".claude" / "loop-engineering"
                real.mkdir(parents=True)
                (real / "active-x-y.json").write_text(
                    (markers / "active-abc-def.json").read_text(), encoding="utf-8")
                itens = pe.coletar_dags(tmpp / "proj")
        self.assertEqual(len(itens), 1)
        self.assertIn("Grande criação", itens[0]["titulo"])
        self.assertIn("1/2 done", itens[0]["extra"])
        self.assertIn("a próxima", itens[0]["extra"])

    def test_marker_de_outro_projeto_ou_convergido_e_ignorado(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmpp = Path(tmp)
            real = tmpp / ".claude" / "loop-engineering"
            real.mkdir(parents=True)
            (real / "active-1.json").write_text(json.dumps(
                {"task": "task_1", "cwd": "/outro", "status": "active"}))
            (real / "active-2.json").write_text(json.dumps(
                {"task": "task_2", "cwd": str(tmpp / "proj"),
                 "status": "CONVERGED"}))
            with mock.patch.object(pe.Path, "home", return_value=tmpp), \
                 mock.patch.object(pe.subprocess, "run",
                                   side_effect=AssertionError("não deve chamar")):
                itens = pe.coletar_dags(tmpp / "proj")
        self.assertEqual(itens, [])


if __name__ == "__main__":
    unittest.main()
