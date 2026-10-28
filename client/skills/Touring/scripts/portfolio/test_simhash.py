"""Parity of the SimHash port with the production Rust binary, and the properties the gate relies on.

Every assertion is POSITIVE: the vectors were produced by ``touring run`` itself
(``gerar_vetores_rust.py``), so a green run means the Python and the Rust hash agree bit for bit.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import simhash

VETORES = json.loads((AQUI / "vetores_rust.json").read_text(encoding="utf-8"))["vetores"]
CAMPEOES = Path.home() / "projects/touring/docs/plans/2026-10-27-portfolio-scripts/scripts/champions_content"


@pytest.mark.parametrize("vetor", VETORES, ids=[v["nome"] for v in VETORES])
def test_simhash_paridade_com_o_binario(vetor: dict) -> None:
    assert simhash.hex64(simhash.simhash64(vetor["codigo"])) == vetor["simhash_rust"]


def test_vetores_cobrem_as_bordas_declaradas() -> None:
    # The parity only means something if the divergence-prone cases are in the set.
    nomes = {v["nome"] for v in VETORES}
    for borda in ("crlf", "acentos_nfd", "form_feed_e_vt", "separador_de_linha_unicode",
                  "nbsp_meio_da_linha", "um_token_so", "so_comentarios", "escrita_cjk"):
        assert borda in nomes, borda


def test_vetor_adulterado_reprova() -> None:
    # Control: one changed bit in the expected value must be caught, or the parity test is blind.
    v = VETORES[0]
    adulterado = f"{int(v['simhash_rust'], 16) ^ 1:016x}"
    assert simhash.hex64(simhash.simhash64(v["codigo"])) != adulterado


def test_simhash_separa_programas_e_aproxima_copia_editada() -> None:
    por_nome = {v["nome"]: int(v["simhash_rust"], 16) for v in VETORES}
    copia = simhash.hamming(por_nome["longo_realista"], por_nome["longo_realista_renomeado"])
    outro = simhash.hamming(por_nome["longo_realista"], por_nome["python_simples"])
    assert copia <= simhash.TAU_FORTE < outro


def test_tokens_mantem_o_separador_colado_como_o_rust() -> None:
    assert simhash.tokens("foo(bar) # x") == ["foo(", "bar)", "#", "x"]
    assert simhash.tokens("# só comentário\n\n// outro") == []


def test_menos_de_tres_tokens_e_identidade_nao_semelhanca() -> None:
    assert simhash.simhash64("ok") != simhash.simhash64("ko")
    assert simhash.simhash64("") == simhash.FNV_OFFSET


@pytest.mark.skipif(not CAMPEOES.is_dir(), reason="campeões da calibração ausentes (unscored, não zero)")
def test_campeoes_reais_ficam_longe_entre_si() -> None:
    # The τ=10 calibration said distinct champions sit >= 20 bits apart; check it on the port.
    hashes = {
        p.name: simhash.simhash64(json.loads(p.read_text(encoding="utf-8"))["content"])
        for p in sorted(CAMPEOES.glob("*.json"))
    }
    nomes = sorted(hashes)
    minima = min(simhash.hamming(hashes[a], hashes[b]) for i, a in enumerate(nomes) for b in nomes[i + 1 :])
    assert minima > simhash.TAU_FORTE


def test_cli_hamming(capsys: pytest.CaptureFixture[str]) -> None:
    assert simhash.main(["--hamming", "ff", "0f"]) == 0
    assert capsys.readouterr().out.strip() == "4"
    assert simhash.main([]) == 2
