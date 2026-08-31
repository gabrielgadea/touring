#!/usr/bin/env python3
"""Testes do cognicao_formal (F4 — ciclo de medição formal).

Cada teste declara o comportamento no nome; o journal usa tempdir via
``COGNICAO_JOURNAL`` e a memória do touring é desligada por
``COGNICAO_NO_MEMORY`` — nenhum teste toca estado real do harness.
"""

from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path

import cognicao_formal as cf


class EsquemaTests(unittest.TestCase):
    def test_o_esquema_tem_7_operacoes_com_nomes_evocaveis_unicos(self):
        self.assertEqual(len(cf.ESQUEMA), 7)
        chaves = [op["chave"] for op in cf.ESQUEMA]
        nomes = [op["nome"] for op in cf.ESQUEMA]
        self.assertEqual(len(set(chaves)), 7, "chaves duplicadas")
        self.assertEqual(len(set(nomes)), 7, "nomes duplicados")
        for op in cf.ESQUEMA:
            self.assertTrue(op["pergunta"].strip(), f"{op['chave']} sem pergunta")


class MedirTests(unittest.TestCase):
    TEXTO_COMPLETO = (
        "A intenção é criar um painel de abertura. Para que eu comece cada "
        "sessão orientado, e para quem opera o sistema todos os dias. "
        "Quando estiver pronto, deve apresentar os itens classificados — o "
        "resultado final é uma lista ordenada. Não deve incluir o scout "
        "(fora do escopo). As etapas: primeiro coletar, depois classificar, "
        "então renderizar. O risco é a coleta lenta; a alternativa descartada "
        "foi um daemon novo. Critério de aceitação: o hook responde em <4s, "
        "medido por teste."
    )

    def test_texto_cobrindo_as_7_operacoes_da_ratio_1(self):
        r = cf.medir_texto(self.TEXTO_COMPLETO)
        self.assertEqual(r["ausentes"], [], f"faltou: {r['ausentes']}")
        self.assertEqual(r["ratio"], 1.0)

    def test_texto_vazio_da_ratio_0_com_as_7_ausentes(self):
        r = cf.medir_texto("")
        self.assertEqual(r["presentes"], [])
        self.assertEqual(len(r["ausentes"]), 7)
        self.assertEqual(r["ratio"], 0.0)

    def test_deteccao_e_especifica_por_operacao(self):
        r = cf.medir_texto("Isso não deve tocar a rede — fora do escopo.")
        self.assertIn("fronteira", r["presentes"])
        self.assertNotIn("cadeia", r["presentes"])
        r2 = cf.medir_texto("Critério de aceitação: exit 0, medido por gate.")
        self.assertIn("pronto", r2["presentes"])

    def test_o_sinal_viaja_com_o_veredito(self):
        r = cf.medir_texto("As etapas do trabalho vêm em sequência de fases.")
        self.assertIn("cadeia", r["sinais"])
        self.assertTrue(r["sinais"]["cadeia"])

    def test_o_detector_se_declara_heuristico(self):
        self.assertEqual(cf.medir_texto("x")["detector"], "heuristico-lexical-v0")

    def test_formulacoes_naturais_do_baseline_real_sao_detectadas(self):
        # Falsos negativos da estreia (30/08): estas formulações estavam no
        # prompt real de Gabriel e a régua v0 não as via.
        r = cf.medir_texto("percebo que podemos aperfeiçoar a interatividade")
        self.assertIn("emanacao", r["presentes"])
        r2 = cf.medir_texto("compreender com qual objetivo, com qual finalidade")
        self.assertIn("finalidade", r2["presentes"])


class JournalRoundtripTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        os.environ["COGNICAO_JOURNAL"] = str(Path(self._tmp.name) / "j.jsonl")
        os.environ["COGNICAO_NO_MEMORY"] = "1"

    def tearDown(self):
        os.environ.pop("COGNICAO_JOURNAL", None)
        os.environ.pop("COGNICAO_NO_MEMORY", None)
        self._tmp.cleanup()

    def test_medir_gravar_e_padrao_fecham_o_ciclo(self):
        rc = cf.main(["medir", "--texto", "As etapas: primeiro a, depois b.",
                      "--gravar", "--dominio", "codigo"])
        self.assertEqual(rc, 0)
        pad = cf.padrao_data()
        self.assertEqual(pad["n"], 1)
        self.assertIn("ratio_medio", pad)
        self.assertEqual(pad["dominios"], {"codigo": 1})

    def test_registrar_alimenta_o_padrao(self):
        rc = cf.main(["registrar", "--tipo", "decisao",
                      "--texto", "aprovei F0+F4 primeiro",
                      "--pergunta-destravou", "a regua nasce antes do rito?"])
        self.assertEqual(rc, 0)
        self.assertEqual(cf.padrao_data()["registros"], 1)

    def test_padrao_sem_journal_reporta_n_zero_nunca_inventa(self):
        pad = cf.padrao_data()
        self.assertEqual(pad["n"], 0)
        self.assertIn("nota", pad)

    def test_linha_corrompida_no_journal_e_pulada(self):
        path = cf.journal_path()
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('{"kind":"antecipacao","ratio":0.5,"presentes":[]}\n'
                        "NÃO-É-JSON\n", encoding="utf-8")
        self.assertEqual(cf.padrao_data()["n"], 1)

    def test_mais_ausentes_aponta_a_operacao_que_falta(self):
        cf.main(["medir", "--texto",
                 "As etapas: primeiro a, depois b. Não deve tocar rede.",
                 "--gravar"])
        pad = cf.padrao_data()
        self.assertIn("pronto", pad["mais_ausentes"])

    def test_par_cross_dominio_junta_as_2_ultimas_de_dominios_distintos(self):
        cf.main(["medir", "--texto", "etapas: a depois b", "--gravar",
                 "--dominio", "codigo"])
        cf.main(["medir", "--texto", "não deve incluir x", "--gravar",
                 "--dominio", "codigo"])
        cf.main(["medir", "--texto", "para que a equipe decida", "--gravar",
                 "--dominio", "negocio"])
        par = cf.padrao_data()["par_cross_dominio"]
        # a = a mais recente (negocio); b = a mais recente de OUTRO domínio.
        self.assertEqual(par["a"]["dominio"], "negocio")
        self.assertEqual(par["b"]["dominio"], "codigo")
        self.assertIn("imagem", par["a"]["ausentes"])

    def test_par_cross_dominio_ausente_com_um_so_dominio(self):
        cf.main(["medir", "--texto", "x", "--gravar", "--dominio", "codigo"])
        cf.main(["medir", "--texto", "y", "--gravar", "--dominio", "codigo"])
        self.assertNotIn("par_cross_dominio", cf.padrao_data())


if __name__ == "__main__":
    unittest.main()
