"""Testes do aceite J17: o critério é o do J15 (V3-QUENTE); entrada ausente nunca vira PRONTO."""

import json
from pathlib import Path

import consolidar_j17

CHAMADAS = 1035


def frio(*pendencias):
    return {
        "pronto": not pendencias,
        "pendencias": list(pendencias),
        "inconclusivo": False,
        "inconclusivo_motivo": "",
        "chamadas": CHAMADAS,
        "p95_ms": 2000.0,
    }


def quente(**mudancas):
    base = {
        "valido": True,
        "p95_motor_ms": 250.0,
        "timeouts_camada2": 0,
        "denominador": {
            "entradas_esperadas": CHAMADAS,
            "medidas_com_sombra": CHAMADAS,
            "medidas_com_camada2": CHAMADAS,
            "rotas": {"daemon": 2 * CHAMADAS},
        },
    }
    base.update(mudancas)
    return base


def grava(pasta: Path, v2: dict, v3: dict, hot: dict, candidato: str = "30_4_104"):
    (pasta / f"{candidato}_v2.json").write_text(json.dumps(v2))
    (pasta / f"{candidato}_v3.json").write_text(json.dumps(v3))
    (pasta / "hot.json").write_text(json.dumps(hot))


def test_pronto_com_p95_frio_isolado(tmp_path):
    grava(tmp_path, frio(), frio("p95 2000.0 ms > 1000 ms"), quente())
    assert consolidar_j17.consolidar(tmp_path)["veredito"] == "PRONTO"


def test_falso_deny_continua_reprovando(tmp_path):
    grava(tmp_path, frio(), frio("falso deny (arquivo) piorou: 1 > base 0"), quente())
    v = consolidar_j17.consolidar(tmp_path)
    assert v["veredito"] == "NÃO PRONTO" and v["pendencias"] == ["V3: falso deny (arquivo) piorou: 1 > base 0"]


def test_p95_quente_acima_do_limite_reprova(tmp_path):
    grava(tmp_path, frio(), frio(), quente(p95_motor_ms=1000.5))
    assert consolidar_j17.consolidar(tmp_path)["veredito"] == "NÃO PRONTO"


def test_arquivo_ausente_e_inconclusivo_nunca_pronto(tmp_path):
    grava(tmp_path, frio(), frio(), quente())
    (tmp_path / "hot.json").unlink()
    v = consolidar_j17.consolidar(tmp_path)
    assert v["veredito"] == "INCONCLUSIVO" and "hot.json" in v["inconclusivo_motivos"][0]


def test_json_ilegivel_e_inconclusivo(tmp_path):
    grava(tmp_path, frio(), frio(), quente())
    (tmp_path / "30_4_104_v3.json").write_text("{truncado")
    assert consolidar_j17.consolidar(tmp_path)["veredito"] == "INCONCLUSIVO"


def test_o_nome_do_candidato_e_parametro(tmp_path):
    grava(tmp_path, frio(), frio(), quente(), candidato="30_4_102")
    assert consolidar_j17.consolidar(tmp_path)["veredito"] == "INCONCLUSIVO"
    assert consolidar_j17.consolidar(tmp_path, "30_4_102")["veredito"] == "PRONTO"


def test_denominador_da_medicao_quente_que_nao_fecha_e_inconclusivo(tmp_path):
    hot = quente()
    hot["denominador"]["medidas_com_camada2"] = CHAMADAS - 1
    grava(tmp_path, frio(), frio(), hot)
    assert consolidar_j17.consolidar(tmp_path)["veredito"] == "INCONCLUSIVO"
