#!/usr/bin/env python3
"""Suíte do okf_emit — cada garantia do emissor provada por execução.

O contrato espelha o guard de CI do workspace touring
(`scripts/test_okf_audit_reports.py`): todo doc emitido abre com `---` e
carrega type/title/description/timestamp. Aqui prova-se que o EXECUTOR torna
o defeito impossível (D8), não apenas detectável depois.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import okf_emit  # noqa: E402
from okf_emit import OkfValidationError, Section  # noqa: E402

os.environ["OKF_EMIT_NO_MEMORY"] = "1"

CAMPOS = ("type:", "title:", "description:", "timestamp:")


def _emitir(out, **kw):
    base = dict(doc_type="AuditReport", title="t", description="d")
    base.update(kw)
    return okf_emit.emit(out, base.pop("doc_type"), base.pop("title"),
                         base.pop("description"), **base)


class TestFrontmatter(unittest.TestCase):
    def test_valido_carrega_os_4_campos_do_guard(self):
        fm = okf_emit.render_frontmatter("AuditReport", "cross-audit x", "prova y")
        self.assertTrue(fm.startswith("---\n"))
        for campo in CAMPOS:
            self.assertIn(campo, fm)

    def test_campo_ausente_recusa_e_ensina_o_formato(self):
        with self.assertRaises(OkfValidationError) as cm:
            okf_emit.render_frontmatter("AuditReport", "", "d")
        msg = str(cm.exception)
        self.assertIn("title", msg)
        self.assertIn("Exemplo", msg)  # A5: a mensagem ensina, não só recusa

    def test_titulo_com_aspas_e_quebra_sai_yaml_seguro(self):
        fm = okf_emit.render_frontmatter("X", 'a "b"\nc', "d")
        # json.dumps escapa aspas e quebra — a linha title permanece uma linha
        linha = next(l for l in fm.splitlines() if l.startswith("title:"))
        self.assertIn('\\"b\\"', linha)
        self.assertIn("\\n", linha)

    def test_tag_com_cerquilha_sai_quotada(self):
        fm = okf_emit.render_frontmatter("X", "t", "d", tags=["#kind:log", "loop"])
        linha = next(l for l in fm.splitlines() if l.startswith("tags:"))
        self.assertIn('"#kind:log"', linha)
        self.assertIn(" loop", linha)


class TestEmit(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(prefix="okf-emit-"))
        self.out = self.dir / "docs" / "audits" / "cross-audit-2026-08-30.md"

    def test_documento_emitido_passa_o_contrato_do_guard(self):
        _emitir(self.out, sections=[Section("Resumo", "corpo")])
        head = self.out.read_text()[:2000]
        self.assertTrue(head.startswith("---\n"))
        fm = head.split("---", 2)[1]
        for campo in CAMPOS:
            self.assertIn(campo, fm)
        self.assertIn("## Resumo", head)

    def test_secao_json_acima_do_teto_fecha_a_fence_e_declara(self):
        grande = json.dumps({"itens": ["x" * 50] * 200})
        r = _emitir(self.out, sections=[Section("DEBT", grande, kind="json")],
                    cap=500)
        txt = self.out.read_text()
        # fence sempre par — a truncagem nunca a quebra (defeito real de 28/08)
        self.assertEqual(txt.count("```") % 2, 0)
        self.assertIn("A6: seção elidida", txt)
        self.assertIn("DEBT", r["elided"])
        sidecar = list((self.out.parent / ".full").glob("*DEBT*"))
        self.assertEqual(len(sidecar), 1)
        self.assertEqual(sidecar[0].read_text(), grande)  # íntegra, não o corte

    def test_sem_estouro_nao_ha_sidecar_nem_nota(self):
        _emitir(self.out, sections=[Section("S", "curto")])
        self.assertNotIn("A6", self.out.read_text())
        self.assertFalse((self.out.parent / ".full").exists())

    def test_proveniencia_encadeia_blake2b_do_anterior(self):
        prev = self.dir / "anterior.md"
        prev.write_text("conteudo anterior")
        _emitir(self.out, prev=prev)
        import hashlib
        esperado = hashlib.blake2b(prev.read_bytes(), digest_size=16).hexdigest()
        txt = self.out.read_text()
        self.assertIn(f"provenance_prev_blake2b: {esperado}", txt)
        self.assertIn("content_blake2b:", txt)

    def test_registro_de_memoria_respeita_o_kill_switch(self):
        r = _emitir(self.out)
        self.assertFalse(r["memory_stored"])
        self.assertEqual(r.get("memory_skipped"), "env")

    def test_chave_default_e_deterministica_por_caminho(self):
        a = okf_emit._default_key(self.out)
        b = okf_emit._default_key(self.out)
        c = okf_emit._default_key(self.dir / "outro" / self.out.name)
        self.assertEqual(a, b)
        self.assertNotEqual(a, c)  # mesmo stem, árvore distinta → chave distinta


class TestCli(unittest.TestCase):
    """O contrato que o nó `report` do ADW consome: REPORT= no stdout, exit 0/2."""

    SCRIPT = str(Path(__file__).resolve().parent / "okf_emit.py")

    def _run(self, *args):
        env = dict(os.environ, OKF_EMIT_NO_MEMORY="1")
        return subprocess.run([sys.executable, self.SCRIPT, *args],
                              capture_output=True, text=True, env=env)

    def test_sucesso_imprime_report_e_o_arquivo_passa_no_guard(self):
        with tempfile.TemporaryDirectory() as td:
            out = str(Path(td) / "docs" / "audits" / "cross-audit-x.md")
            p = self._run("--out", out, "--type", "AuditReport",
                          "--title", "cross-audit alvo",
                          "--description", "painel cego — FACT= + evidência",
                          "--field", "flow=cross-audit (adw)",
                          "--section", "PURPOSE=FACT=a=fiel ev",
                          "--section-json", "MAP={\"ok\":true}",
                          "--no-memory")
            self.assertEqual(p.returncode, 0, p.stderr)
            self.assertIn(f"REPORT={out}", p.stdout)
            head = Path(out).read_text()[:2000]
            self.assertTrue(head.startswith("---\n"))
            fm = head.split("---", 2)[1]
            for campo in CAMPOS:
                self.assertIn(campo, fm)

    def test_descricao_ausente_sai_2_com_mensagem_que_ensina(self):
        p = self._run("--out", "/tmp/x.md", "--type", "T", "--title", "t",
                      "--description", "   ", "--no-memory")
        self.assertEqual(p.returncode, 2)
        self.assertIn("description", p.stderr)
        self.assertIn("Exemplo", p.stderr)

    def test_secao_sem_igual_ensina_a_forma(self):
        p = self._run("--out", "/tmp/x.md", "--type", "T", "--title", "t",
                      "--description", "d", "--section", "sem-separador",
                      "--no-memory")
        self.assertEqual(p.returncode, 2)
        self.assertIn("NOME=CONTEUDO", p.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
