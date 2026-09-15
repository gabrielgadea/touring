"""scan_debt.py honra a própria docstring: supressão só conta quando é COMENTÁRIO de verdade.

Achado do cross-audit de 05/09/2026 (projeto analise): `test_censo_pyo3.py` planta, numa
fixture de LISTA DE STRINGS, linhas como ``"import x  # type: ignore[import-untyped]"`` para
provar que o regex de import do censo tolera comentário no fim — e o scanner contava as duas
strings como supressões vivas. A docstring diz «never inside a string literal»; o padrão
``suppression`` era buscado na linha inteira. O tokenizador do Python decide.

Roda com: python3 -m pytest ~/.claude/skills/TACO-cross-audit/scripts/test_scan_debt.py -q
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import scan_debt  # noqa: E402


def _hits(tmp_path: Path, nome: str, texto: str) -> list[dict]:
    p = tmp_path / nome
    p.write_text(texto, encoding="utf-8")
    return scan_debt.scan_file(p)


def _supressoes(hits: list[dict]) -> list[int]:
    return [h["line"] for h in hits if h["category"] == "suppression"]


def test_comentario_real_conta(tmp_path: Path) -> None:
    """Controle positivo: o `# type: ignore` de verdade continua sendo dívida."""
    hits = _hits(tmp_path, "a.py", "import foo  # type: ignore[import-untyped]\nx = 1\n")
    assert _supressoes(hits) == [1]


def test_string_literal_nao_conta(tmp_path: Path) -> None:
    """O caso da fixture: a linha inteira é uma STRING dentro de uma lista."""
    texto = (
        "LINHAS = [\n"
        '    "            import kazuba_rust_core as krc  # type: ignore[import-untyped]",\n'
        '    "            from kazuba_rust_core import AkomaParser  # type: ignore[import-untyped]",\n'
        "]\n"
    )
    hits = _hits(tmp_path, "fixture.py", texto)
    assert _supressoes(hits) == []


def test_string_e_comentario_na_mesma_linha(tmp_path: Path) -> None:
    """Só o `#` fora da string é comentário; o tokenizador separa os dois."""
    texto = 's = "x  # type: ignore"  # type: ignore[assignment]\n'
    hits = _hits(tmp_path, "misto.py", texto)
    assert _supressoes(hits) == [1]


def test_rust_allow_continua_na_linha_inteira(tmp_path: Path) -> None:
    """Fora do Python não há tokenizador: o atributo Rust segue contado onde estiver."""
    hits = _hits(tmp_path, "m.rs", "#[allow(dead_code)]\nfn f() {}\n")
    assert _supressoes(hits) == [1]


def test_python_que_nao_tokeniza_cai_no_textual(tmp_path: Path) -> None:
    """Fail-open DECLARADO: sem tokenização possível, vale a leitura textual antiga.

    O que derruba o tokenizador é a string tripla nunca fechada (TokenError «EOF in
    multi-line string») — e também o parêntese nunca fechado («EOF in multi-line
    statement»), premissa que a primeira versão deste teste tinha errada. O controle
    confirma que a via de fallback é a que responde: `python_comments` devolve None.
    """
    texto = 'y = 2  # type: ignore\ns = """abre e nunca fecha\n'
    assert scan_debt.python_comments(texto) is None
    hits = _hits(tmp_path, "quebrado.py", texto)
    assert _supressoes(hits) == [1]


def test_erro_de_parser_ainda_tokeniza(tmp_path: Path) -> None:
    """Controle do controle: erro que SÓ o parser vê (def sem parênteses) segue pela via tokenizada."""
    texto = "def quebrado:\n    y = 2  # type: ignore\n"
    assert scan_debt.python_comments(texto) == {2: "# type: ignore"}
    hits = _hits(tmp_path, "parser.py", texto)
    assert _supressoes(hits) == [2]
