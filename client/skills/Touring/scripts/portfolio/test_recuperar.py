"""Contract of the single portfolio decision: the four veredito_gate/1 states, independent of the tau chosen."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import recuperar

PROGRAMA = "\n".join(f"linha_{i} = calcular(valor_{i}, base) + ajuste_{i}" for i in range(12))
EDITADO = PROGRAMA.replace("ajuste_3", "correcao_3") + "\n# comentario novo\n"
OUTRO = "\n".join(f"for item in lista_{i}:\n    print(item.nome, item.peso, item.custo)" for i in range(8))


def _c(cid: str, conteudo: str, nome: str = "x.py", proposito: str = "") -> recuperar.Candidato:
    return recuperar.Candidato(id=cid, nome=nome, conteudo=conteudo, proposito=proposito)


def test_copia_editada_e_reprovado() -> None:
    v = recuperar.decidir("novo.py", EDITADO, [_c("a", PROGRAMA), _c("b", OUTRO)])
    assert v.estado == "reprovado"
    assert v.candidatos[0][0] == "a"
    assert v.candidatos[0][1] == "corpo"
    assert v.exit_code == 1


def test_programa_distinto_e_aprovado() -> None:
    v = recuperar.decidir("novo.py", OUTRO, [_c("a", PROGRAMA)])
    assert v.estado == "aprovado"
    assert v.exit_code == 0


def test_fonte_fora_do_ar_e_inaferivel_nunca_aprovado() -> None:
    v = recuperar.decidir("novo.py", OUTRO, [_c("a", PROGRAMA)], fontes_falhas=["tantivy (TimeoutExpired)"])
    assert v.estado == "inaferivel"
    assert v.exit_code == 2
    assert "tantivy" in v.motivo


def test_evidencia_forte_vale_mesmo_com_fonte_falha() -> None:
    # Finding the equivalent in one source is evidence; the other source's failure does not erase it.
    v = recuperar.decidir("novo.py", EDITADO, [_c("a", PROGRAMA)], fontes_falhas=["tantivy (RuntimeError)"])
    assert v.estado == "reprovado"
    assert v.fontes_falhas == ("tantivy (RuntimeError)",)


def test_nao_script_nao_aplicavel() -> None:
    assert recuperar.decidir("notas.md", PROGRAMA, [_c("a", PROGRAMA)]).estado == "nao_aplicavel"


def test_corpo_curto_nunca_reprova() -> None:
    curto = "x = 1\nprint(x)\n"
    v = recuperar.decidir("novo.py", curto, [_c("a", curto)])
    assert v.estado == "aprovado"
    assert "curto" in v.motivo


def test_proposito_sozinho_aconselha_mas_nao_reprova() -> None:
    v = recuperar.decidir(
        "baixar_proveniencia.py",
        "# baixa a proveniencia das fontes oficiais\n" + OUTRO,
        [_c("a", PROGRAMA, "fetch_proveniencia.py", "baixa a proveniencia das fontes oficiais")],
    )
    assert v.estado == "aprovado"
    assert v.candidatos
    assert v.candidatos[0][1] == "proposito"


def test_fonte_manifest_ausente_e_vazia_e_ilegivel_levanta(tmp_path: Path) -> None:
    assert recuperar.fonte_manifest(tmp_path / "nao_existe.json") == []
    ruim = tmp_path / "MANIFEST.json"
    ruim.write_text("{nao é json")
    with pytest.raises(ValueError):
        recuperar.fonte_manifest(ruim)


def test_cli_exit_deriva_do_estado(tmp_path: Path) -> None:
    alvo = tmp_path / "notas.md"
    alvo.write_text("texto")
    r = subprocess.run(
        [sys.executable, str(AQUI / "recuperar.py"), "--arquivo", str(alvo)],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    assert r.returncode == 0
    assert json.loads(r.stdout)["estado"] == "nao_aplicavel"


def test_tantivy_com_cwd_removido_nao_vira_inaferivel(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    # Defect of 28/10/2026: a session directory removed before the hook ran made subprocess.run(cwd=…) raise
    # FileNotFoundError, and the whole tantivy source read as failed. The search needs no cwd.
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    falso = bin_dir / "touring"
    falso.write_text("#!/bin/sh\nprintf '[]'\n")
    falso.chmod(0o755)
    monkeypatch.setenv("PATH", f"{bin_dir}:/usr/bin:/bin")
    removido = tmp_path / "sessao_que_sumiu"
    assert not removido.exists()
    assert recuperar.fonte_tantivy("calcular ajuste base", cwd=str(removido)) == []
