#!/usr/bin/env python3
"""Guard cruzado D8 — o predicado de turno humano tem UMA fonte, em duas línguas.

O QUE ESTE TESTE IMPEDE
    Duas implementações da mesma regra em linguagens diferentes divergem em
    silêncio, e a divergência só aparece como um número errado meses depois. É
    o defeito D2 do plano `2026-09-04-economia-de-contexto` (duas escalas rivais
    para o orçamento CILA), e o anti-padrão D8 da rule `touring-4-pillars`:
    *"enforcement mora no executor, não no anúncio"* — o texto declarado e o
    predicado aplicado têm de derivar da MESMA fonte.

    O predicado aqui decide duas coisas caras:
      * `context_budget.rs`  — o DENOMINADOR de `injected_bytes_per_turn`, a
        métrica-manchete do plano;
      * `loop_turn_record.py` — ONDE COMEÇA o registro do turno (classe B).

    Medido em 04/09/2026 sobre 5 transcripts: 35 de 172 records contados como
    turno humano (20,3%) eram do harness — saída de slash command, banner de
    caveat, resumo de compactação. O denominador estava inflado 25,5%, e a régua
    subestimava em ~25% exatamente o custo que existe para expor.

COMO ELE FUNCIONA
    Lê o array `NON_HUMAN_TURN_MARKERS` do FONTE RUST (o executor) e a tupla
    homônima do FONTE PYTHON, e exige igualdade de conjunto e de ordem. Não há
    fixture: as duas leituras são dos arquivos que realmente rodam.

    O lado Python é lido do ESPELHO versionado (`client/`), não do vivo em
    `~/.claude`, porque é o espelho que existe no CI. A integridade
    espelho↔vivo é o trabalho de `test_sync_client_skills.py`; encadear os dois
    guards cobre o caminho inteiro sem que nenhum precise do outro lado.
"""
from __future__ import annotations

import ast
import re
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
RUST = REPO / "crates/touring-cli/src/cli/context_budget.rs"
PY_MIRROR = REPO / "client/skills/loop-engineering/scripts/hooks/loop_turn_record.py"

_RUST_ARRAY = re.compile(
    r"pub const NON_HUMAN_TURN_MARKERS:\s*\[&str;\s*\d+\]\s*=\s*\[(?P<body>.*?)\];",
    re.S,
)


def rust_markers(source: str) -> list[str]:
    """Os marcadores como o EXECUTOR Rust os declara."""
    m = _RUST_ARRAY.search(source)
    assert m, "NON_HUMAN_TURN_MARKERS não encontrado em context_budget.rs"
    # Literais Rust simples (sem raw strings nem escapes exóticos neste array).
    return re.findall(r'"((?:[^"\\]|\\.)*)"', m.group("body"))


def python_markers(source: str) -> list[str]:
    """Os marcadores como o lado Python os declara, lidos por AST.

    AST e não regex: uma tupla reformatada por um linter quebraria um regex e o
    guard passaria a não guardar nada — a falha silenciosa que ele existe para
    impedir.
    """
    tree = ast.parse(source)
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(
            isinstance(t, ast.Name) and t.id == "NON_HUMAN_TURN_MARKERS" for t in node.targets
        ):
            value = ast.literal_eval(node.value)
            return list(value)
    raise AssertionError("NON_HUMAN_TURN_MARKERS não encontrado em loop_turn_record.py")


def test_rust_and_python_declare_the_same_markers():
    """A regra é uma só; duas cópias que divergem é o defeito, não a redundância."""
    assert RUST.exists(), f"fonte Rust ausente: {RUST}"
    assert PY_MIRROR.exists(), f"espelho Python ausente: {PY_MIRROR}"
    r = rust_markers(RUST.read_text(encoding="utf-8"))
    p = python_markers(PY_MIRROR.read_text(encoding="utf-8"))
    assert r == p, (
        "predicado de turno humano DIVERGIU entre executor e declaração\n"
        f"  rust  : {r}\n"
        f"  python: {p}"
    )


def test_the_markers_are_the_ones_actually_measured():
    """Os quatro medidos em 04/09/2026 — 35 de 172 records, 20,3%.

    Fixar a POPULAÇÃO impede que alguém esvazie a lista para fazer um número
    subir: com a lista vazia o predicado aceita tudo e o denominador volta a
    inflar, silenciosamente.
    """
    r = rust_markers(RUST.read_text(encoding="utf-8"))
    assert len(r) >= 4, f"a lista encolheu para {len(r)} — o denominador volta a inflar"
    for needed in ("<local-command-stdout>", "<local-command-caveat>"):
        assert any(needed in m for m in r), f"{needed} saiu da lista (13 ocorrências medidas)"
    assert any("continued from a previous conversation" in m for m in r), (
        "o resumo de compactação saiu da lista (9 ocorrências medidas)"
    )


def test_both_sides_agree_on_a_real_transcript_line():
    """Paridade de COMPORTAMENTO, não só de literais.

    Dois arquivos podem declarar a mesma lista e ainda assim aplicá-la de
    formas diferentes — a lição `teste-do-componente-nao-e-teste-do-caminho`.
    Aqui o predicado Python é executado de verdade contra as duas classes.
    """
    import importlib.util
    import sys

    # Um teste NÃO pode sujar a árvore que inspeciona. Sem isto o import
    # escreve `__pycache__/*.pyc` dentro de `client/`, e o guard do espelho
    # (`test_no_generated_artifact_sits_in_the_mirror`) reprova — como reprovou
    # na estreia deste arquivo, em 04/09/2026. O guard funcionou; o defeito era
    # meu.
    previous = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        spec = importlib.util.spec_from_file_location("_ltr", PY_MIRROR)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
    finally:
        sys.dont_write_bytecode = previous

    assert mod.is_human_turn_text("faça a auditoria") is True
    assert mod.is_human_turn_text("faça\n<system-reminder>x</system-reminder>") is True
    for marker in rust_markers(RUST.read_text(encoding="utf-8")):
        assert mod.is_human_turn_text(f"antes {marker} depois") is False, marker


if __name__ == "__main__":
    test_rust_and_python_declare_the_same_markers()
    test_the_markers_are_the_ones_actually_measured()
    test_both_sides_agree_on_a_real_transcript_line()
    print("OK — predicado de turno humano tem uma fonte só, em duas línguas")
