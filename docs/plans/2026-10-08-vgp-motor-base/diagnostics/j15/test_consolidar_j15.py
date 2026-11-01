"""Testes do aceite J15: p95 frio é informativo; todo o resto continua gate."""

import json

import consolidar_j15


def frio(*pendencias, inconclusivo=False, chamadas=1064):
    """Cria um resultado frio mínimo, com denominação e sinais explícitos."""
    return {
        "pronto": not pendencias,
        "pendencias": list(pendencias),
        "inconclusivo": inconclusivo,
        "inconclusivo_motivo": "sinal ausente" if inconclusivo else "",
        "chamadas": chamadas,
        "p95_ms": 2000.0,
        "falso_deny_por_balde": {},
        "recall_simbolo": {"arquivo": 1.0, "inline": 1.0},
    }


def quente(**mudancas):
    """Cria um run quente válido e aplica apenas a mutação pedida pelo teste."""
    base = {
        "valido": True,
        "p95_motor_ms": 250.0,
        "timeouts_camada2": 0,
        "denominador": {
            "entradas_esperadas": 1064,
            "medidas_com_sombra": 1064,
            "medidas_com_camada2": 1064,
            "rotas": {"daemon": 2128},
        },
    }
    base.update(mudancas)
    return base


def test_p95_frio_isolado_nao_reprova():
    """A única pendência removida é a latência fria prevista no pré-registro."""
    v = consolidar_j15.avaliar(frio("p95 2000.0 ms > 1000 ms"), frio("p95 2000.0 ms > 1000 ms"), quente())
    assert v["veredito"] == "PRONTO"
    assert v["pendencias"] == []


def test_falha_real_nao_some_com_o_filtro_de_p95():
    """Falso deny continua reprovando mesmo ao lado da pendência fria filtrada."""
    v = consolidar_j15.avaliar(
        frio("p95 2000.0 ms > 1000 ms"),
        frio("p95 2000.0 ms > 1000 ms", "falso deny (inline) 1.250% > 1%"),
        quente(),
    )
    assert v["veredito"] == "NÃO PRONTO"
    assert v["pendencias"] == ["V3: falso deny (inline) 1.250% > 1%"]


def test_timeout_quente_acima_do_patamar_e_inconclusivo():
    """Mais de 20% de timeouts impede o aceite, nunca conta como zero."""
    v = consolidar_j15.avaliar(frio(), frio(), quente(timeouts_camada2=213))
    assert v["veredito"] == "INCONCLUSIVO"


def test_sinal_quente_ausente_e_inconclusivo_mesmo_com_p95_baixo():
    """P95 baixo sem sombra suficiente não transforma ausência em sucesso."""
    hot = quente(valido=False)
    hot["denominador"]["medidas_com_sombra"] = 0
    v = consolidar_j15.avaliar(frio(), frio(), hot)
    assert v["veredito"] == "INCONCLUSIVO"


def test_rota_standalone_invalida_o_denominador_quente():
    """Toda chamada de aquecimento e medição precisa passar pelo daemon."""
    hot = quente()
    hot["denominador"]["rotas"] = {"daemon": 2127, "standalone": 1}
    v = consolidar_j15.avaliar(frio(), frio(), hot)
    assert v["veredito"] == "INCONCLUSIVO"


def test_frio_inconclusivo_nao_e_salvo_pelo_quente_verde():
    """A inconclusividade do juiz frio permanece no veredito consolidado."""
    v = consolidar_j15.avaliar(frio(), frio(inconclusivo=True), quente())
    assert v["veredito"] == "INCONCLUSIVO"


def test_p95_quente_acima_do_limite_reprova():
    """O único gate de latência do J15 é o p95 quente de 1.000 ms."""
    v = consolidar_j15.avaliar(frio(), frio(), quente(p95_motor_ms=1000.001))
    assert v["veredito"] == "NÃO PRONTO"
    assert "p95 quente" in v["pendencias"][0]


def test_consolidar_exige_os_tres_artefatos(tmp_path):
    """Artefato ausente propaga erro em vez de produzir aceite vazio."""
    try:
        consolidar_j15.consolidar(tmp_path)
    except FileNotFoundError as erro:
        assert "30_4_102_v2.json" in str(erro)
    else:
        raise AssertionError("ausência de artefato não pode virar aceite")


def test_consolidar_le_os_jsons_reais(tmp_path):
    """O leitor de disco combina v2, V3 e quente no mesmo veredito."""
    for nome in ("30_4_102_v2.json", "30_4_102_v3.json"):
        (tmp_path / nome).write_text(json.dumps(frio()))
    (tmp_path / "hot.json").write_text(json.dumps(quente()))
    resultado = consolidar_j15.consolidar(tmp_path)
    assert resultado["veredito"] == "PRONTO"
