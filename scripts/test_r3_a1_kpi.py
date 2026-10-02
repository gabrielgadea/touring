#!/usr/bin/env python3
"""Testes do S-3: paridade do porte do KPI + EXEMPT + duplicata.

O teste de paridade roda contra o KPI REAL (`touring kpi -j`) no mesmo instante
do extrator — comparar com um JSON guardado em disco é comparar com o passado, e o
corpus cresce a cada turno.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import r3_a1_kpi as kpi  # noqa: E402
import r3_extract_a1_blocks as ex  # noqa: E402


def _attachment(tipo: str, texto: str) -> str:
    """Um bloco de transcript no formato que o Claude Code escreve (a forma que o
    próprio teste Rust do KPI usa, `context_budget.rs:827-830`)."""
    inner = json.dumps({"hookSpecificOutput": {"additionalContext": texto}})
    return json.dumps({"type": "attachment",
                       "attachment": {"type": tipo, "stdout": inner}})


def test_family_table_is_the_rust_one() -> None:
    """A tabela e a ORDEM: `code-mode-deny` primeiro, e o catch-all no fim.

    Um bloco que casa duas agulhas pertence à PRIMEIRA família da tabela
    (context_budget.rs:179-183) — inverter a ordem troca a classificação de todo
    bloco que contém `CODE MODE` e `TOURING SUGGEST`.
    """
    assert kpi.injection_family("[CODE MODE · rajada] x") == "code-mode-deny"
    assert kpi.injection_family("[TOURING SUGGEST · cli] x") == "touring-suggest"
    assert kpi.injection_family("=== REMEMBER ===\nhistórico") == kpi.OTHER
    # as duas agulhas no mesmo texto: a primeira da tabela vence
    ambos = "[TOURING SUGGEST] e CODE MODE"
    assert kpi.injection_family(ambos) == "code-mode-deny", (
        "a ordem da tabela decide (context_budget.rs:142-178), e code-mode-deny vem antes"
    )


def test_count_propositions_counts_must_should_may_lines() -> None:
    """`count_propositions` — context_budget.rs:196-203: linhas cujo trim_start
    começa por MUST/SHOULD/MAY."""
    assert kpi.count_propositions("  MUST  touring run\n  MAY x\n  SHOULD y") == 3
    assert kpi.count_propositions("nada aqui") == 0
    assert kpi.count_propositions("MUSTACHE não é MUST") == 1, (
        "começa por MUST: conta — o predicado é prefixo, não palavra"
    )


def test_only_hook_success_enters_the_kpi() -> None:
    """`hook_contexts` — context_budget.rs:222: SÓ `hook_success`.

    `hook_additional_context` NÃO entra no KPI. O universe do r3 é superset, e a
    paridade de 1% só vale sobre a interseção (E1.1).
    """
    rec_s = json.loads(_attachment("hook_success", "TOURING SUGGEST · x"))
    rec_c = json.loads(_attachment("hook_additional_context", "TOURING SUGGEST · x"))
    assert kpi.hook_contexts(rec_s) == ["TOURING SUGGEST · x"]
    assert kpi.hook_contexts(rec_c) == [], "o KPI ignora hook_additional_context (:222)"


def test_plain_text_still_costs_the_window() -> None:
    """context_budget.rs:246-247: hook que imprimiu texto puro (não JSON) ainda
    ocupa a janela — o texto entra mesmo assim."""
    rec = json.dumps({"type": "attachment",
                      "attachment": {"type": "hook_success", "stdout": "texto puro"}})
    assert kpi.hook_contexts(json.loads(rec)) == ["texto puro"]


def test_kpi_reads_only_three_newest_transcripts(tmp_path: Path) -> None:
    """`MAX_TRANSCRIPTS = 3` (context_budget.rs:40, aplicado em :805) e o mais
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


def test_family_table_parity_with_context_budget_kpi() -> None:
    """O critério de paridade do S-3: bytes por família dentro de 1% do
    `touring kpi -j`, medidos NO MESMO INSTANTE (o corpus cresce a cada turno)."""
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


def test_agregador_do_kpi_conta_so_o_universe_do_kpi(tmp_path: Path) -> None:
    """O agregado `universe_kpi` conta SÓ `hook_success` (context_budget.rs:222).

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
