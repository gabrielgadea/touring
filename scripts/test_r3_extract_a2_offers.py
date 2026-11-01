#!/usr/bin/env python3
"""Testes do extrator S-2 — sem tocar no corpus real.

O corpus (`~/.claude/touring/r3-data/a2_offers.jsonl`) é medido, não reescrito:
estes testes usam transcripts SINTÉTICOS com a forma que o Claude Code escreve,
para que uma mudança de formato seja detectada aqui e não no meio do split.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import r3_extract_a2_offers as ex  # noqa: E402

OFFER = (
    "[CODE MODE · rajada] 2ª inspeção `cat` em 300s — a 1ª já executou intacta; "
    "esta rajada É o programa, rode-o de uma vez:\n  "
    "touring run --lang bash --code 'cat /a/x.rs /b/y.rs'\n"
    "Um `touring run` zera a janela."
)


def _linha_denegar(texto: str, ts: str) -> str:
    """Um `tool_result` de deny na MESMA linha de um turno de usuário."""
    return json.dumps({"timestamp": ts, "type": "user",
                       "message": {"content": [
                           {"type": "text", "text": "olá"},
                           {"type": "tool_result", "tool_use_id": "t1",
                            "content": texto, "is_error": True}]}})


def _escrever(tmp_path: Path, eventos: list[tuple[str, str, str]], is_error: bool = True) -> Path:
    """Escreve um transcript sintético no formato que o Claude Code usa.

    `is_error=True` é o DENY genuíno; `False` é um tool_result que apenas CITA o
    texto do deny (grep em transcript, saída de relatório) — a distinção que o
    S-2 corrigiu medindo 20 citações, 4 delas com a rota completa no corpo.
    """
    linhas = []
    for ts, tipo, texto in eventos:
        if tipo == "bash":
            bloco = {"type": "tool_use", "id": "t1", "name": "Bash",
                     "input": {"command": texto}}
        else:
            bloco = {"type": "tool_result", "tool_use_id": "t1", "content": texto,
                     "is_error": is_error}
        linhas.append(json.dumps({
            "timestamp": ts, "type": "assistant",
            "message": {"content": [bloco]},
        }))
    p = tmp_path / "aaaa1111.jsonl"
    p.write_text("\n".join(linhas) + "\n", encoding="utf-8")
    return p


def test_window_is_cut_at_the_next_offer(tmp_path: Path) -> None:
    """E1.4: uma oferta nova SUBSTITUI a anterior (`cli_suggester.rs:4536`)."""
    p = _escrever(tmp_path, [
        ("2026-09-01T10:00:00Z", "offer", OFFER),
        ("2026-09-01T10:00:01Z", "bash", "ls -la"),
        ("2026-09-01T10:00:02Z", "offer", OFFER),      # corta a janela da 1ª
        ("2026-09-01T10:00:03Z", "bash", "touring run --lang bash --code 'cat /a/x.rs /b/y.rs'"),
    ])
    ofertas = ex.extract_session(p)
    assert len(ofertas) == 2
    assert ofertas[0]["cortada_em_nova_oferta"] is True
    # a 1ª só viu `ls -la` (None) antes de ser cortada
    assert ofertas[0]["n_cmds"] == 1 and ofertas[0]["b0_decisao"] == "not_followed"
    # a 2ª viu a rota fundida → followed
    assert ofertas[1]["b0_decisao"] == "followed" and ofertas[1]["b0_valor"] == 1.0


def test_offer_without_followup_is_not_followed(tmp_path: Path) -> None:
    """Sem nenhum Bash depois, a janela é vazia e B0 é `not_followed` (E1.2)."""
    p = _escrever(tmp_path, [("2026-09-01T10:00:00Z", "offer", OFFER)])
    ofertas = ex.extract_session(p)
    assert ofertas[0]["n_cmds"] == 0
    assert ofertas[0]["b0_decisao"] == "not_followed" and ofertas[0]["b0_valor"] is None


def test_window_stops_at_k_commands(tmp_path: Path) -> None:
    """K=3 (E1.4): o 4º Bash não entra na janela."""
    cmds = ["ls", "git status", "wc -l f", "TOURING_GATE_OK=1 wc -l g"]
    p = _escrever(tmp_path,
                  [("2026-09-01T10:00:00Z", "offer", OFFER)]
                  + [(f"2026-09-01T10:00:{i:02d}Z", "bash", c) for i, c in enumerate(cmds, 1)])
    ofertas = ex.extract_session(p)
    assert ofertas[0]["n_cmds"] == 3, f"a janela é K=3, veio {ofertas[0]['n_cmds']}"
    # o 4º (bypass → 0,0) está fora, então B0 fica not_followed sem valor
    assert ofertas[0]["b0_decisao"] == "not_followed" and ofertas[0]["b0_valor"] is None


def test_comando_acima_de_200_chars_e_preservado_inteiro(tmp_path: Path) -> None:
    """NÃO há truncagem: o B0 é calculado sobre a janela completa, então guardar o
    texto cortado faria o avaliador cego discordar do baseline por apresentação
    (medido em 9 de 198 decisões, todas followed→not_followed)."""
    longo = "echo " + ("x" * 500)
    p = _escrever(tmp_path, [
        ("2026-09-01T10:00:00Z", "offer", OFFER),
        ("2026-09-01T10:00:01Z", "bash", longo),
    ])
    cmd = ex.extract_session(p)[0]["cmds"][0]
    assert cmd == longo, f"o comando tem de chegar inteiro ao dataset (tem {len(cmd)} chars)"


def test_b0_guardado_e_reproduzivel_sobre_o_texto_guardado(tmp_path: Path) -> None:
    """O invariante que a truncagem quebrava: `b0_decision(cmds guardados)` tem de
    dar o MESMO veredito que `b0_decisao` guardado.

    É este teste que reprova se alguém reintroduzir a truncagem — e o mutante
    correspondente morre aqui."""
    from r3_b0 import b0_decision
    longa = "echo " + ("y" * 400) + " touring run --lang bash --code 'cat /a/x.rs /b/y.rs'"
    p = _escrever(tmp_path, [
        ("2026-09-01T10:00:00Z", "offer", OFFER),
        ("2026-09-01T10:00:01Z", "bash", "ls"),
        ("2026-09-01T10:00:02Z", "bash", longa),
    ])
    o = ex.extract_session(p)[0]
    assert len(o["cmds"][1]) > 200, "o 2º comando precisa passar de 200 chars para o teste ter dente"
    d, v = b0_decision(o["cmds"], k=o["janela_k"])
    assert (d, v) == (o["b0_decisao"], o["b0_valor"]), (
        f"o B0 guardado ({o['b0_decisao']}/{o['b0_valor']}) não é reproduzível sobre o "
        f"texto guardado ({d}/{v}) — truncagem ou drift"
    )


def test_citacao_do_deny_nao_vira_oferta(tmp_path: Path) -> None:
    """Um tool_result que apenas CITA o texto do deny não é oferta.

    `is_error=false` é o sinal: o Claude Code marca como erro o hook que NEGOU, e
    um `grep`/relatório que reproduz o texto vem com `is_error=false`. Medido:
    200 candidatos, 20 citações, **4 delas com a rota completa no corpo** — logo
    "marcador + rota casada" NÃO discrimina, só o `is_error` discrimina.
    """
    p = _escrever(tmp_path, [
        ("2026-09-01T10:00:00Z", "offer", OFFER),   # cita a rota inteira
        ("2026-09-01T10:00:01Z", "bash", "TOURING_GATE_OK=1 ls"),
    ], is_error=False)
    assert ex.extract_session(p) == [], (
        "citação com a rota completa no corpo não pode virar oferta — o filtro de "
        "`is_error` é o único que discrimina"
    )


def test_only_the_g1_rajada_marker_is_an_offer(tmp_path: Path) -> None:
    """`record_route_offer` é chamado de UM lugar: o deny do G1 (`cli_suggester.rs:5670`).

    Um deny de outra classe não é oferta — contá-lo inflaria o n com eventos que
    nunca consultaram `route_offers()`.
    """
    outro = "[CODE MODE · G7 re-inspeção] 2ª leitura na janela — rode o programa uma vez:"
    p = _escrever(tmp_path, [("2026-09-01T10:00:00Z", "offer", outro)])
    assert ex.extract_session(p) == [], "G7 não é record_route_offer"


# ---- RECALL: o detector tem de casar o texto COMO O CLAUDE CODE O ENTREGA ----
#
# O deny nunca chega ao transcript sozinho: o Claude Code embrulha o hook em
# `PreToolUse:<Tool> hook error: ` antes do payload. Medido em 02/10 nos denies de
# 25/09 e de 02/10 — os dois com o marcador na posição 28. A 1ª versão do detector
# ancorava em `^\[CODE MODE` e perdia 82 ofertas (114 em vez de 196) por isso.

DENY_25SEP = (
    "PreToolUse:Bash hook error: [CODE MODE · rajada] 2ª inspeção `cat` em 300s — "
    "a 1ª já executou intacta; esta rajada É o programa, rode-o de uma vez:\n  "
    "touring run --lang bash --code 'cat /a/x.rs /b/y.rs'\nUm `touring run` zera."
)
DENY_02OUT = (
    "PreToolUse:Bash hook error: [CODE MODE · rajada] 2ª inspeção `grep` em 300s — "
    "a 1ª já executou intacta; esta rajada É o programa, rode-o de uma vez:\n  "
    "touring run --lang bash --code 'grep -rn x .'\nUm `touring run` zera."
)
DENY_SEM_WRAPPER = OFFER  # o texto cru do cli_suggester, sem o embrulho do CC


@pytest.mark.parametrize("deny,rotulo", [
    (DENY_25SEP, "formato de 25/09"),
    (DENY_02OUT, "formato de 02/10"),
    (DENY_SEM_WRAPPER, "texto cru, sem wrapper"),
])
def test_recall_de_deny_com_wrapper_e_sem(tmp_path: Path, deny: str, rotulo: str) -> None:
    """REGRESSÃO DE RECALL: os dois formatos medidos precisam ser achados.

    Este é o teste que teria pego as 82 ofertas perdidas — o `^` do regex só
    casava quando o Claude Code não embrulhou o hook, o que nunca é o caso real.
    """
    assert ex.OFFER_MARKER.search(deny) is not None, f"{rotulo}: o detector NÃO casou"
    p = _escrever(tmp_path, [
        ("2026-09-01T10:00:00Z", "offer", deny),
        ("2026-09-01T10:00:01Z", "bash", "TOURING_GATE_OK=1 ls -la"),
    ])
    ofertas = ex.extract_session(p)
    assert len(ofertas) == 1, f"{rotulo}: esperado 1 oferta, veio {len(ofertas)}"
    assert ofertas[0]["rota_proposta"] is not None, f"{rotulo}: a rota não foi extraída"


def test_marker_deve_casar_em_qualquer_posicao() -> None:
    """O predicado é "o marcador aparece no texto", não "o texto começa por ele"."""
    assert ex.OFFER_MARKER.match("[CODE MODE · rajada] x") is not None
    assert ex.OFFER_MARKER.search("prefixo qualquer [CODE MODE · rajada] x") is not None, (
        "o wrapper `PreToolUse:<Tool> hook error: ` antecede o marcador no caso real"
    )
    assert ex.OFFER_MARKER.search("nada aqui") is None


def test_malformed_line_is_counted_not_fatal(tmp_path: Path) -> None:
    """Um journal truncado não pode virar 'nenhuma oferta' em silêncio."""
    p = _escrever(tmp_path, [("2026-09-01T10:00:00Z", "offer", OFFER)])
    with open(p, "a", encoding="utf-8") as f:
        f.write('{"timestamp": "2026-09-01T10:00:0\n')  # linha truncada
    ofertas = ex.extract_session(p)
    assert len(ofertas) == 1
    assert ofertas[0]["linhas_malformadas_no_transcript"] == 1


def test_no_session_crosses_its_own_file(tmp_path: Path) -> None:
    """A chave do runtime é a sessão (`cli_suggester.rs:4535`): nada de um arquivo
    vira oferta de outro."""
    a = _escrever(tmp_path / "a" if (tmp_path / "a").mkdir() or True else tmp_path,
                  [("2026-09-01T10:00:00Z", "offer", OFFER),
                   ("2026-09-01T10:00:01Z", "bash", "TOURING_GATE_OK=1 ls")])
    b = tmp_path / "bbbb2222.jsonl"
    b.write_text(json.dumps({
        "timestamp": "2026-09-01T11:00:00Z", "type": "assistant",
        "message": {"content": [{"type": "tool_use", "id": "t", "name": "Bash",
                                 "input": {"command": "ls"}}]},
    }) + "\n", encoding="utf-8")
    oa = ex.extract_session(a)
    ob = ex.extract_session(b)
    assert oa[0]["session"] != "bbbb2222"
    assert ob == [], "um transcript sem oferta não produz oferta"


# ---- A ROTA: a âncora é a aspa de FECHO seguida de quebra ----
#
# Medido nos 180 denies reais: 174 dos programas são MULTILINHA e a aspa interna
# chega crua. Três gramáticas foram medidas e duas falharam: o não-guloso devolve
# prefixo em 91 de 180 (51%), o guloso come o rodapé. A que acerta é
# `--code '(.*)'\n` — guloso até a ÚLTIMA aspa seguida de quebra.

DENY_COM_RODAPE = (
    "PreToolUse:Bash hook error: [CODE MODE · rajada] 2ª inspeção `cat` em 300s — "
    "a 1ª já executou intacta; esta rajada É o programa, rode-o de uma vez:\n  "
    "touring run --lang python --code 'root=os.path.expanduser(\"~/x\")'\n"
    "Um `touring run` zera a janela."
)


def test_programa_com_aspas_internas_nao_e_cortado() -> None:
    """Aspas DUPLAS internas: o não-guloso antigo devolvia só `os.path.expanduser`."""
    prog = ex.programa_da_rota(DENY_COM_RODAPE)
    assert prog == 'root=os.path.expanduser("~/x")', f"programa cortado: {prog!r}"
    assert ex.lang_da_rota(DENY_COM_RODAPE) == "python"


def test_programa_multilinha_com_aspa_simples() -> None:
    """O caso real dos 174: programa em VÁRIAS linhas terminando em `awk '{...}'`.

    A última linha do programa é a que fecha com aspa + quebra; o rodapé do gate
    vem na linha seguinte e não tem essa forma.
    """
    deny = (
        "PreToolUse:Bash hook error: [CODE MODE · rajada] 2ª inspeção `cat`:\n  "
        "touring run --lang bash --code 'cd /x\nls -la | awk \'{print $6}\''\n"
        "Um `touring run` zera a janela."
    )
    prog = ex.programa_da_rota(deny)
    assert prog == "cd /x\nls -la | awk '{print $6}'", f"multilinha cortado: {prog!r}"


def test_rota_nao_come_o_rodape_do_gate() -> None:
    """A âncora é a quebra: o que vem depois (o rodapé) não é programa."""
    prog = ex.programa_da_rota(DENY_COM_RODAPE)
    assert "zera a janela" not in prog, f"o rodapé entrou no programa: {prog!r}"
    assert "CODE MODE" not in prog


def test_rota_vazia_nao_e_inventada() -> None:
    """Um texto sem `--code` não produz programa — e None é desconhecido, não
    string vazia (a distinção que o B0 também respeita)."""
    assert ex.programa_da_rota("[CODE MODE · rajada] sem rota aqui") is None
    assert ex.lang_da_rota("[CODE MODE · rajada] sem rota aqui") is None


def test_ancora_e_o_que_impede_o_prefixo() -> None:
    """A âncora existe para matar o NÃO-guloso — que é o defeito dos 91.

    Uma afirmação honesta sobre o escopo: o guloso (sem âncora) DIVERGE do
    ancorado só quando o rodapé contém uma aspa simples depois da do programa.
    Num deny real o rodapé é "Um `touring run` zera a janela…", que não tem aspa
    simples — e aí o guloso por acaso acerta. Por isso este teste mira o
    não-guloso, que é o defeito real, e afirma o caso em que o guloso erra como
    uma propriedade do teste (para ele ter dentes), não como uma diferença que
    ocorra no corpus.
    """
    import re
    com_aspa_interna = ("[CODE MODE · rajada] rode-o:\n  "
                        "touring run --lang bash --code 'echo 'a b'; ls'\n"
                        "Um `touring run` zera a janela.")
    # a produção acerta…
    assert ex.programa_da_rota(com_aspa_interna) == "echo 'a b'; ls", (
        "o codigo de producao devolveu prefixo — a ancora nao esta em uso"
    )
    # O não-guloso SEM ÂNCORA — a 1ª versão, o defeito dos 91 — erra neste
    # mesmo texto: fecha na aspa interna e devolve o prefixo.
    sem_ancora = r"--code '(.*?)'"
    assert re.search(sem_ancora, com_aspa_interna, re.S).group(1) == "echo ", (
        "o caso nao distingue: o nao-guloso sem ancora acerta aqui"
    )
    # E o guloso, que é a razão de a âncora não ser simplesmente removível:
    # num texto cujo rodapé tem aspa depois da do programa, ele come o rodapé.
    com_aspa_no_rodape = ("[CODE MODE · rajada] rode-o:\n  "
                          "touring run --lang bash --code 'ls /a'\n"
                          "rodape 'citado' e nada")
    assert ex.programa_da_rota(com_aspa_no_rodape) == "ls /a"
    assert re.search(r"--code '(.*)'", com_aspa_no_rodape, re.S).group(1) != "ls /a"


def test_offer_de_tool_result_sob_a_linha_de_um_turno_de_usuario(tmp_path: Path) -> None:
    """Um `tool_result` (o deny) nunca divide linha com o turno `user` que o gerou.

    Este teste existe porque um `continue` mal posto depois de contar o turno pulava
    o `content` da MESMA linha, e o deny deixava de ser lido — a extração devolvia
    zero ofertas e a falha só aparecia no `--check` ("audit-val sem not_followed"),
    longe do ponto do erro. Nenhum outro testepegava: as fixtures tinham uma coisa
    por linha.
    """
    deny = ("PreToolUse:Bash hook error: [CODE MODE · rajada] 2ª inspeção `cat`:\n"
            "  touring run --lang bash --code 'cat /a/x.rs /b/y.rs'\n"
            "Um `touring run` zera a janela.")
    linhas = [
        json.dumps({"timestamp": "2026-09-01T10:00:00Z", "type": "user",
                    "message": {"content": "olá"}}),
        _linha_denegar(deny, "2026-09-01T10:00:01Z"),
    ]
    p = tmp_path / "aaaa1111.jsonl"
    p.write_text("\n".join(linhas) + "\n", encoding="utf-8")
    assert ex.extract_session(p), "o deny tem de ser lido mesmo numa linha de turno"
