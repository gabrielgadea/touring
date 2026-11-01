"""Tests for the timestamp mutant runner, with outputs isolated to tmp_path."""

import importlib.util
import json
import shutil
from pathlib import Path

ARQ = Path(__file__).resolve().with_name("mutantes_ts_ausente.py")
spec = importlib.util.spec_from_file_location("mutantes_ts_ausente", ARQ)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


def test_sha_muda_com_o_conteudo(tmp_path):
    arq = tmp_path / "x"
    arq.write_text("a")
    antes = mod.sha(arq)
    arq.write_text("b")
    assert mod.sha(arq) != antes


def test_main_mata_os_quatro_mutantes_sem_escrever_no_bundle(monkeypatch, tmp_path):
    monkeypatch.setattr(mod, "AQUI", tmp_path)
    for nome in ("test_ts_inline.py", "test_ts_arquivo.py"):
        shutil.copy2(ARQ.with_name(nome), tmp_path / nome)
    assert mod.main() == 0
    resultados = json.loads((tmp_path / "mutantes_ts_ausente.json").read_text())
    assert len(resultados) == 4
    assert all(r["morto"] and r["rc"] == 1 for r in resultados)
