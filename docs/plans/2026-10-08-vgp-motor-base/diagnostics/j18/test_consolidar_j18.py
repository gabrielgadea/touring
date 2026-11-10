"""Testes do aceite J18: J15 + ABBA v3a + regressão/efeito do pré-registro."""

import json
from pathlib import Path

import consolidar_j18

CHAMADAS = 1035


def frio(*pendencias, falsos=()):
    return {
        "pronto": not pendencias,
        "pendencias": list(pendencias),
        "inconclusivo": False,
        "inconclusivo_motivo": "",
        "chamadas": CHAMADAS,
        "p95_ms": 2000.0,
        "falsos": list(falsos),
    }


def falso(identidade, mensagem="unknown import symbol"):
    return {"id": identidade, "achados": [{"sugestao": mensagem}]}


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


def grava(pasta: Path, v2: dict, v3: dict, controle: dict, hot: dict, candidato="30_4_108"):
    (pasta / f"{candidato}_v2.json").write_text(json.dumps(v2))
    (pasta / f"{candidato}_v3a.json").write_text(json.dumps(v3))
    (pasta / "30_4_104_v3a.json").write_text(json.dumps(controle))
    (pasta / "hot.json").write_text(json.dumps(hot))


def test_pronto_com_p95_frio_isolado(tmp_path):
    grava(tmp_path, frio(), frio("p95 2000.0 ms > 1000 ms"), frio(), quente())
    assert consolidar_j18.consolidar(tmp_path)["veredito"] == "PRONTO"


def test_falso_deny_continua_reprovando(tmp_path):
    grava(tmp_path, frio(), frio("falso deny (arquivo) piorou: 1 > base 0"), frio(), quente())
    assert consolidar_j18.consolidar(tmp_path)["veredito"] == "NÃO PRONTO"


def test_p95_quente_acima_do_limite_reprova(tmp_path):
    grava(tmp_path, frio(), frio(), frio(), quente(p95_motor_ms=1000.5))
    assert consolidar_j18.consolidar(tmp_path)["veredito"] == "NÃO PRONTO"


def test_arquivo_ausente_e_inconclusivo_nunca_pronto(tmp_path):
    grava(tmp_path, frio(), frio(), frio(), quente())
    (tmp_path / "hot.json").unlink()
    v = consolidar_j18.consolidar(tmp_path)
    assert v["veredito"] == "INCONCLUSIVO" and "hot.json" in v["inconclusivo_motivos"][0]


def test_controle_ausente_e_inconclusivo(tmp_path):
    grava(tmp_path, frio(), frio(), frio(), quente())
    (tmp_path / "30_4_104_v3a.json").unlink()
    assert consolidar_j18.consolidar(tmp_path)["veredito"] == "INCONCLUSIVO"


def test_json_ilegivel_e_inconclusivo(tmp_path):
    grava(tmp_path, frio(), frio(), frio(), quente())
    (tmp_path / "30_4_108_v3a.json").write_text("{truncado")
    assert consolidar_j18.consolidar(tmp_path)["veredito"] == "INCONCLUSIVO"


def test_o_nome_do_candidato_e_parametro(tmp_path):
    grava(tmp_path, frio(), frio(), frio(), quente(), candidato="30_4_102")
    assert consolidar_j18.consolidar(tmp_path)["veredito"] == "INCONCLUSIVO"
    assert consolidar_j18.consolidar(tmp_path, "30_4_102")["veredito"] == "PRONTO"


def test_denominador_da_medicao_quente_que_nao_fecha_e_inconclusivo(tmp_path):
    hot = quente()
    hot["denominador"]["medidas_com_camada2"] = CHAMADAS - 1
    grava(tmp_path, frio(), frio(), frio(), hot)
    assert consolidar_j18.consolidar(tmp_path)["veredito"] == "INCONCLUSIVO"


def test_regressao_unica_e_nao_pronto_com_id_e_mensagem(tmp_path):
    grava(tmp_path, frio(), frio(falsos=[falso("novo-falso")]), frio(), quente())
    v = consolidar_j18.consolidar(tmp_path)
    assert v["veredito"] == "NÃO PRONTO"
    assert v["regressao_vs_controle"] == [
        {"id": "novo-falso", "mensagem": "unknown import symbol"}
    ]
    assert "novo-falso" in v["pendencias"][-1]


def test_efeito_da_correcao_e_evidencia_nao_gate(tmp_path):
    grava(tmp_path, frio(), frio(), frio(falsos=[falso("corrigido")]), quente())
    v = consolidar_j18.consolidar(tmp_path)
    assert v["veredito"] == "PRONTO"
    assert v["efeito_da_correcao"] == [{"id": "corrigido", "mensagem": "unknown import symbol"}]


def test_v3b_nao_entra_no_veredito(tmp_path):
    grava(tmp_path, frio(), frio(), frio(), quente())
    (tmp_path / "30_4_108_v3b.json").write_text(
        json.dumps(frio(falsos=[falso("so-na-v3b")]))
    )
    assert consolidar_j18.consolidar(tmp_path)["veredito"] == "PRONTO"
