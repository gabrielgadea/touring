"""HORUST-REAL — oráculo rustc em JSON (TDD): parse, contrato de sinal, validade por arquivo, bisseção da cascata e restauração por SHA.
O módulo sob teste vem de `$HORUST_SRC_DIR` (os mutantes rodam sobre uma cópia).
"""

import hashlib
import json
import os
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(Path(os.environ.get("HORUST_SRC_DIR", HERE))))
sys.path.insert(0, os.environ.get("HORUST_SCRIPTS_DIR") or str(HERE.parent.parent / "scripts"))
import chama_oraculo  # noqa: E402
import oraculo_cargo_json as oc  # noqa: E402


def msg(arq, linha, codigo, texto, nivel="error", primario=True, col=5):
    return json.dumps({
        "reason": "compiler-message",
        "message": {
            "message": texto, "level": nivel,
            "code": {"code": codigo, "explanation": None} if codigo else None,
            "spans": [{"file_name": arq, "line_start": linha, "line_end": linha, "column_start": col, "is_primary": primario}],
            "children": [],
        },
    })


FIM_OK = json.dumps({"reason": "build-finished", "success": True})
FIM_RUIM = json.dumps({"reason": "build-finished", "success": False})


# ------------------------------------------------------------------ parse


def test_parse_pega_so_erro_com_span_primario_e_deduplica():
    saida = "\n".join([
        msg("a.rs", 3, "E0425", "cannot find value `xx` in this scope"),
        msg("a.rs", 3, "E0425", "cannot find value `xx` in this scope"),  # lib + teste: mesmo diagnóstico
        msg("a.rs", 9, "E0425", "aviso", nivel="warning"),
        msg("b.rs", 1, None, "aborting due to 1 previous error", primario=False),
        "isto nao e json",
        FIM_RUIM,
    ])
    s = oc.parse_saida(saida)
    assert [(d.arq, d.linha, d.codigo) for d in s.diags] == [("a.rs", 3, "E0425")]
    assert s.terminou is True and s.sucesso is False and s.ruins == 1


def test_parse_erro_de_sintaxe_sem_codigo_continua_erro():
    s = oc.parse_saida(msg("a.rs", 4, None, "expected `;`") + "\n" + FIM_RUIM)
    assert [(d.codigo, d.linha) for d in s.diags] == [("", 4)]


def test_parse_sem_build_finished_diz_que_nao_terminou():
    s = oc.parse_saida(msg("a.rs", 1, "E0425", "x"))
    assert s.terminou is False and s.sucesso is None


def test_parse_normaliza_caminho():
    s = oc.parse_saida(msg("./crates/x/src/a.rs", 1, "E0425", "x") + "\n" + FIM_RUIM)
    assert s.diags[0].arq == "crates/x/src/a.rs"


# ------------------------------------------------------------------ contrato


def test_contrato_cargo_json_recusa_rc0_sem_build_finished():
    with pytest.raises(chama_oraculo.SinalVazio):
        chama_oraculo.chama_oraculo("cargo_json", [sys.executable, "-c", "pass"])


def test_contrato_cargo_json_aceita_o_sinal_no_stdout():
    r = chama_oraculo.chama_oraculo("cargo_json", [sys.executable, "-c", f"print({FIM_OK!r})"])
    assert r["rc"] == 0 and "build-finished" in r["stdout"]


def test_contrato_nao_aceita_o_sinal_so_no_stderr():
    with pytest.raises(chama_oraculo.SinalVazio):
        chama_oraculo.chama_oraculo("cargo_json", [sys.executable, "-c", f"import sys; print({FIM_OK!r}, file=sys.stderr)"])


# ------------------------------------------------------------------ validar


def inj(codigo="E0425", linha=10, nome="calcula_totel", arq="crates/x/src/a.rs", id_="i1"):
    return oc.Injecao(id_, arq, "nome", "nome_valor", codigo, linha, nome, "antes", "depois")


def d(linha=10, codigo="E0425", texto="cannot find value `calcula_totel` in this scope", arq="crates/x/src/a.rs"):
    return oc.Diag(arq, linha, 5, codigo, texto)


def test_validar_aceita_o_diagnostico_exato():
    assert oc.validar(inj(), [d()]) == ""


@pytest.mark.parametrize(
    "diags,motivo",
    [
        ([], "sem_erro"),
        ([d(), d(linha=12)], "outro_erro_no_arquivo"),
        ([d(codigo="E0423")], "codigo_inesperado"),
        ([d(linha=11)], "linha_diferente"),
        ([d(texto="cannot find value `outro` in this scope")], "nome_fora_da_mensagem"),
    ],
)
def test_validar_recusa_com_motivo_nomeado(diags, motivo):
    assert oc.validar(inj(), diags) == motivo


def test_validar_nome_e_token_inteiro_nao_pedaco():
    assert oc.validar(inj(nome="calc"), [d(texto="cannot find value `calcula_totel` in this scope")]) == "nome_fora_da_mensagem"


def test_validar_import_aceita_nome_dentro_do_caminho_da_mensagem():
    i = oc.Injecao("i", "a.rs", "import", "import_use", "E0432", 3, "Gamma", "a", "b")
    assert oc.validar(i, [oc.Diag("a.rs", 3, 5, "E0432", "unresolved import `crate::alfa::Gamma`")]) == ""


def test_validar_e0063_aceita_o_literal_acima_ate_60_linhas_e_recusa_alem_ou_abaixo():
    i = oc.Injecao("i", "a.rs", "kwarg", "kwarg_remove", "E0063", 100, "Config", "a", "b")
    txt = "missing field `limite` in initializer of `Config`"
    assert oc.validar(i, [oc.Diag("a.rs", 98, 5, "E0063", txt)]) == ""
    assert oc.validar(i, [oc.Diag("a.rs", 40, 5, "E0063", txt)]) == ""
    assert oc.validar(i, [oc.Diag("a.rs", 39, 5, "E0063", txt)]) == "linha_diferente"
    assert oc.validar(i, [oc.Diag("a.rs", 101, 5, "E0063", txt)]) == "linha_diferente"


# ------------------------------------------------------------------ atribuir e rodada


def test_atribuir_acha_diagnostico_em_arquivo_sem_injecao():
    a = inj(arq="crates/x/src/a.rs")
    res, sem_dono = oc.atribuir([a], [d(arq="crates/x/src/a.rs"), d(arq="crates/x/src/b.rs", linha=1)])
    assert res == {"i1": ""} and [x.arq for x in sem_dono] == ["crates/x/src/b.rs"]


def fabrica(culpado=None):
    """Runner falso: cada injeção gera o diagnóstico certo; a `culpado` gera, além disso, um erro no arquivo `cascata.rs`."""
    chamadas = []

    def runner(injs):
        chamadas.append([i.id for i in injs])
        out = [oc.Diag(i.arq, i.linha, 1, i.codigo, f"cannot find `{i.nome}`") for i in injs]
        if culpado and any(i.id == culpado for i in injs):
            out.append(oc.Diag("crates/x/src/cascata.rs", 1, 1, "E0432", "unresolved"))
        return out

    return runner, chamadas


def lote(n):
    return [oc.Injecao(f"i{k}", f"crates/x/src/f{k}.rs", "nome", "nome_valor", "E0425", 10 + k, f"nome{k}", "a", "b") for k in range(n)]


def test_rodada_limpa_roda_uma_vez_so():
    runner, chamadas = fabrica()
    res = oc.rodada(lote(6), runner)
    assert all(v == "" for v in res.values()) and len(chamadas) == 1


def test_rodada_com_cascata_cruzada_isola_o_culpado_e_salva_o_resto():
    runner, chamadas = fabrica(culpado="i3")
    res = oc.rodada(lote(8), runner)
    assert res["i3"] == "cascata_cruzada"
    assert all(v == "" for k, v in res.items() if k != "i3")
    assert len(chamadas) > 1 and len(res) == 8


def test_rodada_de_uma_injecao_com_cascata_e_descartada():
    runner, _ = fabrica(culpado="i0")
    assert oc.rodada(lote(1), runner) == {"i0": "cascata_cruzada"}


def test_rodada_vazia_nao_chama_o_runner():
    runner, chamadas = fabrica()
    assert oc.rodada([], runner) == {} and chamadas == []


# ------------------------------------------------------------------ restauração


def sha(b):
    return hashlib.sha256(b).hexdigest()


def prepara(tmp_path):
    arq = tmp_path / "crates/x/src/a.rs"
    arq.parent.mkdir(parents=True)
    arq.write_bytes(b"original\n")
    return oc.Injecao("i", "crates/x/src/a.rs", "nome", "nome_valor", "E0425", 1, "n", "original\n", "mutado\n"), arq


def test_aplica_e_restaura_por_sha(tmp_path):
    i, arq = prepara(tmp_path)
    with oc.aplicado(tmp_path, [i]):
        assert arq.read_text() == "mutado\n"
    assert arq.read_bytes() == b"original\n"


def test_restaura_mesmo_com_excecao_no_meio(tmp_path):
    i, arq = prepara(tmp_path)
    with pytest.raises(RuntimeError):
        with oc.aplicado(tmp_path, [i]):
            raise RuntimeError("boom")
    assert arq.read_bytes() == b"original\n"


def test_recusa_arquivo_que_nao_e_o_original_esperado(tmp_path):
    i, arq = prepara(tmp_path)
    arq.write_bytes(b"alguem mexeu\n")
    with pytest.raises(oc.RestauracaoFalhou):
        with oc.aplicado(tmp_path, [i]):
            pass
    assert arq.read_bytes() == b"alguem mexeu\n", "não pisa em arquivo que não é o esperado"


def test_restauracao_que_nao_volta_ao_sha_e_erro_alto(tmp_path, monkeypatch):
    i, arq = prepara(tmp_path)
    escreveu = Path.write_bytes

    def mente(self, dados):
        return escreveu(self, b"corrompido\n" if dados == b"original\n" else dados)

    monkeypatch.setattr(Path, "write_bytes", mente)
    with pytest.raises(oc.RestauracaoFalhou):
        with oc.aplicado(tmp_path, [i]):
            pass


def test_dois_arquivos_na_mesma_rodada_voltam_ao_original(tmp_path):
    i, a = prepara(tmp_path)
    b = tmp_path / "crates/x/src/b.rs"
    b.write_bytes(b"outro\n")
    j = oc.Injecao("j", "crates/x/src/b.rs", "nome", "nome_valor", "E0425", 1, "n", "outro\n", "mutado2\n")
    with oc.aplicado(tmp_path, [i, j]):
        assert (a.read_text(), b.read_text()) == ("mutado\n", "mutado2\n")
    assert (a.read_bytes(), b.read_bytes()) == (b"original\n", b"outro\n")


# ------------------------------------------------------------------ comando


def test_comando_do_cargo_pelo_slot_com_rotulo_e_alvo():
    c = oc.montar_comando("touring-cli", "lib", "horust-real", via_slot=True, offline=False)
    assert c[0].endswith("hw-slot") and c[1:5] == ["--class", "rust", "--wait", "7200"]
    assert c[c.index("--") + 1 :] == ["cargo", "check", "-p", "touring-cli", "--lib", "--keep-going", "--message-format=json", "--locked"]
    assert "horust-real-touring-cli" in c


def test_comando_sem_slot_e_offline_para_o_teste_sintetico():
    c = oc.montar_comando("alfa", "bins", "t", via_slot=False, offline=True)
    assert c == ["cargo", "check", "-p", "alfa", "--bins", "--keep-going", "--message-format=json", "--locked", "--offline"]


def test_contrato_cargo_json_recusa_saida_que_nao_tem_build_finished_mesmo_nao_vazia():
    with pytest.raises(chama_oraculo.SinalVazio):
        chama_oraculo.chama_oraculo("cargo_json", [sys.executable, "-c", "print('{\"reason\": \"compiler-artifact\"}')"])


def test_comando_com_varios_pacotes_leva_um_p_por_pacote():
    c = oc.montar_comando(["alfa", "beta"], "lib", "t", via_slot=False, offline=False)
    assert c == ["cargo", "check", "-p", "alfa", "-p", "beta", "--lib", "--keep-going", "--message-format=json", "--locked"]
    s = oc.montar_comando(["alfa", "beta"], "lib", "t", via_slot=True, offline=False)
    assert "t-alfa+1" in s, "o rótulo do slot diz quantos pacotes vão junto"


def test_injecao_leva_o_pacote():
    i = oc.Injecao("i", "a.rs", "nome", "nome_valor", "E0425", 1, "x", "a", "b", pkg="alfa")
    assert i.pkg == "alfa" and oc.Injecao("i", "a.rs", "nome", "nome_valor", "E0425", 1, "x", "a", "b").pkg == ""


def test_runner_recusa_injecao_sem_pacote_antes_de_tocar_no_arquivo(tmp_path):
    i, arq = prepara(tmp_path)  # Injecao sem `pkg`
    with pytest.raises(ValueError):
        oc.runner_cargo(tmp_path, tmp_path / "t", "lib", via_slot=False)([i])
    assert arq.read_bytes() == b"original\n"


def test_validar_aceita_qualquer_codigo_da_lista_de_aceitos_e_so_eles():
    i = oc.Injecao("i", "a.rs", "nome", "nome_tipo", "E0425", 10, "TipoX", "a", "b", aceitos=("E0425", "E0412"))
    msg = "cannot find type `TipoX` in this scope"
    assert oc.validar(i, [oc.Diag("a.rs", 10, 5, "E0425", msg)]) == ""
    assert oc.validar(i, [oc.Diag("a.rs", 10, 5, "E0412", msg)]) == ""
    assert oc.validar(i, [oc.Diag("a.rs", 10, 5, "E0405", msg)]) == "codigo_inesperado"
    sem = oc.Injecao("i", "a.rs", "nome", "nome_valor", "E0425", 10, "TipoX", "a", "b")
    assert oc.validar(sem, [oc.Diag("a.rs", 10, 5, "E0412", msg)]) == "codigo_inesperado", "sem lista, só o código esperado"
