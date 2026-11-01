"""Classifica os falsos deny do juiz VGP pelo construto que DEFINE o nome no próprio snap.

Para cada negativo com achado `deny`: `nome` → como o nome está ligado no arquivo (except-as, with-as, for,
compreensão, parâmetro, walrus, import, def/class, atribuição, builtin…) por predicado positivo sobre o `ast`;
o que não casa nenhum predicado fica "não ligado no arquivo", visível. `atributo`/`import` → nome e linha.
Uso: python3 classificar_falsos_vgp.py <juiz.json>
"""

from __future__ import annotations

import ast
import builtins
import collections
import json
import sys
from pathlib import Path

CORPUS = Path.home() / ".claude/touring/vgp/corpus_v0d"


def entradas() -> dict[str, dict]:
    out = {}
    for nome in ("entradas.jsonl", "entradas_inline_neg.jsonl"):
        p = CORPUS / nome
        if p.is_file():
            for linha in p.read_text().splitlines():
                if linha.strip():
                    e = json.loads(linha)
                    out[e["id"]] = e
    return out


def _alvos(no: ast.AST) -> set[str]:
    return {n.id for n in ast.walk(no) if isinstance(n, ast.Name)}


def _ligacao_do_no(no: ast.AST, nome: str) -> str | None:  # noqa: C901 — tabela de predicados, um por construto
    if isinstance(no, ast.ExceptHandler) and no.name == nome:
        return "except-as"
    if isinstance(no, ast.withitem) and no.optional_vars is not None and nome in _alvos(no.optional_vars):
        return "with-as"
    if isinstance(no, (ast.For, ast.AsyncFor)) and nome in _alvos(no.target):
        return "for-alvo"
    if isinstance(no, ast.comprehension) and nome in _alvos(no.target):
        return "compreensao-alvo"
    if isinstance(no, ast.arg) and no.arg == nome:
        return "parametro"
    if isinstance(no, ast.NamedExpr) and nome in _alvos(no.target):
        return "walrus"
    if isinstance(no, (ast.Import, ast.ImportFrom)) and any((a.asname or a.name.split(".")[0]) == nome for a in no.names):
        return "import"
    if isinstance(no, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) and no.name == nome:
        return "def/class"
    if isinstance(no, (ast.Global, ast.Nonlocal)) and nome in no.names:
        return "global/nonlocal"
    if isinstance(no, (ast.MatchAs, ast.MatchStar)) and no.name == nome:
        return "match-captura"
    if isinstance(no, ast.Assign) and any(nome in _alvos(t) for t in no.targets):
        return "atribuicao"
    if isinstance(no, (ast.AnnAssign, ast.AugAssign)) and nome in _alvos(no.target):
        return "atribuicao-anotada/aug"
    return None


def ligacao(arvore: ast.AST, nome: str) -> str:
    achadas = {lig for no in ast.walk(arvore) if (lig := _ligacao_do_no(no, nome))}
    if achadas:
        return "+".join(sorted(achadas))
    if hasattr(builtins, nome):
        return "builtin"
    if any(isinstance(n, ast.ImportFrom) and any(a.name == "*" for a in n.names) for n in ast.walk(arvore)):
        return "star-import"
    return "nao-ligado-no-arquivo"


def main() -> int:
    juiz = json.loads(Path(sys.argv[1]).read_text())
    ent = entradas()
    classes: collections.Counter = collections.Counter()
    exemplos: dict = {}
    for f in juiz["falsos"]:
        e = ent.get(f["id"])
        if e is None:
            classes[(f["balde"], "?", "entrada-nao-achada")] += 1
            continue
        texto = (CORPUS / e["snap"]).read_text(errors="replace")
        try:
            arvore = ast.parse(texto)
        except SyntaxError:
            classes[(f["balde"], "?", "snap-nao-compila")] += 1
            continue
        linhas = texto.splitlines()
        for a in f["achados"]:
            if a["severidade"] != "deny":
                continue
            lig = ligacao(arvore, a["nome"]) if a["classe"] == "nome" else "-"
            chave = (f["balde"], a["classe"], lig)
            classes[chave] += 1
            ln = a["linha"]
            trecho = linhas[ln - 1].strip()[:100] if 0 < ln <= len(linhas) else ""
            exemplos.setdefault(chave, []).append(f"{f['id']}:{ln} `{a['nome']}` | {trecho}")
    for chave, n in classes.most_common():
        print(f"{n:4}  {chave}")
        for x in exemplos.get(chave, [])[:3]:
            print(f"        {x}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
