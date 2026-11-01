"""Parity of the SimHash port with the production Rust binary, and the properties the gate relies on.

Every assertion is POSITIVE: the vectors were produced by ``touring run`` itself
(``gerar_vetores_rust.py``), so a green run means the Python and the Rust hash agree bit for bit.
"""

from __future__ import annotations

import json
import re
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
    assert simhash.hex64(simhash.simhash64(vetor["codigo"], distintos=False)) == vetor["simhash_rust"]


@pytest.mark.parametrize("vetor", VETORES, ids=[v["nome"] for v in VETORES])
def test_simhash_v2_paridade_com_o_binario(vetor: dict) -> None:
    # 30.4.100+: the binary journals code_simhash_v2 (one vote per distinct shingle); the port's
    # distintos=True must match it bit for bit, the same way distintos=False matches v1.
    assert simhash.hex64(simhash.simhash64(vetor["codigo"], distintos=True)) == vetor["simhash_rust_v2"]


def test_default_e_o_v2() -> None:
    # Step (b), 29/10/2026: the default is the banner-robust v2; v1 must be asked for by name.
    v = next(v for v in VETORES if v["nome"] == "reguas_repetidas")
    assert simhash.hex64(simhash.simhash64(v["codigo"])) == v["simhash_rust_v2"] != v["simhash_rust"]


def test_classe_de_palavra_da_regex_e_a_do_rust_no_bmp() -> None:
    # The tokenizer regex relies on sre's \w being exactly ch.isalnum() or "_" (Rust is_alphanumeric
    # || '_', proven by the vectors); the full 0x110000 sweep lives in diagnostics/paridade_simhash_otimizado.py.
    w = re.compile(r"\w")
    discordam = [
        hex(cp)
        for cp in range(0x10000)
        if not 0xD800 <= cp <= 0xDFFF and bool(w.match(chr(cp))) != (chr(cp) == "_" or chr(cp).isalnum())
    ]
    assert discordam == []


def test_vetores_trazem_os_dois_modos() -> None:
    # A vector set without v2 would leave the v2 parity test parametrized over nothing it checks.
    assert all(v.get("simhash_rust") and v.get("simhash_rust_v2") for v in VETORES)


def test_vetores_distinguem_v1_de_v2() -> None:
    # Control for the v2 parity: on bodies without repeated shingles v1 == v2, so a binary that journaled
    # v1 under the v2 key would pass every v2 parity case. At least one vector must have v1 != v2.
    divergentes = [v["nome"] for v in VETORES if v["simhash_rust"] != v["simhash_rust_v2"]]
    assert "reguas_repetidas" in divergentes, divergentes


def test_vetores_cobrem_as_bordas_declaradas() -> None:
    # The parity only means something if the divergence-prone cases are in the set.
    nomes = {v["nome"] for v in VETORES}
    for borda in (
        "crlf",
        "acentos_nfd",
        "form_feed_e_vt",
        "separador_de_linha_unicode",
        "nbsp_meio_da_linha",
        "um_token_so",
        "so_comentarios",
        "escrita_cjk",
    ):
        assert borda in nomes, borda


def test_vetor_adulterado_reprova() -> None:
    # Control: one changed bit in the expected value must be caught, or the parity test is blind.
    v = VETORES[0]
    adulterado = f"{int(v['simhash_rust'], 16) ^ 1:016x}"
    assert simhash.hex64(simhash.simhash64(v["codigo"], distintos=False)) != adulterado


def test_simhash_separa_programas_e_aproxima_copia_editada() -> None:
    por_nome = {v["nome"]: int(v["simhash_rust"], 16) for v in VETORES}
    copia = simhash.hamming(por_nome["longo_realista"], por_nome["longo_realista_renomeado"])
    outro = simhash.hamming(por_nome["longo_realista"], por_nome["python_simples"])
    assert copia <= simhash.TAU_FORTE < outro


BANNER = 'print("' + "─" * 70 + '")\n'
PROG_A = BANNER * 4 + "\n".join(f"total_{i} = soma(parcelas_{i}) / quantidade" for i in range(6))
PROG_B = BANNER * 4 + "\n".join(f"for linha in arquivo_{i}:\n    escrever(linha.upper())" for i in range(6))


def test_banners_colidem_no_modo_rust_e_se_separam_no_modo_distinto() -> None:
    # The defect measured on the frozen corpus, reproduced: two unrelated programs that share only
    # decorative lines. Rust-weighted votes collapse them; distinct shingles keep them apart.
    rust = simhash.hamming(simhash.simhash64(PROG_A, distintos=False), simhash.simhash64(PROG_B, distintos=False))
    distinto = simhash.hamming(simhash.simhash64(PROG_A, distintos=True), simhash.simhash64(PROG_B, distintos=True))
    assert rust <= simhash.TAU_FORTE < distinto


def test_modo_distinto_ainda_aproxima_copia_editada() -> None:
    base = next(v["codigo"] for v in VETORES if v["nome"] == "longo_realista")
    copia = next(v["codigo"] for v in VETORES if v["nome"] == "longo_realista_renomeado")
    d = simhash.hamming(simhash.simhash64(base, distintos=True), simhash.simhash64(copia, distintos=True))
    assert d <= simhash.TAU_FORTE


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
        p.name: simhash.simhash64(json.loads(p.read_text(encoding="utf-8"))["content"], distintos=False)
        for p in sorted(CAMPEOES.glob("*.json"))
    }
    nomes = sorted(hashes)
    minima = min(simhash.hamming(hashes[a], hashes[b]) for i, a in enumerate(nomes) for b in nomes[i + 1 :])
    assert minima > simhash.TAU_FORTE


def test_cli_hamming(capsys: pytest.CaptureFixture[str]) -> None:
    assert simhash.main(["--hamming", "ff", "0f"]) == 0
    assert capsys.readouterr().out.strip() == "4"
    assert simhash.main([]) == 2
