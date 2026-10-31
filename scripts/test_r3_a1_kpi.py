#!/usr/bin/env python3
"""Testes do S-3: paridade do porte do KPI + EXEMPT + duplicata.

O teste de paridade roda contra o KPI REAL (`touring kpi -j`) no mesmo instante
do extrator — comparar com um JSON guardado em disco é comparar com o passado, e o
corpus cresce a cada turno.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import r3_a1_kpi as kpi  # noqa: E402
import r3_extract_a1_blocks as ex  # noqa: E402


def _attachment(tipo: str, texto: str) -> str:
    """Um bloco de transcript no formato que o Claude Code escreve (a forma que o
    próprio teste Rust do KPI usa, `context_budget.rs:1069`)."""
    inner = json.dumps({"hookSpecificOutput": {"additionalContext": texto}})
    return json.dumps({"type": "attachment",
                       "attachment": {"type": tipo, "stdout": inner}})


def test_family_table_is_the_rust_one() -> None:
    """A tabela e a ORDEM: `code-mode-deny` primeiro, e o catch-all no fim.

    Um bloco que casa duas agulhas pertence à PRIMEIRA família da tabela
    (a régua: injection_family.rs:41-50) — inverter a ordem troca a classificação
    de todo bloco que contém `CODE MODE` e `TOURING SUGGEST`.
    """
    assert kpi.injection_family("[CODE MODE · rajada] x") == "code-mode-deny"
    assert kpi.injection_family("[TOURING SUGGEST · cli] x") == "touring-suggest"
    assert kpi.injection_family("=== REMEMBER ===\nhistórico") == kpi.OTHER
    # as duas agulhas no mesmo texto: a primeira da tabela vence
    ambos = "[TOURING SUGGEST] e CODE MODE"
    assert kpi.injection_family(ambos) == "code-mode-deny", (
        "a ordem da tabela decide (a régua: injection_family.rs:88), e code-mode-deny vem antes"
    )


# --- A RÉGUA, lida do FONTE ---------------------------------------------------
# A taxonomia canônica NÃO está mais em `context_budget.rs`: a S-4 (26/10/2026)
# moveu-a para `crates/touring-foundation/src/injection_family.rs`, e
# `context_budget.rs:204` re-exporta as funções. Um guard que lê o caminho
# velho lê um arquivo que já não guarda a tabela — e morre com IndexError, sem
# dizer qual linha estava procurando.
#
# A leitura é por SCANNER, não por regex ingênua: as agulhas contêm `]`
# ("[EVENT_READER]") e os padrões são raw strings (`r"\[G\d+\s"`). Um `.*?` até
# o primeiro `]` parte a agulha no meio — e o guard passaria a comparar MEIOS,
# que é a falha silenciosa que ele existe para impedir.
REPO = Path(__file__).parent.parent
REGUA = REPO / "crates/touring-foundation/src/injection_family.rs"


def _pula_ws(s: str, pos: int) -> int:
    """Passa por espaços e comentários Rust."""
    n = len(s)
    while pos < n:
        if s[pos] in " \t\r\n":
            pos += 1
        elif s.startswith("//", pos):
            fim = s.find("\n", pos)
            pos = n if fim < 0 else fim + 1
        elif s.startswith("/*", pos):
            fim = s.find("*/", pos)
            pos = n if fim < 0 else fim + 2
        else:
            break
    return pos


def _le_string(s: str, pos: int) -> tuple[str, int]:
    """Lê uma string Rust (normal ou raw) em `pos`. Devolve (valor, índice seguinte)."""
    if s[pos] == "r":
        abre = pos + 1
        while s[abre] == "#":
            abre += 1
        assert s[abre] == '"', s[pos:pos + 20]
        fecha = '"' + "#" * (abre - pos - 1)
        fim = s.index(fecha, abre + 1)
        return s[abre + 1:fim], fim + len(fecha)
    cursor = pos + 1
    pedacos: list[str] = []
    while True:
        c = s[cursor]
        if c == "\\":
            nxt = s[cursor + 1]
            pedacos.append({"n": "\n", "t": "\t", "r": "\r", '"': '"', "\\": "\\"}.get(nxt, nxt))
            cursor += 2
            continue
        if c == '"':
            return "".join(pedacos), cursor + 1
        pedacos.append(c)
        cursor += 1


def _corpo_const(src: str, nome: str) -> str:
    """O corpo do `= &[...]` de uma const do Rust, sem as chaves do array."""
    inicio = src.index("pub const " + nome)
    # `len("= &[")` e não `+3`: o "=" e o espaço e o "&" e o "[" — o corpo começa
    # DEPOIS do "[", ou a profundidade começa errada e o array nunca fecha.
    corpo = src.index("= &[", inicio) + len("= &[")
    pos = corpo
    profundidade = 1
    while profundidade:
        c = src[pos]
        if c == '"' or (c == "r" and src[pos + 1] in '"#'):
            _, pos = _le_string(src, pos)
            continue
        if c == "[":
            profundidade += 1
        elif c == "]":
            profundidade -= 1
            if profundidade == 0:
                return src[corpo:pos]
        pos += 1
    raise AssertionError(f"const {nome} sem fechamento")


def _item(corpo: str, pos: int) -> tuple[tuple[str, object], int]:
    """Um item de uma entrada Rust: ('s', string) | ('l', lista) | ('x', identificador)."""
    c = corpo[pos]
    if c == '"' or (c == "r" and corpo[pos + 1] in '"#'):
        valor, pos = _le_string(corpo, pos)
        return ("s", valor), pos
    if c == "&":
        pos = _pula_ws(corpo, pos + 1)
        assert corpo[pos] == "[", corpo[pos:pos + 20]
        pos += 1
        lista: list[str] = []
        while True:
            pos = _pula_ws(corpo, pos)
            if corpo[pos] == "]":
                pos += 1
                break
            if corpo[pos] == ",":
                pos += 1
                continue
            valor, pos = _le_string(corpo, pos)
            lista.append(valor)
        return ("l", lista), pos
    fim = pos
    while fim < len(corpo) and corpo[fim] not in ",()":
        fim += 1
    return ("x", corpo[pos:fim].strip()), fim


def _entradas(corpo: str) -> list[list]:
    """As entradas de um array Rust: cada uma é uma lista de itens."""
    entradas: list[list] = []
    pos = 0
    n = len(corpo)
    while True:
        pos = _pula_ws(corpo, pos)
        if pos >= n:
            return entradas
        if corpo[pos] == ",":
            pos += 1
            continue
        assert corpo[pos] == "(", corpo[pos:pos + 40]
        pos += 1
        itens: list[tuple[str, object]] = []
        while True:
            pos = _pula_ws(corpo, pos)
            if corpo[pos] == ")":
                pos += 1
                break
            item, pos = _item(corpo, pos)
            itens.append(item)
            pos = _pula_ws(corpo, pos)
            if pos < n and corpo[pos] == ",":
                pos += 1
        entradas.append(itens)


def _tabelas_rust() -> dict:
    """As TRÊS tabelas da régua, lidas do fonte Rust."""
    src = REGUA.read_text(encoding="utf-8")
    marcadores = [
        (it[0][1], tuple(it[1][1])) for it in _entradas(_corpo_const(src, "FAMILY_MARKERS"))
    ]
    padroes = [
        (it[0][1], tuple(it[1][1])) for it in _entradas(_corpo_const(src, "FAMILY_PATTERNS"))
    ]
    emissores = []
    for it in _entradas(_corpo_const(src, "EMITTERS")):
        claim = it[2][1]
        assert str(claim).startswith("Claim::"), claim
        emissores.append((it[0][1], it[1][1], claim == "Claim::Owns"))
    return {"marcadores": marcadores, "padroes": padroes, "emissores": emissores}


def test_the_port_mirrors_the_canonical_taxonomy() -> None:
    """As TRÊS tabelas do porte == as três da régua, lidas do FONTE.

    O porte é o TERCEIRO espelho (a régua Rust, a política Python e este porte).
    Um predicado escrito duas vezes nunca se encontra — então este teste
    compara as três tabelas com a fonte, ESTRUTURALMENTE: nome, ordem, agulhas e
    padrão. Divergir aqui reprova no CI; divergir só na paridade de 1% custaria
    um deploy inteiro para aparecer.
    """
    t = _tabelas_rust()
    assert t["marcadores"], "FAMILY_MARKERS não foi lida da régua"
    assert list(kpi.FAMILY_TABLE) == t["marcadores"], "FAMILY_TABLE divergiu da régua"
    assert [
        (n, tuple(p.pattern for p in ps)) for n, ps in kpi.FAMILY_PATTERNS
    ] == t["padroes"], "FAMILY_PATTERNS divergiu da régua"
    assert list(kpi.EMITTERS) == t["emissores"], "EMITTERS divergiu da régua"


def test_attributed_family_emitter_owns_or_defers_to_the_text() -> None:
    """Dono do emissor > texto > reserva do emissor > `other` — a regra do Rust (C10)."""
    # dono: a citação `last fail:` no texto não tira o bloco do pre-edit
    assert kpi.attributed_family("last fail: x", "Touring: analyzing blast radius...") == "pre-edit"
    # reserva: o texto decide primeiro (um deny emitido pelo cli-suggest continua deny)
    cli = "touring cli-suggest: classifying operation (Rust, in-daemon)..."
    assert kpi.attributed_family("[CODE MODE · rajada] x", cli) == "code-mode-deny"
    assert kpi.attributed_family("MUST    touring ast meta x", cli) == "touring-suggest"
    # sem marca, ou marca desconhecida: só o texto, nunca inventa família
    assert kpi.attributed_family("[gabrielgadea]", None) == kpi.OTHER
    assert kpi.attributed_family("[gabrielgadea]", "some-new-hook.sh") == kpi.OTHER
    assert kpi.attributed_family("[TOURING SUGGEST] x", None) == "touring-suggest"


def test_the_real_emitter_fixture_leaves_no_other_in_the_port() -> None:
    """A MESMA fixture real do teste Rust (45 `hook_success`, 23 marcas, caminhos anonimizados):
    o porte, com a atribuição por emissor, não deixa nada em `other`.

    S-3: o texto SOZINHO já não deixa a maioria no catch-all — era esse o defeito
    (C10, medido nas duas linguagens sobre os mesmos registros: 77,2 % dos bytes
    do corpus sem nome). Agora o texto nomeia o suficiente, e o que sobra é o
    genuinamente desconhecido."""
    fixture = Path(__file__).parent.parent / "crates/touring-cli/tests/fixtures/context_budget_emitters.jsonl"
    agg = kpi.agregar([fixture])
    assert agg["injected_blocks"] == 45
    assert kpi.OTHER not in agg["by_family"], agg["by_family"].keys()
    so_texto = sum(
        len(texto.encode("utf-8"))
        for linha in fixture.read_text(encoding="utf-8").splitlines()
        for _, texto in kpi.hook_injections(json.loads(linha))
        if kpi.injection_family(texto) == kpi.OTHER
    )
    assert so_texto / agg["injected_bytes"] < 0.2, (
        f"o texto sozinho não pode mais deixar a maioria no catch-all: {so_texto} B"
    )


def test_count_propositions_counts_must_should_may_lines() -> None:
    """`count_propositions` — context_budget.rs:215: linhas cujo trim_start
    começa por MUST/SHOULD/MAY."""
    assert kpi.count_propositions("  MUST  touring run\n  MAY x\n  SHOULD y") == 3
    assert kpi.count_propositions("nada aqui") == 0
    assert kpi.count_propositions("MUSTACHE não é MUST") == 1, (
        "começa por MUST: conta — o predicado é prefixo, não palavra"
    )


def test_only_hook_success_enters_the_kpi() -> None:
    """`hook_contexts` — context_budget.rs:248: SÓ `hook_success`.

    `hook_additional_context` NÃO entra no KPI. O universe do r3 é superset, e a
    paridade de 1% só vale sobre a interseção (E1.1).
    """
    rec_s = json.loads(_attachment("hook_success", "TOURING SUGGEST · x"))
    rec_c = json.loads(_attachment("hook_additional_context", "TOURING SUGGEST · x"))
    assert kpi.hook_contexts(rec_s) == ["TOURING SUGGEST · x"]
    assert kpi.hook_contexts(rec_c) == [], "o KPI ignora hook_additional_context (:248)"


def test_plain_text_still_costs_the_window() -> None:
    """context_budget.rs:246-247: hook que imprimiu texto puro (não JSON) ainda
    ocupa a janela — o texto entra mesmo assim."""
    rec = json.dumps({"type": "attachment",
                      "attachment": {"type": "hook_success", "stdout": "texto puro"}})
    assert kpi.hook_contexts(json.loads(rec)) == ["texto puro"]


def test_kpi_reads_only_three_newest_transcripts(tmp_path: Path) -> None:
    """`MAX_TRANSCRIPTS = 3` (context_budget.rs:40, aplicado em :909) e o mais
    recente primeiro (:801). Sem reproduzir isto, a 'paridade' compara duas
    populations diferentes."""
    for i in range(6):
        p = tmp_path / f"{i}.jsonl"
        p.write_text("{}\n")
        import os
        os.utime(p, (1_700_000_000 + i, 1_700_000_000 + i))
    escolhidos = kpi.transcripts_do_kpi(tmp_path)
    assert len(escolhidos) == kpi.MAX_TRANSCRIPTS == 3
    assert [p.name for p in escolhidos] == ["5.jsonl", "4.jsonl", "3.jsonl"], (
        f"mais recente primeiro, veio {[p.name for p in escolhidos]}"
    )


def test_exempt_covers_deny_family_and_markers() -> None:
    """CRITERIOS §1: nenhum braço pode descartar um EXEMPT. Família
    `code-mode-deny` OU marcador MUST/REGRA #/CRITICAL/BLOCK/deny/erro/permission."""
    assert ex.is_exempt("[CODE MODE · rajada] x", "code-mode-deny")
    assert ex.is_exempt("MUST touring run", "other")
    assert ex.is_exempt("REGRA #19", "other")
    assert ex.is_exempt("pre-tool guard BLOCKED", "other")
    assert not ex.is_exempt("[TOURING SUGGEST]Bytes", "touring-suggest"), (
        "um nudge de suggest sem marcador não é EXEMPT"
    )


def test_duplicate_is_exact_text_inside_five_turns(tmp_path: Path) -> None:
    """Duplicata = texto IDÊNTICO na janela de 5 turnos (plan S-3)."""
    texto = "TOURING SUGGEST · code-mode-loop"
    linhas = [_attachment("hook_success", texto)]
    for _ in range(4):
        linhas.append(json.dumps({"type": "user", "message": {"content": "oi"}}))
    linhas.append(_attachment("hook_success", texto))   # turno 4: dentro da janela
    p = tmp_path / "aaaa1111.jsonl"
    p.write_text("\n".join(linhas) + "\n")
    blocos, _ = ex.extract([p])
    assert blocos[0]["is_duplicate"] is False
    assert blocos[-1]["is_duplicate"] is True, "mesmo texto 4 turnos depois é duplicata"


def test_duplicate_outside_the_window_is_not_duplicate(tmp_path: Path) -> None:
    """Passados 5 turnos, o mesmo texto deixa de ser duplicata — a janela é o
    predicado, não a igualdade."""
    texto = "TOURING SUGGEST · code-mode-loop"
    linhas = [_attachment("hook_success", texto)]
    for _ in range(8):
        linhas.append(json.dumps({"type": "user", "message": {"content": "oi"}}))
    linhas.append(_attachment("hook_success", texto))
    p = tmp_path / "bbbb2222.jsonl"
    p.write_text("\n".join(linhas) + "\n")
    blocos, _ = ex.extract([p])
    assert blocos[-1]["is_duplicate"] is False, "8 turnos depois está fora da janela de 5"


def test_touring_and_harness_are_separated(tmp_path: Path) -> None:
    """E1.1: os denominadores de C-A1-1/C-A1-3 são os bytes das SEIS famílias do
    Touring. `other` (a listagem de skills e o buffer REMEMBER) é do harness e
    nenhum gate do Touring pode descartá-la."""
    linhas = [
        _attachment("hook_success", "[TOURING SUGGEST · cli] MUST x"),
        _attachment("hook_success", "- academic-research-writer\n  body"),
    ]
    p = tmp_path / "cccc3333.jsonl"
    p.write_text("\n".join(linhas) + "\n")
    blocos, resumo = ex.extract([p])
    assert resumo["touring_only"]["blocks"] == 1
    assert resumo["harness_only"]["blocks"] == 1
    assert blocos[0]["is_touring_family"] is True
    assert blocos[1]["is_touring_family"] is False


def _geracao_do_binario() -> tuple[bool, str]:
    """A geração do binário DEPLOYADO — verificada, não inferida.

    Comparar o porte com um binário de outra geração da taxonomia dá um número
    confidentemente errado, que é pior do que nenhum. A marca é OBJETIVA: o
    binário imprime o commit de que foi construído (`touring --version`, em
    STDERR) e a régua tem um commit — o que a tocou por último. O binário só
    vale se esse commit for ancestral do dele.

    A marca anterior (a família `code-mode-contract` aparecer com bytes no KPI)
    era dependente do CORPUS: um corpus sem esse bloco declarava "não medido" e
    a paridade de 1% deixava de ser medida — em silêncio. Aqui a ausência é
    declarada, e a presença não depende do que há nos transcripts.
    """
    try:
        p = subprocess.run(["touring", "--version"], capture_output=True, text=True, timeout=30)
    except OSError as exc:
        return False, f"o binário `touring` não respondeu: {exc}"
    m = re.search(r"git:\s+([0-9a-f]{7,40})", p.stdout + p.stderr)
    if not m:
        return False, "o binário `touring` não imprimiu o commit de construção"
    sha = m.group(1)
    rel = str(REGUA.relative_to(REPO))
    tac = subprocess.run(
        ["git", "log", "-1", "--format=%H", "--", rel],
        capture_output=True, text=True, cwd=REPO,
    ).stdout.strip()
    if not tac:
        return False, "a régua não tem commit conhecido neste repositório"
    if subprocess.run(
        ["git", "cat-file", "-e", sha + "^{commit}"],
        capture_output=True, text=True, cwd=REPO,
    ).returncode != 0:
        return False, f"a geração do binário ({sha[:8]}) não pôde ser verificada neste repositório"
    if subprocess.run(
        ["git", "merge-base", "--is-ancestor", tac, sha],
        capture_output=True, text=True, cwd=REPO,
    ).returncode == 0:
        return True, ""
    return False, (
        f"o binário deployado ({sha[:8]}) é anterior à última mudança da régua ({tac[:8]}): "
        "comparar os dois seria comparar taxonomias diferentes"
    )


def test_family_table_parity_with_context_budget_kpi() -> None:
    """O critério de paridade do S-3: bytes por família dentro de 1% do
    `touring kpi -j`, medidos NO MESMO INSTANTE (o corpus cresce a cada turno)."""
    ok, motivo = _geracao_do_binario()
    if not ok:
        pytest.skip(motivo)
    par = ex.paridade()
    if "erro" in par:
        pytest.skip(f"KPI indisponível: {par['erro']}")
    for linha in par["familias"]:
        assert abs(linha["delta_pct"]) <= 1.0, (
            f"{linha['family']}: meu {linha['meu_bytes']} vs kpi {linha['kpi_bytes']} "
            f"({linha['delta_pct']:+.2f}%)"
        )
    assert par["meu_blocks"] == par["kpi_blocks"], (
        f"blocos: meu {par['meu_blocks']} vs kpi {par['kpi_blocks']}"
    )


def test_paridade_declarada_quando_os_transcripts_nao_existem(monkeypatch) -> None:
    """Um diretório de transcripts ausente (CI) ou ilegível (sandbox) é a AUSÊNCIA
    DO DADO, não um erro: `paridade()` declara e o teste de paridade pula.

    Antes disto a leitura ficava FORA do try, e um `FileNotFoundError` derrubava a
    suíte inteira no CI — medido: `PermissionError: ~/.claude/projects/…` no sandbox.
    """
    import r3_extract_a1_blocks as ex
    monkeypatch.setattr(ex, "PROJ", "/caminho/que/nao/existe/xyz")
    r = ex.paridade()
    assert "erro" in r, f"a ausencia do dado tem de ser declarada, não estourar: {r}"
    assert "transcripts" in r["erro"], f"o erro nomeia o que faltou: {r['erro']}"
    assert "paridade_1pct" not in r, "sem leitura não há veredito de paridade"


def _paridade_simulada(monkeypatch, listas, kpi_varridos):
    """`paridade()` com o disco e o KPI simulados: `listas` são as leituras sucessivas da lista de
    transcripts; o porte e o KPI veem os mesmos bytes, só a POPULAÇÃO pode divergir."""
    import types

    import r3_extract_a1_blocks as ex
    leituras = iter(listas)
    monkeypatch.setattr(ex.kpi, "transcripts_do_kpi", lambda _d: next(leituras))
    monkeypatch.setattr(ex.kpi, "agregar", lambda paths: {
        "transcripts_scanned": len(paths), "injected_blocks": 1, "by_family": {"x": {"bytes": 10}}})
    saida = json.dumps({"context_budget": {"transcripts_scanned": kpi_varridos,
                                           "by_family": {"x": {"bytes": 10, "blocks": 1}}}})
    monkeypatch.setattr(ex.subprocess, "run", lambda *a, **k: types.SimpleNamespace(stdout=saida))
    return ex.paridade()


def test_paridade_nunca_compara_populacoes_diferentes(monkeypatch) -> None:
    """O porte e o KPI escolhem, cada um, "os 3 transcripts mais recentes" do PROJETO, de TODAS as
    sessões. Em 04/10/2026 outra sessão escreveu entre as duas leituras, o KPI leu um transcript que o
    porte não leu, e o teste reprovou um porte correto (36.459 + 14.633 = 51.092 B, exato;
    cross-audit X9). População que muda entre as leituras é declarada, nunca vira veredito."""
    a, b = Path("aaaa.jsonl"), Path("bbbb.jsonl")
    r = _paridade_simulada(monkeypatch, [[a], [b]] * 5, kpi_varridos=1)
    assert "erro" in r and "movimento" in r["erro"], r
    assert "paridade_1pct" not in r, "populações diferentes não dão veredito"
    r = _paridade_simulada(monkeypatch, [[a], [a]] * 5, kpi_varridos=2)
    assert "erro" in r and "movimento" in r["erro"], f"o KPI varreu outra quantidade: {r}"


def test_paridade_com_populacao_estavel_da_veredito(monkeypatch) -> None:
    """Controle positivo: a mesma lista antes e depois do KPI e a mesma contagem varrida dão veredito."""
    a = Path("aaaa.jsonl")
    r = _paridade_simulada(monkeypatch, [[a], [a]], kpi_varridos=1)
    assert r.get("paridade_1pct") is True, r


def test_agregador_do_kpi_conta_so_o_universe_do_kpi(tmp_path: Path) -> None:
    """O agregado `universe_kpi` conta SÓ `hook_success` (context_budget.rs:248).

    Sem esta asserção, `_acumula` pode deixar de contar o KPI e nada acusa: o
    extrator continuaria devolvendo blocos certos e um agregado errado.
    """
    linhas = [
        _attachment("hook_success", "[TOURING SUGGEST · a] MUST x"),
        _attachment("hook_additional_context", "[TOURING SUGGEST · b] MUST y"),
    ]
    p = tmp_path / "aaaa1111.jsonl"
    p.write_text("\n".join(linhas) + "\n", encoding="utf-8")
    _, resumo = ex.extract([p])
    total = sum(v["bytes"] for v in resumo["por_familia"].values())
    assert resumo["universe_kpi"]["blocks"] == 1, (
        f"o KPI ve só hook_success; viu {resumo['universe_kpi']['blocks']}"
    )
    assert 0 < resumo["universe_kpi"]["bytes"] < total, (
        f"o agregado do KPI tem de ser MENOR que o total (o universe r3 é superset): "
        f"{resumo['universe_kpi']['bytes']} de {total}"
    )


def test_linha_malformada_nao_e_turno(tmp_path: Path) -> None:
    """Uma linha truncada não pode virar turno de pessoa.

    Se `_json` devolvesse um dict vazio com `type: "user"`, o `turno` subiria e a
    janela de duplicata (5 turnos) fecharia mais cedo — um erro silencioso que
    muda o `is_duplicate` sem mudar nenhuma contagem visível.
    """
    p = tmp_path / "aaaa1111.jsonl"
    p.write_text(
        _attachment("hook_success", "[TOURING SUGGEST · x] MUST y")
        + "\n" + '{"timestamp": "2026-09-01T10:00:0\n' + "\n",
        encoding="utf-8")
    blocos, _ = ex.extract([p])
    assert blocos[0]["turno"] == 0, (
        f"a linha malformada contou como turno (virou {blocos[0]['turno']})"
    )


def test_familia_harness_nao_entra_no_universe_do_touring(tmp_path: Path) -> None:
    """`is_touring_family` é o que separa o denominador de C-A1-1 do resto.

    Um bloco de `other` (a listagem de skills) é do harness: nenhum gate do Touring
    pode descartá-lo, e ele não conta no denominador das famílias do Touring.
    """
    linhas = [
        _attachment("hook_success", "[TOURING SUGGEST · a] MUST x"),
        _attachment("hook_success", "- academic-research-writer\n  body"),
    ]
    p = tmp_path / "aaaa1111.jsonl"
    p.write_text("\n".join(linhas) + "\n", encoding="utf-8")
    blocos, resumo = ex.extract([p])
    por_familia = {b["family"]: b["is_touring_family"] for b in blocos}
    assert por_familia["touring-suggest"] is True
    assert por_familia["other"] is False, (
        "`other` é o catch-all do KPI e maioritariamente harness (E1.1)"
    )
    assert resumo["touring_only"]["blocks"] == 1 and resumo["harness_only"]["blocks"] == 1


def test_paridade_nao_medida_nao_e_reprovacao() -> None:
    """"Corpus em movimento" e KPI indisponível imprimiam "NAO <<<", a mesma linha de uma divergência
    real acima de 1% (revisão cega dos consertos de 04/10): quem lê a saída não distinguia uma régua
    que reprovou de uma que não mediu."""
    erro = ex.linha_paridade({"erro": "corpus em movimento: a população mudou"})
    assert "NAO MEDIDA" in erro and "corpus em movimento" in erro and "<<<" not in erro
    assert ex.linha_paridade({"paridade_1pct": True}).endswith("OK")
    assert ex.linha_paridade({"paridade_1pct": False}).endswith("NAO <<<")
