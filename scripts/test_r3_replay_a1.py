#!/usr/bin/env python3
"""Testes do replay A1 — com foco nos TRÊS bugs que deram número plausível e errado.

Cada bug abaixo produziu um número RAZOÁVEL (0,6635 → 0,1682 → 0,1094) sem erro visível.
O critério que os pega não é "a saída parece certa" e sim uma ASSERÇÃO sobre a
ESTRUTURA que só é verdadeira quando o bug não está lá.

1. a chave `(session, turno)` não identifica um bloco;
2. o texto é por BLOCO, não por turno;
3. o casamento do dataset com o índice tem PISO, não igualdade frouxa.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import r3_replay_a1 as ra

# Um turno com DOIS blocos — o caso que a chave (session, turno) achata.
BLOCO_A = "TOURING SUGGEST · a — MUST touring index find Foo"
BLOCO_B = "TOURING SUGGEST · b — MUST touring run --lang bash"


def _dataset_com_irmaos() -> list[dict]:
    """Dois blocos no MESMO turno, como o dataset real tem (8.837 de 13.825)."""
    return [
        {
            "session": "aaaa1111",
            "turno": 7,
            "family": "touring-suggest",
            "attachment_type": "hook_success",
            "bytes": 100,
            "propositions": 1,
            "is_touring_family": True,
            "is_exempt": False,
            "is_duplicate": False,
            "head": BLOCO_A[:40],
        },
        {
            "session": "aaaa1111",
            "turno": 7,
            "family": "touring-suggest",
            "attachment_type": "hook_success",
            "bytes": 120,
            "propositions": 1,
            "is_touring_family": True,
            "is_exempt": False,
            "is_duplicate": False,
            "head": BLOCO_B[:40],
        },
    ]


def test_chave_session_turno_nao_identifica_um_bloco() -> None:
    """BUG 1: dois blocos do MESMO turno precisam de ids distintos.

    A chave `(session, turno)` achataria os dois em 1 id — e o veredito do segundo
    sobrescreveria o do primeiro. Este teste prova que a chave REAL não faz isso,
    e que a chave achatada de fato faria (para o mutante ter o que matar).
    """
    blocos = _dataset_com_irmaos()
    # a chave QUE ACHATA (o bug), aplicada de propósito:
    achatado = {f"{b['session']}:{b['turno']}" for b in blocos}
    assert len(achatado) == 1, (
        "a chave (session,turno) deveria achatar 2 blocos em 1 — se não achatou, "
        "a fixture parou de representar o caso real"
    )
    # a chave REAL:
    seq: dict[tuple[str, int], int] = {}
    for b in blocos:
        k = (b["session"], b["turno"])
        b["block_id"] = f"{k[0]}:{k[1]}:{seq.get(k, 0)}"
        seq[k] = seq.get(k, 0) + 1
    assert len({b["block_id"] for b in blocos}) == 2, (
        "o block_id real achatou dois blocos — é o bug 1 de volta"
    )


def test_block_id_eh_unico_por_bloco_no_dataset() -> None:
    """O `block_id` real distingue irmãos — e é POSICIONAL, estável por hash."""
    blocos = _dataset_com_irmaos()
    seq: dict[tuple[str, int], int] = {}
    for b in blocos:
        k = (b["session"], b["turno"])
        b["block_id"] = f"{k[0]}:{k[1]}:{seq.get(k, 0)}"
        seq[k] = seq.get(k, 0) + 1
    assert blocos[0]["block_id"] != blocos[1]["block_id"], "irmãos colaram no mesmo id"
    assert ra._seq_de(blocos[0]) == 0 and ra._seq_de(blocos[1]) == 1


def test_texto_e_por_bloco_e_nao_por_turno() -> None:
    """BUG 2: o índice guardava só o PRIMEIRO texto do turno (`setdefault`).

    Os dois irmãos ficavam com o mesmo texto — e portanto o mesmo hash e o mesmo
    id — e a regra de duplicata via "duplicata exata" via para o segundo sem
    ser duplicata.
    """
    textos = {("aaaa1111", 7, 0): BLOCO_A, ("aaaa1111", 7, 1): BLOCO_B}
    blocos = _dataset_com_irmaos()
    blocos[0]["block_id"], blocos[1]["block_id"] = "aaaa1111:7:0", "aaaa1111:7:1"
    t0 = ra.texto_de(textos, blocos[0])
    t1 = ra.texto_de(textos, blocos[1])
    assert t0 == BLOCO_A and t1 == BLOCO_B, (
        f"os irmãos receberam o mesmo texto: {t0!r} / {t1!r} — é o bug 2"
    )


def test_casamento_do_dataset_com_o_indice_tem_piso() -> None:
    """BUG 3 (o critério que a Songsera_PROPôs): o casamento tem PISO, não igualdade.

    Igualdade frouxa ("qualquer coisa que case") deixa passar a variante que
    casava 7 de 28.688. O piso é 99% e a contagem é explícita.
    """
    # um índice sintético com 1000 blocos, dos quais 995 casam
    blocos = []
    textos = {}
    for i in range(1000):
        b = {
            "session": f"s{i // 10:04d}",
            "turno": i % 10,
            "block_id": f"s{i // 10:04d}:{i % 10}:0",
        }
        blocos.append(b)
        if i < 995:
            textos[(b["session"], b["turno"], 0)] = f"texto {i}"
    casados = sum(1 for b in blocos if ra.texto_de(textos, b))
    piso = 0.99 * len(blocos)
    assert casados >= piso, (
        f"casamento {casados}/{len(blocos)} abaixo do piso {piso:.0f} — "
        "o índice e o dataset discordam"
    )
    # e a variante frouxa (qualquer texto não-vazio) casaria 1000 — por isso o piso
    frouxo = sum(1 for b in blocos if b["block_id"])
    assert frouxo > casados, (
        "o teste precisa do casamento REAL, não de um proxy que sempre passa"
    )


def test_regra_nao_descarta_bloque_exempt() -> None:
    """Um único EXEMPT descartado invalida o braço (CRITERIOS §1) — o teste é a
    defesa, e é o que o gate mede como `exempt_retained`."""
    blocos = [
        {
            "session": "aaaa1111",
            "turno": 1,
            "family": "code-mode-deny",
            "block_id": "aaaa1111:1:0",
            "bytes": 500,
            "propositions": 0,
            "is_touring_family": True,
            "is_exempt": True,
            "is_duplicate": False,
        }
    ]
    textos = {("aaaa1111", 1, 0): "[CODE MODE · rajada] MUST touring run"}
    veredito = ra.braço_regra(blocos, textos)
    v = veredito["aaaa1111:1:0"]
    assert v["exempt"] is True and v["descarta"] is False, (
        f"um EXEMPT foi descartado: {v}"
    )


def test_regra_descarta_zero_proposicao_nao_exempt() -> None:
    """O outro lado: 0 proposições e não-EXEMPT É descartável (é a regra)."""
    blocos = [
        {
            "session": "aaaa1111",
            "turno": 1,
            "family": "touring-suggest",
            "block_id": "aaaa1111:1:0",
            "bytes": 500,
            "propositions": 0,
            "is_touring_family": True,
            "is_exempt": False,
            "is_duplicate": False,
        }
    ]
    textos = {("aaaa1111", 1, 0): "TOURING SUGGEST · x"}
    v = ra.braço_regra(blocos, textos)["aaaa1111:1:0"]
    assert v["descarta"] is True and v["exempt"] is False


def test_universo_de_a1_exclui_a_familia_other() -> None:
    """E1.1: `other` é do harness (listagem de skills, buffer REMEMBER) e nenhum
    gate do Touring pode descartá-lo — o denominador são as 6 famílias do Touring."""
    blocos = [
        {
            "session": "s",
            "turno": 1,
            "family": "touring-suggest",
            "block_id": "s:1:0",
            "bytes": 100,
            "propositions": 1,
            "is_touring_family": True,
            "is_exempt": False,
            "is_duplicate": False,
        },
        {
            "session": "s",
            "turno": 2,
            "family": "other",
            "block_id": "s:2:0",
            "bytes": 90000,
            "propositions": 0,
            "is_touring_family": False,
            "is_exempt": False,
            "is_duplicate": False,
        },
    ]
    part = {("s", 1): "test", ("s", 2): "test"}
    m = ra.metricas(blocos, part, None)
    assert m["bytes_totais"] == 100, (
        f"o `other` (90.000 B) entrou no denominador: {m['bytes_totais']}"
    )


# ─────────────────────────────────────────────── cache (o gasto de API)


def test_re_run_sem_mudanca_da_zero_chamadas(tmp_path: Path, monkeypatch) -> None:
    """Re-run sem mudança de entrada tem de dar 0 chamadas de API.

    O gasto de 1.514 chamadas veio de reexecutar o replay inteiro: o braço re-perguntava
    o que já estava no arquivo. Aqui o mutante é "o cache é ignorado" — e ele tem de
    reprovar, porque a chamada viria.
    """
    cache = tmp_path / "cache.jsonl"
    bloco = {
        "session": "aaaa1111",
        "turno": 1,
        "family": "touring-suggest",
        "block_id": "aaaa1111:1:0",
        "bytes": 100,
        "propositions": 1,
        "attachment_type": "hook_success",
        "head": "TOURING SUGGEST x",
        "is_touring_family": True,
        "is_exempt": False,
        "is_duplicate": False,
    }
    textos = {("aaaa1111", 1, 0): "TOURING SUGGEST · x"}

    chamadas = {"n": 0}

    def _fake_api(*a, **k):
        chamadas["n"] += 1
        return {ra.QID: {"type": "noul", "noul": 0.42}}

    monkeypatch.setattr(ra, "_prob", lambda corpo, qid: 0.42)
    monkeypatch.setattr("time.perf_counter", lambda: 0.0)
    import jev_client

    monkeypatch.setattr(jev_client, "system_one", _fake_api)

    # 1ª rodada: escreve no cache
    ra.braço_jev([bloco], textos, verbose=False, cache_path=cache)
    assert chamadas["n"] == 1, (
        f"a 1ª rodada deveria chamar 1 vez, chamou {chamadas['n']}"
    )
    assert cache.exists(), "o cache não foi gravado"

    # 2ª rodada: MESMA entrada, 0 chamadas
    out = ra.braço_jev([bloco], textos, verbose=False, cache_path=cache)
    assert chamadas["n"] == 1, (
        f"o re-run chamou a API de novo (total {chamadas['n']}) — o cache foi ignorado"
    )
    assert out["aaaa1111:1:0"]["p_discard"] == 0.42, "a predição do cache não voltou"


def test_mudar_a_pergunta_invalida_o_cache(tmp_path: Path, monkeypatch) -> None:
    """Mudar a `spec` da pergunta tem de invalidar o cache — senão o re-run devolve a
    resposta da PERGUNTA ANTIGA, que é o mesmo modo de falha com outra roupa."""
    chamadas = {"n": 0}

    def _fake_api(*a, **k):
        chamadas["n"] += 1
        return {ra.QID: {"type": "noul", "noul": 0.9}}

    monkeypatch.setattr("time.perf_counter", lambda: 0.0)
    import jev_client

    monkeypatch.setattr(jev_client, "system_one", _fake_api)

    ck_a = ra._chave_cache("B-J", "x", "pergunta-antiga")
    ck_b = ra._chave_cache("B-J", "x", "pergunta-nova")
    assert ck_a != ck_b, "a pergunta não entra na chave do cache"
    # e a versão entra: subir VERSAO_CACHE invalida tudo
    v1 = ra._chave_cache("B-J", "x", "p")
    antigo = ra.VERSAO_CACHE
    try:
        ra.VERSAO_CACHE = antigo + 1
        v2 = ra._chave_cache("B-J", "x", "p")
    finally:
        ra.VERSAO_CACHE = antigo
    assert v1 != v2, "mudar VERSAO_CACHE tem de invalidar o cache"


def test_cache_truncado_nao_derruba_a_execucao(tmp_path: Path) -> None:
    """Um cache com linha truncada descarta a LINHA, não o arquivo — o mesmo
    tratamento que o extrator dá a transcript malformado."""
    c = tmp_path / "cache.jsonl"
    c.write_text(
        '{"cache_key": "1", "pred": {"p_discard": 0.3}}\n{"cache_key": ',
        encoding="utf-8",
    )
    cache = ra.carregar_cache(c)
    assert cache == {"1": {"p_discard": 0.3}}, f"o cache inteiro foi perdido: {cache}"


# ─────────────────────────── a saída de um replay não é a do outro (S-7/S-6)


def test_replay_nao_apaga_a_saida_do_outro(tmp_path: Path, monkeypatch) -> None:
    """Rodar o A1 não pode remover artefatos do A2 — e vice-versa.

    O sintoma que trouxe isto aqui: rodar o A1 deixou o diretório de saída com
    SÓ artefatos do A1, e o `find` do A2 não achou nada. Nenhum código apagava
    nada — os dois replays só `mkdir(exist_ok=True)` — mas COMPARTILHAVAM o
    diretório, então um `find` do A2 procurava no lugar errado depois de o A1
    escrever. Aqui os dois têm diretório próprio, por construção.

    **O que este teste NÃO pode fazer:** confiar que trocar `_RAIZ` redireciona
    a escrita. `SAIDA` é calculada no IMPORT do módulo, então é uma constante
    — trocar `_RAIZ` depois não muda `SAIDA` em nada, e o teste escrevia no
    diretório REAL por cima dos artefatos de verdade. (Foi assim que a 1ª versão
    deste arquivo destruiu `a2/results-a2.json`.) Por isso o monkeypatch é na
    CONSTANTE (`SAIDA`), e a verificação seguinte prova que o caminho real
    não foi tocado.
    """
    import r3_replay_a2 as a2

    # o caminho REAL, antes de qualquer troca: é contra ele que a escrita é medida
    real_a1, real_a2 = ra.SAIDA, a2.SAIDA
    antes = _assinatura_da_saida(real_a1) | _assinatura_da_saida(real_a2)

    saida_a1, saida_a2 = tmp_path / "a1", tmp_path / "a2"
    monkeypatch.setattr(ra, "SAIDA", saida_a1)
    monkeypatch.setattr(a2, "SAIDA", saida_a2)
    assert ra.SAIDA != a2.SAIDA, "os dois replays compartilham a saida"
    assert ra.SAIDA.name == "a1" and a2.SAIDA.name == "a2"

    saida_a1.mkdir(parents=True, exist_ok=True)
    saida_a2.mkdir(parents=True, exist_ok=True)
    (saida_a1 / "results-a1.json").write_text(
        '{"use_case":"a1_relevance"}', encoding="utf-8"
    )
    (saida_a2 / "results-a2.json").write_text(
        '{"use_case":"a2_outcome"}', encoding="utf-8"
    )

    # rodar o A1 de verdade (só a regra, sem modelo) no SANDBOX
    _rodar_so_regra(monkeypatch)

    assert (saida_a2 / "results-a2.json").exists(), (
        "rodar o A1 APAGOU o results-a2.json do outro replay"
    )
    # e o diretório REAL tem de estar intacto — é a propriedade que faltava
    depois = _assinatura_da_saida(real_a1) | _assinatura_da_saida(real_a2)
    assert depois == antes, (
        "a suíte alterou o diretório de saida REAL:\n"
        f"  antes={ {k: v[0][:8] for k, v in antes.items()} }\n"
        f"  depois={ {k: v[0][:8] for k, v in depois.items()} }"
    )


def _assinatura_da_saida(d: Path) -> dict:
    """`caminho -> (sha256, mtime)` de tudo que existe sob `d`.

    Sem isto, um teste que escreve no diretório real passa sem erro nenhum: o
    arquivo é sobrescrito com sucesso e o assert do teste continua verde.
    """
    import hashlib

    if not d.exists():
        return {}
    out = {}
    for p in sorted(d.rglob("*")):
        if p.is_file():
            out[str(p)] = (
                hashlib.sha256(p.read_bytes()).hexdigest(),
                p.stat().st_mtime,
            )
    return out


def _rodar_so_regra(monkeypatch) -> None:
    """Roda `ra.main()` só com a regra (sem modelo nem corpus): o que se observa é a escrita."""
    monkeypatch.setattr(ra, "carregar_blocos", list)
    monkeypatch.setattr(ra, "indexar_texto", dict)
    monkeypatch.setattr(sys, "argv", ["r3_replay_a1.py", "--so-regra"])
    try:
        ra.main()
    except SystemExit:
        pass


def _evento_a2() -> dict:
    """O evento `a2_offer_closed` mínimo que os testes de `cache_only` dão ao braço A2."""
    return {
        "session_hash": "abc",
        "kind": "a2_offer_closed",
        "use_case": "a2_outcome",
        "mode": "code",
        "closed_by": "consumed",
        "cmd_heads": ["ls"],
        "baseline_value": 1.0,
    }


def _carregar_de(corpus: Path) -> list:
    """`ra.carregar_blocos()` lendo `corpus`; `ra.CORPUS` volta ao valor antigo ao sair."""
    antigo = ra.CORPUS
    try:
        ra.CORPUS = corpus
        return ra.carregar_blocos()
    finally:
        ra.CORPUS = antigo


def test_a_saida_real_esta_preservada_quando_a_constante_e_trocada(
    tmp_path: Path, monkeypatch
) -> None:
    """Trocando `SAIDA` (e não `_RAIZ`), nada do diretório real muda.

    Este é o teste-guarda: se alguém voltar a redirecionar por `_RAIZ`, a
    escrita volta ao caminho real e este teste reprova — que é o que faltava
    na 1ª versão, em que o teste "provava" a estrutura enquanto destruía os
    artefatos de verdade.
    """
    real = ra.SAIDA
    antes = _assinatura_da_saida(real)
    if not antes:
        pytest.skip("sem saida real para preservar (ainda nao houve replay)")
    monkeypatch.setattr(ra, "SAIDA", tmp_path / "a1")
    _rodar_so_regra(monkeypatch)
    assert _assinatura_da_saida(real) == antes, "a escrita foi para o diretorio REAL"


def test_cache_only_aborta_em_vez_de_chamar_a_api(monkeypatch) -> None:
    """`cache_only`: cache miss ABORTA o braço, nunca chama a rede.

    É o que garante que regerar o A2 não gasta cota: o contador do
    `jev_journal.jsonl` tem de ficar igual antes e depois.
    """
    import r3_replay_a2 as a2

    chamadas = {"n": 0}

    def _fake(*args, **kwargs):
        chamadas["n"] += 1
        return {a2.QID: {"type": "noul", "noul": 0.4}}

    import jev_client

    monkeypatch.setattr(jev_client, "system_one", _fake)
    monkeypatch.setattr(jev_client, "_read_cache", lambda p: {})

    ev = _evento_a2()
    out = a2.braço_jev(
        [ev],
        verbose=False,
        cache_path=Path(os.devnull).parent / "nao-existe.jsonl",
        cache_only=True,
    )
    assert chamadas["n"] == 0, f"cache_only chamou a API {chamadas['n']}x"
    assert "CACHE_MISS" in out[0].get("erro", ""), (
        f"o cache miss tem de ser declarado: {out[0]}"
    )


# ─────── os 3 bugs, testados contra o CÓDIGO REAL (não contra uma recópia)


def test_dataset_real_da_block_id_distinta_para_irmaos_do_turno() -> None:
    """BUG 1, contra `carregar_blocos()` de verdade.

    A versão anterior deste teste reimplementava a chave dentro do teste — o que
    provava que a MINHA cópia achatava, não que a do código achatava. Aqui a
    fixture vai pelo carregador real: um turno com dois blocos tem de produzir
    dois `block_id` distintos.
    """
    import tempfile

    blocos = _dataset_com_irmaos()
    with tempfile.TemporaryDirectory() as d:
        c = Path(d) / "a1_blocks.jsonl"
        c.write_text(
            "\n".join(json.dumps(b, ensure_ascii=False) for b in blocos) + "\n",
            encoding="utf-8",
        )
        p = Path(d) / "corpus"
        p.mkdir()
        (p / "a1_blocks.jsonl").write_text(
            c.read_text(encoding="utf-8"), encoding="utf-8"
        )
        carregados = _carregar_de(p)
    ids = [b["block_id"] for b in carregados]
    assert len(set(ids)) == 2, (
        f"o carregador REAL achatou dois blocos do mesmo turno em {len(set(ids))} id(s): {ids}"
    )


def test_indice_real_da_texto_distinto_por_bloco() -> None:
    """BUG 2, contra `indexar_texto()` de verdade: dois blocos, dois textos.

    O teste anterior reimplementava o dicionário. Este grava um transcript
    sintético com dois `hook_success` no MESMO turno e exige que o índice real
    guarde os DOIS textos em posições diferentes.
    """
    import tempfile

    # o formato REAL: `attachment` com `type: hook_success` e o texto em `stdout`
    # como JSON de `hookSpecificOutput.additionalContext` — é o que
    # `ex._texts_do_attachment` lê (context_budget.rs:218-251).
    def _att(tipo, texto):
        return json.dumps(
            {
                "type": "attachment",
                "attachment": {
                    "type": tipo,
                    "stdout": json.dumps(
                        {"hookSpecificOutput": {"additionalContext": texto}}
                    ),
                },
            }
        )

    linhas = [
        json.dumps(
            {
                "timestamp": "2026-09-01T10:00:00Z",
                "type": "user",
                "message": {"content": "oi"},
            }
        ),
        _att("hook_success", BLOCO_A),
        _att("hook_success", BLOCO_B),
    ]
    with tempfile.TemporaryDirectory() as d:
        proj = Path(d) / "proj"
        proj.mkdir()
        (proj / "aaaa1111.jsonl").write_text("\n".join(linhas) + "\n", encoding="utf-8")
        antigo = ra.ex.PROJ
        try:
            ra.ex.PROJ = str(proj)
            textos = ra.indexar_texto()
        finally:
            ra.ex.PROJ = antigo
    assert textos.get(("aaaa1111", 1, 0)) == BLOCO_A, (
        f"o 1o bloco nao foi indexado: {textos}"
    )
    assert textos.get(("aaaa1111", 1, 1)) == BLOCO_B, (
        f"o 2o bloco do MESMO turno nao foi indexado — é o bug 2: {textos}"
    )


def test_block_id_real_usa_posicao_ENTRE_irmaos_nao_por_arquivo() -> None:
    """BUG 1, mutante correto: `i % 2` numa fixture de 2 blocos ainda dá ids
    distintos, então não mata nada. O que separa as duas implementações é a
    POSIÇÃO DENTRO DO TURNO: num dataset onde um turno tem 3 blocos, a posição
    global (0,1,2) e a posição no turno (0,1,2) coincidem; o que as separa é o
    SEGUNDO turno — a posição global continuaria, a do turno reinicia.
    """
    import tempfile

    blocos = [
        {
            "session": "aaaa1111",
            "turno": 7,
            "family": "touring-suggest",
            "attachment_type": "hook_success",
            "bytes": 10,
            "propositions": 1,
            "is_touring_family": True,
            "is_exempt": False,
            "is_duplicate": False,
        },
        {
            "session": "aaaa1111",
            "turno": 7,
            "family": "touring-suggest",
            "attachment_type": "hook_success",
            "bytes": 20,
            "propositions": 1,
            "is_touring_family": True,
            "is_exempt": False,
            "is_duplicate": False,
        },
        {
            "session": "aaaa1111",
            "turno": 9,
            "family": "touring-suggest",
            "attachment_type": "hook_success",
            "bytes": 30,
            "propositions": 1,
            "is_touring_family": True,
            "is_exempt": False,
            "is_duplicate": False,
        },
    ]
    with tempfile.TemporaryDirectory() as d:
        p = Path(d)
        (p / "a1_blocks.jsonl").write_text(
            "\n".join(json.dumps(b, ensure_ascii=False) for b in blocos) + "\n",
            encoding="utf-8",
        )
        carregados = _carregar_de(p)
    terceiro = carregados[2]["block_id"]
    assert terceiro.endswith(":0"), (
        f"o 3o bloco (1o do turno 9) tem block_id {terceiro!r}: a posição NÃO reinicia "
        "por turno — é a global do arquivo (o bug 1)"
    )
    assert len({b["block_id"] for b in carregados}) == 3


def test_cache_only_nao_chama_a_api_mesmo_com_o_jev_cache_cheio(monkeypatch) -> None:
    """`cache_only` tem de servir do `jev_cache` do PRÓPRIO cliente.

    O teste anterior esvaziava o `_read_cache`, então cobria só o caminho do
    CACHE_MISS. Este põe uma entrada REAL no cache: com o caminho certaininho, o
    braço tem de devolver a predição do cache e não chamar a rede — se o
    `if cache_only and jev_cache` sumir, a chamada acontece e o teste reprova.
    """
    import jev_client
    import r3_replay_a2 as a2

    chamadas = {"n": 0}

    def _fake(*a, **k):
        chamadas["n"] += 1
        return {a2.QID: {"type": "noul", "noul": 0.4}}

    ev = _evento_a2()
    perguntas = {
        a2.QID: {
            "type": "noul",
            "instructions": "Did the agent follow the recommended route after the nudge?",
        }
    }
    chave = jev_client._cache_key(a2.payload_of(ev), perguntas, "jev-1.13.0")
    monkeypatch.setattr(jev_client, "system_one", _fake)
    monkeypatch.setattr(
        jev_client,
        "_read_cache",
        lambda p: {chave: {a2.QID: {"type": "noul", "noul": 0.77}}},
    )
    out = a2.braço_jev(
        [ev],
        verbose=False,
        cache_path=Path(os.devnull).parent / "inexistente.jsonl",
        cache_only=True,
    )
    assert chamadas["n"] == 0, (
        f"cache_only chamou a API apesar do cache cheio ({chamadas['n']}x)"
    )
    assert out[0].get("p_followed") == 0.77, f"a predição do cache não voltou: {out[0]}"


def test_d6_os_dois_denominadores_sao_declarados_sobre_o_mesmo_corte() -> None:
    """D-6 (b, 31/10/2026): o corte é o mesmo; muda só o denominador. Um bloco de família
    do Touring fora das 6 de E1.1 (`pre-edit`) entra no denominador S-4 e não é cortado;
    `plugin-hook`, `generic-noise` e `other` ficam fora dos dois; `train` não conta."""

    def b(fam: str, n: int, turno: int = 1) -> dict:
        return {"session": "s", "turno": turno, "family": fam, "bytes": n}

    blocos = [
        b("touring-suggest", 100),
        b("pre-edit", 300),
        b("plugin-hook", 50),
        b("generic-noise", 9),
        b("other", 40),
        b("touring-suggest", 999, turno=2),
    ]
    part = {("s", 1): "test", ("s", 2): "train"}
    m_bd = {"bytes_totais": 100, "bytes_mantidos": 50}  # o braço cortou 50 B de E1.1
    den = ra.denominadores(blocos, part, m_bd)
    assert den["e1_1"] == {"bytes": 100, "bytes_saved": 0.5}
    assert den["taxonomia_s4"] == {"bytes": 400, "bytes_saved": 0.125}
    assert not ra.kpi.is_touring_by_taxonomy("plugin-hook")
    assert ra.kpi.is_touring_by_taxonomy("pre-edit")
    # As 6 de E1.1 são subconjunto da taxonomia S-4: o denominador declarado nunca é menor.
    assert all(ra.kpi.is_touring_by_taxonomy(f) for f in ra.kpi.TOURING_FAMILIES)
