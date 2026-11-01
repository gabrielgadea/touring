#!/usr/bin/env python3
"""test_model_routing_adherence.py — a régua de roteamento de modelos, sem tocar transcript real.

Todo fixture nasce em `tmp_path` com o formato MEDIDO nos transcripts de 10/2026 (um registro por
bloco de conteúdo, `message.id` repetido, `usage` parcial, `tool_use` com `id`/`name`/`input`).
O que cada grupo trava:

1. contagem — por `model` pedido, o nome legado `Task`, `subagent_type`, dia;
2. fronteira — janela `--since`, arquivo de subagente × principal, sessão irmã fora do total Anthropic;
3. robustez — usage ausente, linha malformada, registro sem timestamp, arquivo velho;
4. deduplicação — a mensagem e a chamada contam UMA vez (o transcript repete ambas);
5. contrato — `--json`, `--brief` (≤ 12 linhas, pt-BR), `--emit` (o `external:` do touring kpi).
"""

from __future__ import annotations

import datetime as dt
import json
import os
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import model_routing_adherence as mra

DESDE = dt.datetime(2026, 10, 8, tzinfo=dt.UTC)
CHAVES_DO_RELATORIO = {
    "janela",
    "chamadas_agent",
    "modelo_real_dos_subagentes",
    "tokens_por_familia",
    "sessoes",
    "desajustes",
    "aderencia_roteamento",
    "gate",
    "nudge",
    "linhas_puladas",
}


@pytest.fixture(autouse=True)
def _home_isolado(monkeypatch, tmp_path):
    """Os defaults de --root/--gate-log/--nudge-log vêm de `Path.home()`: sem isto os testes de
    `main()` leriam os registros REAIS do usuário e mudariam de resultado com o dia."""
    monkeypatch.setenv("HOME", str(tmp_path / "home-isolado"))


def _ts(dia: str = "2026-10-12", hora: str = "12:00:00") -> str:
    return f"{dia}T{hora}.000Z"


def _chamada(
    tid: str, modelo: str | None, tipo: str | None, nome: str = "Agent"
) -> dict:
    entrada: dict = {"description": "d", "prompt": "p"}
    if modelo is not None:
        entrada["model"] = modelo
    if tipo is not None:
        entrada["subagent_type"] = tipo
    return {"type": "tool_use", "id": tid, "name": nome, "input": entrada}


def _assistente(
    modelo: str,
    *,
    msg_id: str | None = None,
    usage: object = None,
    chamadas: tuple[dict, ...] = (),
    ts: str | bool | None = None,  # False = registro sem timestamp
    uuid: str | None = None,
    sessao: str | None = None,
) -> dict:
    msg: dict = {"role": "assistant", "model": modelo, "content": list(chamadas)}
    if msg_id is not None:
        msg["id"] = msg_id
    if usage is not None:
        msg["usage"] = usage
    rec: dict = {"type": "assistant", "message": msg}
    if ts is not False:
        rec["timestamp"] = ts or _ts()
    if uuid is not None:
        rec["uuid"] = uuid
    if sessao is not None:
        rec["sessionId"] = sessao
    return rec


def _usage(
    entrada: int = 0, criacao: int = 0, leitura: int = 0, saida: int = 0
) -> dict:
    return {
        "input_tokens": entrada,
        "cache_creation_input_tokens": criacao,
        "cache_read_input_tokens": leitura,
        "output_tokens": saida,
    }


def _gravar(caminho: Path, registros: list) -> Path:
    caminho.parent.mkdir(parents=True, exist_ok=True)
    linhas = [r if isinstance(r, str) else json.dumps(r) for r in registros]
    caminho.write_text("\n".join(linhas) + "\n", encoding="utf-8")
    return caminho


def _principal(raiz: Path, nome: str, registros: list) -> Path:
    return _gravar(raiz / "-proj" / f"{nome}.jsonl", registros)


def _subagente(raiz: Path, sessao: str, nome: str, registros: list) -> Path:
    return _gravar(raiz / "-proj" / sessao / "subagents" / f"{nome}.jsonl", registros)


def _medir(raiz: Path, desde: dt.datetime = DESDE) -> dict:
    return mra.resumo(mra.varrer(raiz, desde))


def _opus(msg_id: str, *chamadas: dict, **resto) -> dict:
    """Turno do assistente principal (opus) que carrega as `chamadas` tool_use dadas."""
    return _assistente("claude-opus-5-5", msg_id=msg_id, chamadas=chamadas, **resto)


def _emitir(raiz: Path, alvo: Path, *extra: str) -> int:
    """`mra.main` com `--emit`: o argv comum dos testes do contrato `external` do KPI."""
    argv = ["--root", str(raiz), "--since", "2026-10-08", "--emit", str(alvo)]
    return mra.main([*argv, *extra])


# ── 1. contagem ─────────────────────────────────────────────────────────────


def test_conta_chamadas_por_model_pedido(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [
            _opus(
                "m1",
                _chamada("t1", "sonnet", "general-purpose"),
                _chamada("t2", "haiku", "Explore"),
                _chamada("t3", None, None),
            )
        ],
    )
    chamadas = _medir(tmp_path)["chamadas_agent"]
    assert chamadas["total"] == 3
    assert chamadas["por_model_pedido"] == {"haiku": 1, "sem_model": 1, "sonnet": 1}
    assert chamadas["por_subagent_type"] == {
        "(nenhum)": 1,
        "Explore": 1,
        "general-purpose": 1,
    }
    assert chamadas["por_dia"] == {"2026-10-12": 3}


def test_nome_legado_task_conta_como_delegacao_e_outras_tools_nao(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [
            _opus(
                "m1",
                _chamada("t1", "sonnet", "Plan", nome="Task"),
                _chamada("t2", "haiku", "Explore", nome="Agent"),
                _chamada("t3", "haiku", "Explore", nome="Read"),
            )
        ],
    )
    chamadas = _medir(tmp_path)["chamadas_agent"]
    assert chamadas["total"] == 2
    assert chamadas["por_model_pedido"] == {"haiku": 1, "sonnet": 1}


def test_chamadas_por_dia_usam_utc(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [
            _opus("m1", _chamada("t1", "haiku", "Explore"), ts=_ts("2026-10-12", "23:59:59")),
            _opus("m2", _chamada("t2", "haiku", "Explore"), ts="2026-10-13T00:00:01+00:00"),
        ],
    )
    assert _medir(tmp_path)["chamadas_agent"]["por_dia"] == {
        "2026-10-12": 1,
        "2026-10-13": 1,
    }


@pytest.mark.parametrize(
    ("modelo", "familia"),
    [
        ("claude-opus-5-5", "opus"),
        ("claude-sonnet-5-5", "sonnet"),
        ("claude-haiku-4-5-20251001", "haiku"),
        ("claude-fable-5-1", "fable"),
        ("kimi-k3", "kimi"),
        ("K3-preview", "kimi"),
        ("MiniMax-M3.1-Flash-Preview", "minimax"),
        ("glm-5", "glm"),
        ("deepseek-v4", "deepseek"),
        ("<synthetic>", "outro"),
        ("gpt-9", "outro"),
        (None, "outro"),
        (7, "outro"),
    ],
)
def test_familia_do_modelo_por_substring(modelo, familia):
    assert mra.familia_do_modelo(modelo) == familia


# ── 2. fronteira ────────────────────────────────────────────────────────────


def test_sessao_irma_fica_fora_do_total_anthropic(tmp_path):
    _principal(
        tmp_path,
        "anthropic",
        [_assistente("claude-sonnet-5-5", msg_id="a1", usage=_usage(saida=100))],
    )
    _principal(
        tmp_path,
        "irma",
        [_assistente("kimi-k3", msg_id="k1", usage=_usage(saida=1000))],
    )
    r = _medir(tmp_path)
    tokens = r["tokens_por_familia"]
    assert tokens["anthropic"]["total"] == 100
    assert tokens["irmas"] == {"total": 1000, "por_familia": {"kimi": 1000}}
    assert r["sessoes"]["principais_anthropic"] == 1
    assert r["sessoes"]["irmas"] == 1


def test_arquivo_de_subagente_nao_e_sessao_e_traz_o_modelo_real(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [
            _assistente(
                "claude-opus-5-5",
                msg_id="p1",
                usage=_usage(saida=500),
                chamadas=(_chamada("t1", "haiku", "Explore"),),
            )
        ],
    )
    _subagente(
        tmp_path,
        "s1",
        "agent-a",
        [_assistente("claude-haiku-4-5-20251001", msg_id="h1", usage=_usage(saida=7))],
    )
    _subagente(
        tmp_path,
        "s1",
        "agent-b",
        [_assistente("claude-sonnet-5-5", msg_id="s1", usage=_usage(saida=9))],
    )
    r = _medir(tmp_path)
    assert r["modelo_real_dos_subagentes"] == {
        "arquivos_por_familia": {"haiku": 1, "sonnet": 1},
        "total": 2,
    }
    tokens = r["tokens_por_familia"]
    assert tokens["principal"] == {"opus": 500}
    assert tokens["subagente"] == {"sonnet": 9, "haiku": 7}
    assert tokens["anthropic"]["total"] == 516
    assert tokens["anthropic"]["subagente"] == 16
    assert tokens["anthropic"]["parcela_pct"]["haiku"] == round(100 * 7 / 516, 2)
    assert r["sessoes"]["principais_anthropic"] == 1
    assert r["janela"]["arquivos_principais"] == 1
    assert r["janela"]["arquivos_subagente"] == 2


def test_sessao_que_delegou_e_taxa(tmp_path):
    _principal(
        tmp_path,
        "com",
        [
            _assistente(
                "claude-opus-5-5",
                msg_id="c1",
                chamadas=(_chamada("t1", "haiku", "Explore"),),
            )
        ],
    )
    _principal(tmp_path, "sem", [_assistente("claude-opus-5-5", msg_id="s1")])
    sessoes = _medir(tmp_path)["sessoes"]
    assert (sessoes["principais_anthropic"], sessoes["que_delegaram"]) == (2, 1)
    assert sessoes["taxa_delegacao"] == 0.5


def test_filtro_since_descarta_registro_anterior_a_janela(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [
            _assistente(
                "claude-opus-5-5",
                msg_id="velho",
                ts=_ts("2026-10-01"),
                usage=_usage(saida=999),
                chamadas=(_chamada("t-velho", "sonnet", "Explore"),),
            ),
            _assistente(
                "claude-opus-5-5",
                msg_id="novo",
                ts=_ts("2026-10-12"),
                usage=_usage(saida=1),
                chamadas=(_chamada("t-novo", "haiku", "Explore"),),
            ),
        ],
    )
    r = _medir(tmp_path)
    assert r["chamadas_agent"]["total"] == 1
    assert r["chamadas_agent"]["por_model_pedido"] == {"haiku": 1}
    assert r["tokens_por_familia"]["anthropic"]["total"] == 1
    assert (
        _medir(tmp_path, dt.datetime(2026, 9, 1, tzinfo=dt.UTC))[
            "chamadas_agent"
        ]["total"]
        == 2
    )


def test_since_e_inclusivo_na_meia_noite_utc(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [_opus("m1", ts="2026-10-08T00:00:00.000Z", usage=_usage(saida=3))],
    )
    assert _medir(tmp_path)["tokens_por_familia"]["anthropic"]["total"] == 3


def test_desajuste_busca_com_modelo_caro(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [
            _opus(
                "m1",
                _chamada("t1", "sonnet", "Explore"),
                _chamada("t2", "haiku", "Explore"),
                _chamada("t3", "opus", "touring-scouter"),
                _chamada("t4", "sonnet", "general-purpose"),
                _chamada("t5", None, "Explore"),
                _chamada("t6", "fable", "Explore"),
                _chamada("t7", "fable", "general-purpose"),
            )
        ],
    )
    r = _medir(tmp_path)
    assert r["desajustes"]["total"] == 3
    assert r["desajustes"]["por_tipo_e_modelo"] == {
        "Explore/fable": 1,
        "Explore/sonnet": 1,
        "touring-scouter/opus": 1,
    }
    assert (
        r["desajustes"]["fable_pedido"] == 2
    )  # só conta: o consentimento não é decidível
    assert r["desajustes"]["busca_sem_model"] == 1
    assert r["aderencia_roteamento"] == {
        "valor": round(1 / 4, 4),
        "elegiveis": 4,
        "aderentes": 1,
        "desajustes": 3,
    }


def test_sem_busca_decidivel_a_aderencia_e_none_nunca_zero(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [_opus("m1", _chamada("t1", "sonnet", "general-purpose"))],
    )
    assert _medir(tmp_path)["aderencia_roteamento"]["valor"] is None


# ── 3. robustez ─────────────────────────────────────────────────────────────


def test_usage_ausente_ou_invalido_vale_zero(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [
            _assistente("claude-opus-5-5", msg_id="sem-usage"),
            _assistente("claude-opus-5-5", msg_id="usage-lixo", usage="cinco"),
            _assistente(
                "claude-opus-5-5",
                msg_id="usage-tipos",
                usage={
                    "input_tokens": "9",
                    "output_tokens": True,
                    "cache_read_input_tokens": -4,
                },
            ),
            _assistente("claude-opus-5-5", msg_id="ok", usage=_usage(1, 2, 3, 4)),
        ],
    )
    assert _medir(tmp_path)["tokens_por_familia"]["anthropic"]["total"] == 10
    assert mra.tokens_de_usage(None) == 0
    assert mra.tokens_de_usage(_usage(1, 2, 3, 4)) == 10


def test_linha_malformada_e_contada_e_pulada(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [
            "{isto nao e json",
            "[1, 2, 3]",
            "",
            _assistente(
                "claude-opus-5-5",
                msg_id="m1",
                usage=_usage(saida=5),
                chamadas=(_chamada("t1", "haiku", "Explore"),),
            ),
        ],
    )
    r = _medir(tmp_path)
    assert r["linhas_puladas"]["malformadas"] == 2
    assert r["chamadas_agent"]["total"] == 1
    assert r["tokens_por_familia"]["anthropic"]["total"] == 5


def test_registro_sem_timestamp_e_contado_a_parte(tmp_path):
    _principal(
        tmp_path,
        "s1",
        [_assistente("claude-opus-5-5", msg_id="m1", ts=False, usage=_usage(saida=5))],
    )
    r = _medir(tmp_path)
    assert r["linhas_puladas"]["sem_timestamp"] == 1
    assert r["tokens_por_familia"]["anthropic"]["total"] == 0


def test_registros_que_nao_sao_assistant_sao_ignorados(tmp_path):
    usuario = {
        "type": "user",
        "timestamp": _ts(),
        # Só o `role` separa este registro de um assistant: com usage e tool_use de propósito.
        "message": {
            "role": "user",
            "model": "claude-opus-5-5",
            "usage": _usage(saida=900),
            "content": [_chamada("t-usuario", "sonnet", "Explore")],
        },
    }
    sistema = {"type": "system", "timestamp": _ts(), "subtype": "x"}
    _principal(
        tmp_path,
        "s1",
        [
            usuario,
            sistema,
            _assistente("claude-opus-5-5", msg_id="m1", usage=_usage(saida=2)),
        ],
    )
    r = _medir(tmp_path)
    assert r["tokens_por_familia"]["anthropic"]["total"] == 2
    assert r["chamadas_agent"]["total"] == 0
    assert r["janela"]["mensagens_distintas"] == 1
    assert r["linhas_puladas"] == {
        "malformadas": 0,
        "sem_timestamp": 0,
        "blobs_de_requisicao": 0,
    }


def test_blob_de_requisicao_repete_turnos_antigos_e_nao_conta(tmp_path):
    # Formato medido: `type: api-request-blob`, `message.role: assistant`, sem timestamp/id/usage,
    # com os tool_use de turnos passados dentro — contá-lo contaria cada chamada de novo.
    blob = {
        "type": "api-request-blob",
        "hash": "abc",
        "sessionId": "s",
        "message": {
            "role": "assistant",
            "content": [_chamada("t-passada", "sonnet", "Explore")],
        },
    }
    blob_com_data = {**blob, "timestamp": _ts()}
    _principal(
        tmp_path,
        "s1",
        [
            blob,
            blob_com_data,
            _assistente(
                "claude-opus-5-5",
                msg_id="m1",
                chamadas=(_chamada("t-real", "haiku", "Explore"),),
            ),
        ],
    )
    r = _medir(tmp_path)
    assert r["chamadas_agent"]["total"] == 1
    assert r["chamadas_agent"]["por_model_pedido"] == {"haiku": 1}
    assert r["linhas_puladas"] == {
        "malformadas": 0,
        "sem_timestamp": 0,
        "blobs_de_requisicao": 2,
    }


def test_arquivo_nao_tocado_desde_a_janela_e_ignorado_por_mtime(tmp_path):
    velho = _principal(
        tmp_path,
        "velho",
        [_assistente("claude-opus-5-5", msg_id="v1", usage=_usage(saida=50))],
    )
    os.utime(velho, (dt.datetime(2026, 1, 1, tzinfo=dt.UTC).timestamp(),) * 2)
    _principal(
        tmp_path,
        "novo",
        [_assistente("claude-opus-5-5", msg_id="n1", usage=_usage(saida=7))],
    )
    r = _medir(tmp_path)
    assert r["janela"]["arquivos_ignorados_por_mtime"] == 1
    assert r["janela"]["arquivos_principais"] == 1
    assert r["tokens_por_familia"]["anthropic"]["total"] == 7


def test_dominante_prefere_familia_real_a_outro_e_desempata_por_nome():
    from collections import Counter

    assert mra.dominante(Counter()) is None
    assert mra.dominante(Counter({"outro": 9, "haiku": 1})) == "haiku"
    assert mra.dominante(Counter({"outro": 2})) == "outro"
    assert mra.dominante(Counter({"sonnet": 3, "haiku": 3})) == "haiku"


# ── 4. deduplicação ─────────────────────────────────────────────────────────


def test_mensagem_repetida_conta_uma_vez_com_o_maximo_do_usage(tmp_path):
    # O transcript grava um registro por bloco, todos com o mesmo message.id e usage parcial.
    _principal(
        tmp_path,
        "s1",
        [
            _assistente("claude-opus-5-5", msg_id="m1", usage=_usage(10, 0, 100, 5)),
            _assistente("claude-opus-5-5", msg_id="m1", usage=_usage(10, 0, 100, 20)),
            _assistente("claude-opus-5-5", msg_id="m1", usage=_usage(10, 0, 100, 12)),
        ],
    )
    r = _medir(tmp_path)
    assert r["tokens_por_familia"]["anthropic"]["total"] == 10 + 100 + 20
    assert r["janela"]["mensagens_distintas"] == 1


def test_historico_reproduzido_em_sessao_retomada_nao_conta_duas_vezes(tmp_path):
    registro = _assistente(
        "claude-opus-5-5",
        msg_id="m1",
        usage=_usage(saida=40),
        chamadas=(_chamada("t1", "haiku", "Explore"),),
    )
    _principal(tmp_path, "original", [registro])
    _principal(tmp_path, "retomada", [registro])
    r = _medir(tmp_path)
    assert r["tokens_por_familia"]["anthropic"]["total"] == 40
    assert r["chamadas_agent"]["total"] == 1
    assert (
        r["sessoes"]["principais_anthropic"] == 1
    )  # a retomada não trouxe mensagem nova


def test_chamada_com_o_mesmo_tool_use_id_conta_uma_vez(tmp_path):
    chamada = _chamada("t-repetido", "sonnet", "Explore")
    _principal(
        tmp_path,
        "s1",
        [
            _assistente("claude-opus-5-5", msg_id="m1", chamadas=(chamada,)),
            _assistente(
                "claude-opus-5-5",
                msg_id="m2",
                chamadas=(chamada, _chamada("t-outro", "haiku", "Explore")),
            ),
        ],
    )
    assert _medir(tmp_path)["chamadas_agent"]["total"] == 2


def test_mensagem_sem_id_cai_no_uuid_e_sem_nenhum_dos_dois_conta_cada_registro(
    tmp_path,
):
    _principal(
        tmp_path,
        "s1",
        [
            _assistente("claude-opus-5-5", uuid="u1", usage=_usage(saida=1)),
            _assistente("claude-opus-5-5", uuid="u1", usage=_usage(saida=3)),
            _assistente("claude-opus-5-5", usage=_usage(saida=10)),
            _assistente("claude-opus-5-5", usage=_usage(saida=10)),
        ],
    )
    assert _medir(tmp_path)["tokens_por_familia"]["anthropic"]["total"] == 3 + 10 + 10


# ── 5. contrato (CLI) ───────────────────────────────────────────────────────


def _cenario_completo(raiz: Path) -> None:
    _principal(
        raiz,
        "s1",
        [
            _assistente(
                "claude-opus-5-5",
                msg_id="p1",
                usage=_usage(0, 0, 1_500_000, 0),
                chamadas=(
                    _chamada("t1", "haiku", "Explore"),
                    _chamada("t2", "sonnet", "Explore"),
                ),
            )
        ],
    )
    _subagente(
        raiz,
        "s1",
        "agent-a",
        [
            _assistente(
                "claude-haiku-4-5-20251001", msg_id="h1", usage=_usage(saida=2000)
            )
        ],
    )


def test_json_traz_todas_as_secoes(tmp_path, capsys):
    _cenario_completo(tmp_path)
    assert mra.main(["--root", str(tmp_path), "--since", "2026-10-08", "--json"]) == 0
    saida = json.loads(capsys.readouterr().out)
    assert CHAVES_DO_RELATORIO <= set(saida)
    assert saida["chamadas_agent"]["total"] == 2


def test_brief_cabe_em_12_linhas_e_usa_numeros_ptbr(tmp_path, capsys):
    _cenario_completo(tmp_path)
    assert mra.main(["--root", str(tmp_path), "--since", "2026-10-08"]) == 0
    texto = capsys.readouterr().out.strip()
    assert 1 < len(texto.splitlines()) <= 12
    assert "1,5 M" in texto  # tokens em pt-BR: vírgula decimal
    assert "aderência 0,5" in texto
    assert "desde 2026-10-08" in texto


def test_brief_formata_tokens_das_irmas_em_ptbr(tmp_path, capsys):
    _principal(
        tmp_path,
        "irma",
        [_assistente("kimi-k3", msg_id="k1", usage=_usage(0, 0, 1_459_799_617, 0))],
    )
    mra.main(["--root", str(tmp_path), "--since", "2026-10-08"])
    texto = capsys.readouterr().out
    assert "kimi 1,46 G" in texto
    assert "1459799617" not in texto


def test_json_com_brief_prefere_o_resumo(tmp_path, capsys):
    _cenario_completo(tmp_path)
    mra.main(["--root", str(tmp_path), "--since", "2026-10-08", "--json", "--brief"])
    assert "roteamento de modelos" in capsys.readouterr().out


def test_brief_avisa_quando_nada_foi_medido(tmp_path, capsys):
    assert mra.main(["--root", str(tmp_path), "--since", "2026-10-08"]) == 0
    texto = capsys.readouterr().out
    assert "AVISO" in texto
    assert "indefinida" in texto


def test_root_inexistente_sai_com_2(tmp_path, capsys):
    assert mra.main(["--root", str(tmp_path / "nao-existe")]) == 2
    assert "inexistente" in capsys.readouterr().err


def test_since_invalido_sai_com_2(tmp_path, capsys):
    assert mra.main(["--root", str(tmp_path), "--since", "ontem"]) == 2
    assert "YYYY-MM-DD" in capsys.readouterr().err


def test_since_padrao_e_meia_noite_utc_sete_dias_atras():
    agora = dt.datetime(2026, 10, 17, 15, 30, tzinfo=dt.UTC)
    assert mra.desde_padrao(agora) == dt.datetime(2026, 10, 10, tzinfo=dt.UTC)
    assert mra.parse_since("2026-10-08") == DESDE


def test_emit_grava_o_contrato_external_do_kpi(tmp_path):
    raiz = tmp_path / "projects"
    _principal(
        raiz,
        "s1",
        [
            _opus(
                "m1",
                *(_chamada(f"h{i}", "haiku", "Explore") for i in range(8)),
                _chamada("c1", "sonnet", "Explore"),
                _chamada("c2", "opus", "touring-scouter"),
            )
        ],
    )
    alvo = tmp_path / "kpi" / "external" / "touring.routing.adherence.json"
    assert _emitir(raiz, alvo, "--min-n", "10") == 0
    payload = json.loads(alvo.read_text(encoding="utf-8"))
    assert payload["value"] == 0.8  # 8 aderentes em 10 buscas decidíveis
    assert payload["n"] == 10
    assert payload["id"] == mra.ID_DO_KPI
    assert "detalhe" not in payload
    assert not [
        p for p in alvo.parent.iterdir() if p.suffix == ".tmp"
    ]  # gravação atômica sem resíduo


def test_emit_abaixo_do_minimo_grava_null_com_a_causa_nunca_zero(tmp_path):
    raiz = tmp_path / "projects"
    _principal(
        raiz,
        "s1",
        [_opus("m1", _chamada("c1", "sonnet", "Explore"))],
    )
    alvo = tmp_path / "ext.json"
    _emitir(raiz, alvo)
    payload = json.loads(alvo.read_text(encoding="utf-8"))
    assert (
        payload["value"] is None
    )  # 1 busca decidível < 10: a ausência de amostra não é 0%
    assert "mínimo 10" in payload["detalhe"]


def test_emit_sem_nenhuma_busca_decidivel_grava_null(tmp_path):
    raiz = tmp_path / "projects"
    raiz.mkdir()
    alvo = tmp_path / "ext.json"
    _emitir(raiz, alvo, "--min-n", "0")
    payload = json.loads(alvo.read_text(encoding="utf-8"))
    assert payload["value"] is None
    assert payload["detalhe"]


def test_commitment_do_kpi_declara_este_produtor_e_o_arquivo_do_proprio_id():
    # O resolvedor lê `<dir do yaml>/external/<id>.json`: se o `--emit` declarado no `source`
    # apontar para outro arquivo, o kpi mostra STUB para sempre sem que nada acuse.
    yaml = pytest.importorskip("yaml")
    dados = yaml.safe_load(
        (HERE.parent / "docs/kpi/commitments.yaml").read_text("utf-8")
    )
    por_id = {c["id"]: c for c in dados["commitments"]}
    assert mra.ID_DO_KPI in por_id, (
        "commitment não declarado em docs/kpi/commitments.yaml"
    )
    c = por_id[mra.ID_DO_KPI]
    # Campos que o struct `Commitment` (kpi.rs) exige para desserializar.
    assert {"id", "name", "threshold", "direction", "source"} <= set(c)
    assert c["direction"] in {"gte", "lte", "eq"}
    assert isinstance(c["threshold"], int | float)
    assert c["advisory"] is True  # piso provisório: nunca derruba `touring kpi --check`
    assert c["source"].startswith("external:")
    assert "scripts/model_routing_adherence.py" in c["source"]
    assert f"--emit docs/kpi/external/{mra.ID_DO_KPI}.json" in c["source"]


# ── 6. registros de enforcement: o gate de modelo e o aviso de delegação ────

#: Depois de todo fixture: nenhum aviso de 12/10 fica «pendente» (prazo de 10 min já vencido).
AGORA = dt.datetime(2026, 10, 14, tzinfo=dt.UTC)
Z = dt.UTC


def _gate(ts: object, why: object, allowed: object = True, **extra: object) -> dict:
    # Formato medido em ~/.claude/touring/agent_model_gate.jsonl: `ts` LOCAL com offset.
    return {
        "ts": ts,
        "session_id": "s",
        "agent_id": None,
        "allowed": allowed,
        "why": why,
        "subagent_type": "general-purpose",
        "model": "sonnet",
        **extra,
    }


def _aviso(ts: object, session: object, evento: object = "leitura") -> dict:
    # Formato do `delegation_nudge.py`: `ts, session, evento, n, kb, alvos`.
    return {
        "ts": ts,
        "session": session,
        "evento": evento,
        "n": 4,
        "kb": 120.5,
        "alvos": ["a.rs"],
    }


def _com_registros(raiz: Path, **kw: object) -> dict:
    kw.setdefault("agora", AGORA)
    return mra.resumo(mra.varrer(raiz, DESDE), **kw)


def test_gate_conta_por_why_e_por_dia_incluindo_motivos_novos(tmp_path):
    log = _gravar(
        tmp_path / "gate.jsonl",
        [
            _gate("2026-10-12T10:00:00-03:00", "explicit:sonnet", True),
            _gate("2026-10-12T11:00:00-03:00", "fit:busca-exige-haiku", False),
            _gate("2026-10-12T11:05:00-03:00", "fit:busca-exige-haiku", False),
            # 22:30 em -03:00 já é o dia 13 em UTC: o dia é UTC, como nas demais seções.
            _gate("2026-10-12T22:30:00-03:00", "fable:sem-consentimento", False),
            _gate("2026-10-13T09:00:00-03:00", "fable:consentido", True),
            _gate(
                "2026-10-01T09:00:00-03:00", "explicit:sonnet", True
            ),  # antes da janela
        ],
    )
    g = _com_registros(tmp_path, gate_log=log)["gate"]
    assert g["presente"] is True
    assert g["total"] == 5
    assert g["allowed"] == {"true": 2, "false": 3, "indeterminado": 0}
    # o mais frequente vem primeiro
    assert next(iter(g["por_why"])) == "fit:busca-exige-haiku"
    assert g["por_why"]["fit:busca-exige-haiku"] == {
        "total": 2,
        "allowed": 0,
        "negadas": 2,
    }
    assert g["por_why"]["fable:consentido"] == {"total": 1, "allowed": 1, "negadas": 0}
    assert set(g["por_why"]) == {
        "explicit:sonnet",
        "fit:busca-exige-haiku",
        "fable:sem-consentimento",
        "fable:consentido",
    }
    assert g["por_dia"] == {
        "2026-10-12": {"total": 3, "allowed": 1, "negadas": 2},
        "2026-10-13": {"total": 2, "allowed": 1, "negadas": 1},
    }


def test_gate_ausente_ou_nao_pedido_e_presente_false_nunca_erro(tmp_path):
    assert _com_registros(tmp_path, gate_log=tmp_path / "nao-existe.jsonl")["gate"] == {
        "presente": False
    }
    assert _com_registros(tmp_path)["gate"] == {"presente": False}
    assert _com_registros(tmp_path, gate_log=tmp_path)["gate"] == {
        "presente": False
    }  # diretório


def test_gate_linhas_estranhas_sao_contadas_e_nao_derrubam(tmp_path):
    log = _gravar(
        tmp_path / "gate.jsonl",
        [
            "{isto nao e json",
            "[1, 2]",
            _gate(None, "x"),  # sem ts
            _gate("ontem", "x"),  # ts ilegível
            _gate(
                "2026-10-12T10:00:00-03:00", None, "sim"
            ),  # sem why; allowed não é booleano
            _gate("2026-10-12T10:01:00-03:00", "", False),  # why vazio
            _gate("2026-10-12T10:02:00-03:00", "   ", True),  # why só de espaços
            _gate("2026-10-12T10:03:00-03:00", 7, True),  # why que não é texto
        ],
    )
    g = _com_registros(tmp_path, gate_log=log)["gate"]
    assert g["linhas_puladas"] == {"malformadas": 2, "sem_ts": 2}
    assert g["total"] == 4
    assert g["allowed"] == {"true": 2, "false": 1, "indeterminado": 1}
    assert g["por_why"] == {"(sem why)": {"total": 4, "allowed": 2, "negadas": 1}}


def test_gate_conta_as_chamadas_decididas_pelo_env_nos_dois_formatos(tmp_path):
    """O env `CLAUDE_CODE_SUBAGENT_MODEL` libera a chamada sem `model` e apaga as regras de adequação:
    o relatório diz quantas passaram só por ele (formato de 17/10/2026 e o legado `env:<v>`)."""
    log = _gravar(
        tmp_path / "gate.jsonl",
        [
            _gate("2026-10-12T10:00:00-03:00", "env:CLAUDE_CODE_SUBAGENT_MODEL=haiku", True),
            _gate("2026-10-12T10:01:00-03:00", "env:CLAUDE_CODE_SUBAGENT_MODEL=haiku", True),
            _gate("2026-10-12T10:02:00-03:00", "env:CLAUDE_CODE_SUBAGENT_MODEL=opus", True),
            _gate("2026-10-12T10:03:00-03:00", "env:haiku", True),  # registro anterior à mudança
            _gate("2026-10-12T10:04:00-03:00", "explicit:sonnet", True),
            _gate("2026-10-12T10:05:00-03:00", "frontmatter:haiku", True),
            _gate("2026-10-12T10:06:00-03:00", "deny", False),
        ],
    )
    g = _com_registros(tmp_path, gate_log=log)["gate"]
    assert g["decididas_por_env"] == {"total": 4, "por_valor": {"haiku": 3, "opus": 1}}
    assert g["total"] == 7, "contar o env não pode tirar a chamada do total nem do por_why"
    assert g["por_why"]["env:CLAUDE_CODE_SUBAGENT_MODEL=haiku"]["total"] == 2


def test_gate_sem_chamada_decidida_pelo_env_reporta_zero_e_nao_ausencia(tmp_path):
    log = _gravar(tmp_path / "gate.jsonl", [_gate("2026-10-12T10:00:00-03:00", "explicit:sonnet", True)])
    g = _com_registros(tmp_path, gate_log=log)["gate"]
    assert g["decididas_por_env"] == {"total": 0, "por_valor": {}}
    assert mra.resumo_gate(tmp_path / "nao-existe.jsonl", DESDE) == {"presente": False}


def test_brief_mostra_quantas_chamadas_o_env_decidiu(tmp_path, capsys):
    raiz = tmp_path / "projects"
    raiz.mkdir()
    gate = _gravar(
        tmp_path / "g.jsonl",
        [_gate("2026-10-12T10:00:00-03:00", "env:CLAUDE_CODE_SUBAGENT_MODEL=haiku", True)] * 3,
    )
    assert mra.main(["--root", str(raiz), "--since", "2026-10-08", "--gate-log", str(gate)]) == 0
    assert "decididas por env: 3" in capsys.readouterr().out


def test_o_why_que_o_hook_grava_e_o_que_o_relatorio_conta(tmp_path, monkeypatch):
    """Predicado escrito em dois lugares: o `why` que o hook emite tem de ser o que o relatório reconhece."""
    import importlib.util

    hooks = Path(__file__).resolve().parents[1] / "client" / "hooks"
    monkeypatch.syspath_prepend(str(hooks))
    spec = importlib.util.spec_from_file_location("agent_model_gate_sob_teste", hooks / "agent_model_gate.py")
    assert spec is not None and spec.loader is not None
    gate = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gate)
    veredito = gate.decide(
        {"subagent_type": "general-purpose"}, {"CLAUDE_CODE_SUBAGENT_MODEL": "haiku"}, [], tmp_path
    )
    assert veredito.allowed is True
    assert veredito.why.startswith(mra.PREFIXO_WHY_ENV)
    log = _gravar(tmp_path / "gate.jsonl", [_gate("2026-10-12T10:00:00-03:00", veredito.why, True)])
    g = _com_registros(tmp_path, gate_log=log)["gate"]
    assert g["decididas_por_env"] == {"total": 1, "por_valor": {"haiku": 1}}


def test_nudge_conta_por_evento_e_por_dia(tmp_path):
    log = _gravar(
        tmp_path / "nudge.jsonl",
        [
            _aviso("2026-10-12T09:00:00-03:00", "a"),
            _aviso("2026-10-12T09:30:00-03:00", "a"),
            _aviso("2026-10-12T09:40:00-03:00", "a", "troca-de-modelo"),
            _aviso("2026-10-13T09:00:00-03:00", "b"),
            _aviso("2026-10-13T09:01:00-03:00", None, "erro"),
            _aviso("2026-10-13T09:02:00-03:00", "b", None),
            _aviso("2026-10-01T09:00:00-03:00", "b"),  # antes da janela
            "{lixo",
            _aviso(None, "b"),
        ],
    )
    n = _com_registros(tmp_path, nudge_log=log)["nudge"]
    assert n["presente"] is True
    assert n["total"] == 6
    assert n["por_evento"] == {
        "leitura": 3,
        "(sem evento)": 1,
        "erro": 1,
        "troca-de-modelo": 1,
    }
    assert n["por_dia"] == {
        "2026-10-12": {"leitura": 2, "troca-de-modelo": 1},
        "2026-10-13": {"(sem evento)": 1, "erro": 1, "leitura": 1},
    }
    assert n["linhas_puladas"] == {"malformadas": 1, "sem_ts": 1}


def test_nudge_ausente_e_presente_false_nunca_erro(tmp_path):
    ausente = _com_registros(tmp_path, nudge_log=tmp_path / "nao-existe.jsonl")
    assert ausente["nudge"] == {"presente": False}
    assert _com_registros(tmp_path)["nudge"] == {"presente": False}


def _haiku_em(raiz: Path, sessao: str, hora: str, tid: str, **kw: object) -> Path:
    return _principal(
        raiz,
        f"{sessao}-{tid}",
        [
            _assistente(
                "claude-opus-5-5",
                msg_id=f"m-{tid}",
                sessao=sessao,
                ts=_ts("2026-10-12", hora),
                chamadas=(
                    _chamada(
                        tid,
                        kw.get("modelo", "haiku"),
                        "Explore",
                        kw.get("nome", "Agent"),
                    ),
                ),
            )
        ],
    )


def test_conversao_exige_haiku_na_mesma_sessao_ate_10_minutos_depois(tmp_path):
    raiz = tmp_path / "projects"
    aviso = "2026-10-12T09:00:00-03:00"  # = 12:00:00Z
    _haiku_em(raiz, "sess-A", "12:04:00", "tA")  # converte
    _haiku_em(raiz, "sess-B", "12:11:00", "tB")  # tarde demais (11 min)
    _haiku_em(raiz, "sess-C", "12:02:00", "tC", modelo="sonnet")  # modelo caro
    _haiku_em(
        raiz, "sess-X", "12:02:00", "tX"
    )  # haiku, mas de OUTRA sessão que não a avisada D
    _haiku_em(raiz, "sess-E", "11:59:00", "tE")  # ANTES do aviso
    _haiku_em(raiz, "sess-F", "12:10:00", "tF")  # exatamente no prazo: converte
    _haiku_em(raiz, "sess-H", "12:01:00", "tH", nome="Task")  # nome legado também conta
    _subagente(  # chamada feita DENTRO de um subagente não é a sessão avisada
        raiz,
        "sess-G",
        "agent-1",
        [
            _assistente(
                "claude-haiku-4-5-20251001",
                msg_id="m-tG",
                sessao="sess-G",
                ts=_ts("2026-10-12", "12:03:00"),
                chamadas=(_chamada("tG", "haiku", "Explore"),),
            )
        ],
    )
    log = _gravar(
        tmp_path / "nudge.jsonl",
        [
            *(_aviso(aviso, f"sess-{letra}") for letra in "ABCDEFGH"),
            _aviso(aviso, "sess-A", "troca-de-modelo"),  # outro evento: fora da taxa
            _aviso(aviso, None),  # sem sessão: não dá para cruzar
        ],
    )
    c = _com_registros(raiz, nudge_log=log)["nudge"]["conversao"]
    assert c["avisos_de_leitura"] == 8
    assert c["convertidos"] == 3  # A, F, H
    assert c["taxa"] == 0.375
    assert c["sem_sessao"] == 1
    assert c["pendentes"] == 0
    assert c["janela_s"] == 600


def test_aviso_com_prazo_aberto_e_pendente_e_fora_do_denominador(tmp_path):
    raiz = tmp_path / "projects"
    raiz.mkdir()
    log = _gravar(tmp_path / "nudge.jsonl", [_aviso("2026-10-12T09:00:00-03:00", "s")])
    fechado = dt.datetime(2026, 10, 12, 12, 10, 0, tzinfo=Z)  # 12:00Z + 600 s
    aberto = dt.datetime(2026, 10, 12, 12, 9, 59, tzinfo=Z)
    c_aberto = _com_registros(raiz, nudge_log=log, agora=aberto)["nudge"]["conversao"]
    assert (c_aberto["avisos_de_leitura"], c_aberto["pendentes"], c_aberto["taxa"]) == (
        0,
        1,
        None,
    )
    c_fechado = _com_registros(raiz, nudge_log=log, agora=fechado)["nudge"]["conversao"]
    # Sem chamada nenhuma e o prazo vencido: aí sim é «não converteu» (0,0, nunca None).
    assert (
        c_fechado["avisos_de_leitura"],
        c_fechado["convertidos"],
        c_fechado["taxa"],
    ) == (1, 0, 0.0)


def test_conversao_casa_a_sessao_pelo_nome_do_arquivo_quando_o_registro_nao_a_traz(
    tmp_path,
):
    raiz = tmp_path / "projects"
    _principal(
        raiz,
        "sess-Z",
        [
            _assistente(
                "claude-opus-5-5",
                msg_id="mz",
                ts=_ts("2026-10-12", "12:02:00"),
                chamadas=(_chamada("tz", "haiku", "Explore"),),
            )
        ],
    )
    log = _gravar(
        tmp_path / "nudge.jsonl", [_aviso("2026-10-12T09:00:00-03:00", "sess-Z")]
    )
    c = _com_registros(raiz, nudge_log=log)["nudge"]["conversao"]
    assert (c["avisos_de_leitura"], c["convertidos"]) == (1, 1)


def test_sessao_normalizada_e_identidade_para_uuid_e_none_para_vazio():
    uuid = "10fb71c6-ef49-4c56-b3c0-493d1f4b1837"
    assert mra.sessao_normalizada(uuid) == uuid
    assert mra.sessao_normalizada("a b/c") == "a_b_c"
    assert len(mra.sessao_normalizada("x" * 200)) == 80
    assert [mra.sessao_normalizada(v) for v in (None, "", 5)] == [None, None, None]


def test_cli_le_os_dois_registros_e_o_brief_continua_em_12_linhas(tmp_path, capsys):
    raiz = tmp_path / "projects"
    _haiku_em(raiz, "sess-A", "12:04:00", "tA")
    gate = _gravar(
        tmp_path / "g.jsonl",
        [_gate("2026-10-12T10:00:00-03:00", "fit:busca-exige-haiku", False)],
    )
    nudge = _gravar(
        tmp_path / "n.jsonl", [_aviso("2026-10-12T09:00:00-03:00", "sess-A")]
    )
    base = [
        "--root",
        str(raiz),
        "--since",
        "2026-10-08",
        "--gate-log",
        str(gate),
        "--nudge-log",
        str(nudge),
    ]
    assert mra.main([*base, "--json"]) == 0
    saida = json.loads(capsys.readouterr().out)
    assert saida["gate"]["total"] == 1
    assert saida["nudge"]["conversao"]["taxa"] == 1.0
    assert mra.main(base) == 0
    texto = capsys.readouterr().out.strip()
    assert len(texto.splitlines()) <= 12
    assert (
        "gate de modelo: 1 chamadas (0 liberadas, 1 negadas) · fit:busca-exige-haiku 1"
        in texto
    )
    assert "conversão em haiku ≤10 min: 1/1 (100,0%)" in texto


def test_brief_diz_registro_ausente_nas_duas_linhas(tmp_path, capsys):
    raiz = tmp_path / "projects"
    raiz.mkdir()
    assert mra.main(["--root", str(raiz), "--since", "2026-10-08"]) == 0
    texto = capsys.readouterr().out
    assert "gate de modelo: registro ausente" in texto
    assert "avisos de delegação: registro ausente" in texto
    assert len(texto.strip().splitlines()) <= 12


def test_defaults_dos_registros_vivem_em_home_claude_touring(tmp_path, capsys):
    # É o contrato com os hooks: `~/.claude/touring/agent_model_gate.jsonl` e `delegation_nudge.jsonl`.
    pasta = tmp_path / "home-isolado" / ".claude" / "touring"
    _gravar(
        pasta / "agent_model_gate.jsonl",
        [_gate("2026-10-12T10:00:00-03:00", "explicit:haiku")],
    )
    _gravar(
        pasta / "delegation_nudge.jsonl", [_aviso("2026-10-12T09:00:00-03:00", "s")]
    )
    raiz = tmp_path / "projects"
    raiz.mkdir()
    mra.main(["--root", str(raiz), "--since", "2026-10-08", "--json"])
    saida = json.loads(capsys.readouterr().out)
    assert saida["gate"]["total"] == 1
    assert saida["nudge"]["total"] == 1
