#!/usr/bin/env python3
"""Adjudicação de RÓTULO dos falsos deny de NOME — a regra congelada do HO13 (10/10/2026).

O rótulo «negativo» do corpus significa «nenhum erro observado», não «correto em runtime». Um falso deny de NOME pode ser,
na verdade, um bug latente que o motor acertou (P0M, `b9d26a878f4c`: `srv.shutdown()` num `finally` com `srv` nunca ligado).
Este script decide, por código e sem opinião, quando isso é PROVÁVEL o bastante para ser reportado à parte. Qual contagem
decide o aceite (A: a bruta · B: a bruta menos as adjudicadas, com a bruta sempre ao lado) é decisão do Gabriel, não daqui.

Um falso deny de nome é «bug_latente» SOMENTE quando as três condições valem:
  1. o arquivo é AUTOCONTIDO: sem `from m import *`, `exec`/`eval`/`globals()`/`locals()`/`vars()`, `setattr` (salvo em
     `self`/`cls`), `.__dict__`, `__builtins__`, mutação de `sys.modules`, `__getattr__` de módulo;
  2. o nome NUNCA é ligado, em escopo nenhum do arquivo (atribuição, for, with/except-as, import, def/class, parâmetro,
     walrus, compreensão, `match`, `global`/`nonlocal`, `del`) e não é builtin nem nome implícito de módulo;
  3. o uso (na linha do achado, se dada) fica em código que executa INCONDICIONALMENTE: nível de módulo ou corpo de classe,
     `try` (corpo sem `except` que pegue NameError, ou `finally`), `with` (sem `suppress`), teste de `if`/`while`, iterável
     de `for`, e o que o `def` avalia na definição — decorador, default, anotação (esta, só sem `from __future__ import
     annotations`). Corpo de função/lambda, ramo de `if`/`for`/`while`/`match`/`except`, curto-circuito, compreensão e
     código depois de uma saída (`raise`, `return`, `sys.exit`…) NÃO são incondicionais.
Na dúvida NÃO adjudica: adjudicar errado é pior que não adjudicar. Toda recusa nomeia o motivo.

`ANOTACAO_AVALIADA_NO_DEF`: anotação de `def` é avaliada na definição até o Python 3.13 (o do projeto: 3.12); no 3.14 é
adiada. Se o código dos negativos rodar em ≥ 3.14, ponha False — o efeito é só adjudicar MENOS.

Uso:
  adjudica_rotulo.py --selftest
  adjudica_rotulo.py --snap <arquivo.py> --nome <nome> [--linha N]
  adjudica_rotulo.py --juiz-json <juiz.json> --corpus <dir> [--saida <out.json>]   (bruta ao lado da adjudicada, por balde)
Exit 0 = ok (selftest passou / execução concluída); 1 = selftest falhou.
"""

from __future__ import annotations

import argparse
import ast
import builtins
import hashlib
import importlib.util
import json
import sys
import tempfile
from collections import Counter
from pathlib import Path

ANOTACAO_AVALIADA_NO_DEF = True
NOMES_DINAMICOS = frozenset({"exec", "eval", "globals", "locals", "vars", "__builtins__"})
IMPLICITOS = frozenset(dir(builtins)) | {
    "__file__", "__name__", "__doc__", "__spec__", "__loader__", "__package__", "__path__", "__cached__",
    "__builtins__", "__annotations__", "__debug__", "__class__", "__qualname__", "__module__",
}
LISTAS_DE_COMANDOS = frozenset({"body", "orelse", "finalbody"})
SAIDAS_BASE_VAZIA = frozenset({"exit", "quit"})
SAIDAS_DE_MODULO = frozenset({("sys", "exit"), ("os", "_exit"), ("os", "abort")})


def nao(motivo: str) -> dict:
    return {"veredito": "nao_adjudica", "motivo": motivo}


# ----------------------------------------------------------------------------------------------- estrutura da árvore


def mapa_de_pais(arvore: ast.AST) -> dict[int, tuple[ast.AST, str, int | None]]:
    """id(filho) -> (pai, campo, índice na lista | None)."""
    mapa = {}
    for pai in ast.walk(arvore):
        for campo, valor in ast.iter_fields(pai):
            if isinstance(valor, list):
                for i, f in enumerate(valor):
                    if isinstance(f, ast.AST):
                        mapa[id(f)] = (pai, campo, i)
            elif isinstance(valor, ast.AST):
                mapa[id(valor)] = (pai, campo, None)
    return mapa


def e_getattr_de_modulo(no: ast.AST, pais: dict) -> bool:
    """`def __getattr__` no nível do módulo (dentro de if/try/with é módulo; dentro de classe/função não)."""
    if not (isinstance(no, (ast.FunctionDef, ast.AsyncFunctionDef)) and no.name == "__getattr__"):
        return False
    atual = no
    while id(atual) in pais:
        atual = pais[id(atual)][0]
        if isinstance(atual, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
            return False
    return True


def _import_estrela(no: ast.AST, pais: dict) -> str | None:
    if isinstance(no, ast.ImportFrom) and any(a.name == "*" for a in no.names):
        return "import_estrela"
    return None


def _nome_dinamico(no: ast.AST, pais: dict) -> str | None:
    return "exec_eval_globals_locals" if isinstance(no, ast.Name) and no.id in NOMES_DINAMICOS else None


def _atributo_dinamico(no: ast.AST, pais: dict) -> str | None:
    if not isinstance(no, ast.Attribute):
        return None
    if no.attr == "__dict__" or no.attr == "__builtins__":
        return "dict_ou_builtins_dinamico"
    if no.attr in ("exec", "eval") and isinstance(no.value, ast.Name) and no.value.id == "builtins":
        return "exec_eval_globals_locals"
    return None


def _setattr_dinamico(no: ast.AST, pais: dict) -> str | None:
    if isinstance(no, ast.Call) and isinstance(no.func, ast.Name) and no.func.id == "setattr":
        primeiro = no.args[0] if no.args else None
        if not (isinstance(primeiro, ast.Name) and primeiro.id in ("self", "cls")):
            return "setattr_dinamico"
    return None


def _sys_modules_mutado(no: ast.AST, pais: dict) -> str | None:
    if isinstance(no, ast.Subscript) and isinstance(no.ctx, (ast.Store, ast.Del)):
        alvo = no.value
        if isinstance(alvo, ast.Attribute) and alvo.attr == "modules":
            return "sys_modules_mutado"
    return None


def _getattr_de_modulo(no: ast.AST, pais: dict) -> str | None:
    return "getattr_de_modulo" if e_getattr_de_modulo(no, pais) else None


VERIFICACOES_DE_AUTOCONTENCAO = (_import_estrela, _nome_dinamico, _atributo_dinamico, _setattr_dinamico, _sys_modules_mutado, _getattr_de_modulo)


def dinamismo(arvore: ast.AST, pais: dict) -> str | None:
    """Motivo pelo qual o arquivo NÃO é autocontido (o primeiro achado, em ordem de nó), ou None."""
    for no in ast.walk(arvore):
        for verifica in VERIFICACOES_DE_AUTOCONTENCAO:
            if motivo := verifica(no, pais):
                return motivo
    return None


# (tipos de nó, o que ele liga): TODO identificador ligado em QUALQUER escopo do arquivo — mais forte que «visível no ponto do
# uso», de propósito.
LIGACOES = (
    (ast.Name, lambda n: [n.id] if isinstance(n.ctx, (ast.Store, ast.Del)) else []),
    (ast.arg, lambda n: [n.arg]),
    ((ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef), lambda n: [n.name]),
    (ast.alias, lambda n: [(n.asname or n.name).split(".")[0]]),
    (ast.ExceptHandler, lambda n: [n.name] if n.name else []),
    ((ast.Global, ast.Nonlocal), lambda n: list(n.names)),
    ((ast.MatchAs, ast.MatchStar), lambda n: [n.name] if n.name else []),
    (ast.MatchMapping, lambda n: [n.rest] if n.rest else []),
    ((ast.TypeVar, ast.ParamSpec, ast.TypeVarTuple), lambda n: [n.name]),
)


def nomes_ligados(arvore: ast.AST) -> set[str]:
    return {nome for no in ast.walk(arvore) for tipos, liga in LIGACOES if isinstance(no, tipos) for nome in liga(no)}


def tem_future_annotations(arvore: ast.Module) -> bool:
    return any(
        isinstance(n, ast.ImportFrom) and n.module == "__future__" and any(a.name == "annotations" for a in n.names)
        for n in arvore.body
    )


# ----------------------------------------------------------------------------------------------- incondicionalidade


def _nome_da_chamada(no: ast.expr) -> tuple[str, str]:
    """(base, nome) de `base.nome(...)` ou ('', nome) de `nome(...)`."""
    if isinstance(no, ast.Name):
        return "", no.id
    if isinstance(no, ast.Attribute):
        return (no.value.id if isinstance(no.value, ast.Name) else ""), no.attr
    return "", ""


def termina(cmd: ast.stmt) -> bool:
    """O comando encerra o fluxo (o que vem depois nele não executa)?"""
    if isinstance(cmd, (ast.Raise, ast.Return, ast.Break, ast.Continue)):
        return True
    if isinstance(cmd, ast.Expr) and isinstance(cmd.value, ast.Call):
        base, nome = _nome_da_chamada(cmd.value.func)
        return (base == "" and nome in SAIDAS_BASE_VAZIA) or (base, nome) in SAIDAS_DE_MODULO
    if isinstance(cmd, ast.While) and isinstance(cmd.test, ast.Constant) and cmd.test.value is True:
        return not any(isinstance(n, ast.Break) for n in ast.walk(cmd))
    if isinstance(cmd, ast.Assert) and isinstance(cmd.test, ast.Constant) and cmd.test.value is False:
        return True
    return False


def captura_nameerror(no_try: ast.Try) -> bool:
    """Algum `except` do `try` pega NameError? Na dúvida (nome que não é exceção builtin), sim."""
    for h in no_try.handlers:
        if h.type is None:
            return True
        tipos = h.type.elts if isinstance(h.type, ast.Tuple) else [h.type]
        for t in tipos:
            if isinstance(t, ast.Attribute):
                if t.attr in ("Exception", "BaseException", "NameError"):
                    return True
                continue
            if not isinstance(t, ast.Name):
                return True
            classe = getattr(builtins, t.id, None)
            if not isinstance(classe, type):
                return True
            if issubclass(NameError, classe):
                return True
    return False


def com_suppress(no_with: ast.With | ast.AsyncWith) -> bool:
    return any(isinstance(i.context_expr, ast.Call) and _nome_da_chamada(i.context_expr.func)[1] == "suppress" for i in no_with.items)


def anotacao(futuras: bool) -> str | None:
    """Anotação de `def`/variável de módulo: avaliada na definição (None = segue) ou adiada (motivo)."""
    return None if ANOTACAO_AVALIADA_NO_DEF and not futuras else "anotacao_adiada"


# Uma regra por tipo de PAI: `regra(pai, campo, idx, pais, futuras)` devolve None (o filho nesse campo executa sempre que o pai
# executa) ou o motivo da condicionalidade. Tipo sem regra e fora de SEGUROS = construto desconhecido = não adjudica.
REGRAS: dict[type, object] = {}


def regra(*tipos: type):
    def registra(f):
        REGRAS.update({t: f for t in tipos})
        return f

    return registra


@regra(ast.FunctionDef, ast.AsyncFunctionDef)
def _funcao(pai, campo, idx, pais, futuras):
    if campo == "body":
        return "corpo_de_funcao"
    if campo == "returns":
        return anotacao(futuras)
    return "type_params" if campo == "type_params" else None


@regra(ast.arg, ast.AnnAssign)
def _com_anotacao(pai, campo, idx, pais, futuras):
    return anotacao(futuras) if campo == "annotation" else None


@regra(ast.ClassDef)
def _classe(pai, campo, idx, pais, futuras):
    return "type_params" if campo == "type_params" else None


@regra(ast.TypeAlias)
def _alias_de_tipo(pai, campo, idx, pais, futuras):
    return "type_params"


@regra(ast.Lambda)
def _lambda(pai, campo, idx, pais, futuras):
    return "corpo_de_lambda" if campo == "body" else None


@regra(ast.If)
def _se(pai, campo, idx, pais, futuras):
    return None if campo == "test" else "dentro_de_if"


@regra(ast.While)
def _enquanto(pai, campo, idx, pais, futuras):
    return None if campo == "test" else "dentro_de_while"


@regra(ast.For, ast.AsyncFor)
def _para(pai, campo, idx, pais, futuras):
    return None if campo == "iter" else "dentro_de_for"


@regra(ast.Try, ast.TryStar)
def _tente(pai, campo, idx, pais, futuras):
    if campo == "body":
        return "capturado_por_except" if captura_nameerror(pai) else None
    return None if campo == "finalbody" else "dentro_de_except_ou_else"


@regra(ast.With, ast.AsyncWith)
def _com(pai, campo, idx, pais, futuras):
    return "with_suppress" if campo == "body" and com_suppress(pai) else None


@regra(ast.match_case)  # o `Match` em si só é alcançado pelo `subject`: os `cases` barram aqui
def _caso(pai, campo, idx, pais, futuras):
    return "dentro_de_match"


@regra(ast.BoolOp)
def _e_ou(pai, campo, idx, pais, futuras):
    return None if idx == 0 else "curto_circuito"


@regra(ast.IfExp)
def _se_expressao(pai, campo, idx, pais, futuras):
    return None if campo == "test" else "curto_circuito"


@regra(ast.Compare)
def _comparacao(pai, campo, idx, pais, futuras):
    return None if campo == "left" or idx == 0 else "curto_circuito"


@regra(ast.comprehension)
def _gerador(pai, campo, idx, pais, futuras):
    primeira = pais[id(pai)][2] == 0
    return None if campo == "iter" and primeira else "dentro_de_compreensao"


@regra(ast.ListComp, ast.SetComp, ast.GeneratorExp, ast.DictComp)
def _compreensao(pai, campo, idx, pais, futuras):
    return None if campo == "generators" else "dentro_de_compreensao"


@regra(ast.Assert)
def _afirma(pai, campo, idx, pais, futuras):
    return None if campo == "test" else "dentro_de_assert_msg"


# Passagem direta: não condicionam nada por si. `ExceptHandler` passa porque o `Try` (campo `handlers`) é quem barra.
SEGUROS = (
    ast.Module, ast.arguments, ast.Expr, ast.Assign, ast.AugAssign, ast.Delete, ast.Raise, ast.Return,
    ast.withitem, ast.keyword, ast.expr, ast.Slice, ast.Match, ast.ExceptHandler,
)


def _passa(pai: ast.AST, campo: str, idx: int | None, pais: dict, futuras: bool) -> str | None:
    """None = o caminho de um filho (em `campo`) até o `pai` é incondicional; senão o motivo."""
    for tipo in type(pai).__mro__:
        if tipo in REGRAS:
            return REGRAS[tipo](pai, campo, idx, pais, futuras)
    return None if isinstance(pai, SEGUROS) else "construto_desconhecido"


def incondicional(no: ast.AST, pais: dict, futuras: bool) -> tuple[bool, str]:
    atual = no
    while id(atual) in pais:
        pai, campo, idx = pais[id(atual)]
        motivo = _passa(pai, campo, idx, pais, futuras)
        if motivo is not None:
            return False, motivo
        if idx is not None and campo in LISTAS_DE_COMANDOS and any(termina(c) for c in getattr(pai, campo)[:idx]):
            return False, "depois_de_saida"
        atual = pai
    return True, "ok"


# ----------------------------------------------------------------------------------------------- o veredito


def adjudica(texto: str, nome: str, linha: int | None = None) -> dict:
    """{veredito: 'bug_latente' | 'nao_adjudica', motivo, linhas?}."""
    try:
        arvore = ast.parse(texto)
    except (SyntaxError, ValueError, RecursionError):
        return nao("nao_compila")
    pais = mapa_de_pais(arvore)
    if (dyn := dinamismo(arvore, pais)) is not None:
        return nao(dyn)
    if nome in IMPLICITOS:
        return nao("builtin_ou_implicito")
    if nome in nomes_ligados(arvore):
        return nao("ligado_em_algum_escopo")
    usos = [n for n in ast.walk(arvore) if isinstance(n, ast.Name) and n.id == nome and isinstance(n.ctx, ast.Load) and (linha is None or n.lineno == linha)]
    if not usos:
        return nao("sem_uso_na_linha" if linha is not None else "sem_uso")
    futuras = tem_future_annotations(arvore)
    veredictos = [incondicional(n, pais, futuras) for n in usos]
    linhas = sorted({n.lineno for n, (ok, _) in zip(usos, veredictos, strict=True) if ok})
    if linhas:
        return {"veredito": "bug_latente", "motivo": "nome_nunca_ligado_em_uso_incondicional", "linhas": linhas}
    return nao(Counter(m for _, m in veredictos).most_common(1)[0][0])


# ----------------------------------------------------------------------------------------------- lote (juiz + corpus)


def entradas_do_corpus(corpus: Path) -> dict[str, dict]:
    por_id = {}
    for arq in ("entradas.jsonl", "entradas_inline_neg.jsonl"):
        f = corpus / arq
        if f.is_file():
            for ln in f.read_text(encoding="utf-8").splitlines():
                if ln.strip():
                    e = json.loads(ln)
                    por_id[e["id"]] = e
    return por_id


def adjudica_falso(falso: dict, entrada: dict | None, corpus: Path) -> dict:
    """Um falso deny é adjudicado só se TODOS os seus achados deny são de nome e adjudicam `bug_latente` — e se a lista de
    achados do juiz (cortada em 3) não pode estar truncada."""
    if entrada is None:
        return nao("id_fora_do_corpus")
    achados = [a for a in falso.get("achados", []) if a.get("severidade") == "deny"]
    if not achados:
        return nao("sem_achado_deny")
    if len(falso.get("achados", [])) >= 3:
        return nao("achados_possivelmente_truncados")
    texto = (corpus / entrada["snap"]).read_text(encoding="utf-8", errors="ignore")
    por_achado = [adjudica(texto, a.get("nome", ""), a.get("linha")) if a.get("classe") == "nome" else nao("classe_nao_e_nome") for a in achados]
    if all(v["veredito"] == "bug_latente" for v in por_achado):
        return {"veredito": "bug_latente", "motivo": "todos_os_achados_sao_nome_nunca_ligado", "achados": por_achado}
    return {**nao(next(v["motivo"] for v in por_achado if v["veredito"] != "bug_latente")), "achados": por_achado}


def carrega_limite():
    """`limite_do_balde` do juiz irmão, se estiver ao lado (só para imprimir o CP95 das duas contagens)."""
    f = Path(__file__).resolve().parent / "juiz_motor_vgp.py"
    if not f.is_file():
        return None
    spec = importlib.util.spec_from_file_location("juiz_irmao", f)
    juiz = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(juiz)
    return juiz.limite_do_balde


def adjudica_lote(juiz_json: dict, corpus: Path) -> dict:
    entradas = entradas_do_corpus(corpus)
    por_id = {f["id"]: adjudica_falso(f, entradas.get(f["id"]), corpus) for f in juiz_json.get("falsos", [])}
    limite = carrega_limite()
    por_balde = {}
    for balde, n in juiz_json.get("negativos", {}).items():
        ids = [f["id"] for f in juiz_json.get("falsos", []) if f["balde"] == balde]
        adj = [i for i in ids if por_id[i]["veredito"] == "bug_latente"]
        linha = {"n": n, "falsos_brutos": len(ids), "adjudicados_bug_latente": len(adj), "falsos_apos_adjudicacao": len(ids) - len(adj),
                 "ids_adjudicados": adj, "ids_mantidos": {i: por_id[i]["motivo"] for i in ids if i not in adj}}
        if limite and n:
            linha["limite_superior_95_bruta"] = limite(len(ids), n)
            linha["limite_superior_95_adjudicada"] = limite(len(ids) - len(adj), n)
        por_balde[balde] = linha
    return {"adjudicador_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "por_balde": por_balde, "por_id": por_id}


# ----------------------------------------------------------------------------------------------- selftest

B, N = "bug_latente", "nao_adjudica"
CASOS = (
    # (nome do caso, texto, nome, linha, esperado, trecho do motivo)
    # --- bug latente: o padrão `finally` e os usos incondicionais
    ("finally com nome nunca ligado (o caso P0M)", "import http.server\ntry:\n    print('servindo')\nfinally:\n    srv.shutdown()\n", "srv", 5, B, ""),
    ("uso direto no nível do módulo", "print(x)\n", "x", 1, B, ""),
    ("corpo de classe", "class A:\n    y = naoligado\n", "naoligado", 2, B, ""),
    ("decorador", "@naoligado\ndef f():\n    pass\n", "naoligado", 1, B, ""),
    ("default de parâmetro", "def f(a=naoligado):\n    pass\n", "naoligado", 1, B, ""),
    ("anotação de parâmetro sem future", "def f(a: Tipo):\n    pass\n", "Tipo", 1, B, ""),
    ("anotação de retorno sem future", "def f() -> Tipo:\n    pass\n", "Tipo", 1, B, ""),
    ("anotação de variável de módulo sem future", "x: Tipo = 1\n", "Tipo", 1, B, ""),
    ("corpo do try com except que não pega NameError", "try:\n    naoligado()\nexcept ValueError:\n    pass\n", "naoligado", 2, B, ""),
    ("teste do if", "if naoligado:\n    pass\n", "naoligado", 1, B, ""),
    ("iterável do for", "for i in naoligado:\n    pass\n", "naoligado", 1, B, ""),
    ("primeiro operando de or", "x = naoligado or 1\n", "naoligado", 1, B, ""),
    ("corpo do with", "import os\nwith open('a') as f:\n    naoligado()\n", "naoligado", 3, B, ""),
    ("primeiro iterável da compreensão", "x = [i for i in naoligado]\n", "naoligado", 1, B, ""),
    ("sem linha dada, acha o uso", "print(x)\n", "x", None, B, ""),
    ("finally dentro de with", "with open('a') as f:\n    try:\n        pass\n    finally:\n        srv.shutdown()\n", "srv", 5, B, ""),
    # --- controles: autocontido
    ("import estrela", "from m import *\nprint(srv)\n", "srv", 2, N, "import_estrela"),
    ("exec", "exec('srv = 1')\nprint(srv)\n", "srv", 2, N, "exec_eval_globals_locals"),
    ("eval", "eval('1')\nprint(srv)\n", "srv", 2, N, "exec_eval_globals_locals"),
    ("globals()", "globals()['srv'] = 1\nprint(srv)\n", "srv", 2, N, "exec_eval_globals_locals"),
    ("locals()", "locals()['srv'] = 1\nprint(srv)\n", "srv", 2, N, "exec_eval_globals_locals"),
    ("setattr de módulo", "import sys\nsetattr(sys.modules[__name__], 'srv', 1)\nprint(srv)\n", "srv", 3, N, "setattr_dinamico"),
    ("__getattr__ de módulo", "def __getattr__(n):\n    return 1\nprint(srv)\n", "srv", 3, N, "getattr_de_modulo"),
    ("__dict__", "import m\nm.__dict__['srv'] = 1\nprint(srv)\n", "srv", 3, N, "dict_ou_builtins_dinamico"),
    ("sys.modules mutado", "import sys\nsys.modules['x'] = 1\nprint(srv)\n", "srv", 3, N, "sys_modules_mutado"),
    ("builtins.exec", "import builtins\nbuiltins.exec('1')\nprint(srv)\n", "srv", 3, N, "exec_eval_globals_locals"),
    ("setattr em self é inofensivo", "class A:\n    def __init__(self):\n        setattr(self, 'a', 1)\nprint(srv)\n", "srv", 4, B, ""),
    ("model.eval() não é eval", "model.eval()\nprint(srv)\n", "srv", 2, B, ""),
    # --- controles: o nome é ligado em algum escopo
    ("ligado em função", "def g():\n    srv = 1\nprint(srv)\n", "srv", 3, N, "ligado_em_algum_escopo"),
    ("atribuição", "srv = 1\nprint(srv)\n", "srv", 2, N, "ligado_em_algum_escopo"),
    ("import", "import srv\nprint(srv)\n", "srv", 2, N, "ligado_em_algum_escopo"),
    ("import pontilhado liga a raiz", "import a.srv\nprint(a)\n", "a", 2, N, "ligado_em_algum_escopo"),
    ("from-import com as", "from m import x as srv\nprint(srv)\n", "srv", 2, N, "ligado_em_algum_escopo"),
    ("parâmetro", "def f(srv):\n    pass\nprint(srv)\n", "srv", 3, N, "ligado_em_algum_escopo"),
    ("for", "for srv in []:\n    pass\nprint(srv)\n", "srv", 3, N, "ligado_em_algum_escopo"),
    ("with as", "with open('a') as srv:\n    pass\nprint(srv)\n", "srv", 3, N, "ligado_em_algum_escopo"),
    ("except as", "try:\n    pass\nexcept Exception as srv:\n    pass\nprint(srv)\n", "srv", 5, N, "ligado_em_algum_escopo"),
    ("walrus", "if (srv := 1):\n    pass\nprint(srv)\n", "srv", 3, N, "ligado_em_algum_escopo"),
    ("compreensão", "x = [srv for srv in []]\nprint(srv)\n", "srv", 2, N, "ligado_em_algum_escopo"),
    ("def", "def srv():\n    pass\nprint(srv)\n", "srv", 3, N, "ligado_em_algum_escopo"),
    ("class", "class srv:\n    pass\nprint(srv)\n", "srv", 3, N, "ligado_em_algum_escopo"),
    ("global declarado", "def f():\n    global srv\nprint(srv)\n", "srv", 3, N, "ligado_em_algum_escopo"),
    ("match capture", "match 1:\n    case srv:\n        pass\nprint(srv)\n", "srv", 4, N, "ligado_em_algum_escopo"),
    ("augassign", "srv += 1\n", "srv", None, N, "ligado_em_algum_escopo"),
    ("builtin", "print(len)\n", "len", 1, N, "builtin_ou_implicito"),
    ("__file__ implícito", "print(__file__)\n", "__file__", 1, N, "builtin_ou_implicito"),
    # --- controles: execução condicional
    ("função não chamada", "def f():\n    print(srv)\n", "srv", 2, N, "corpo_de_funcao"),
    ("função chamada (regra literal: corpo de função nunca)", "def f():\n    print(srv)\nf()\n", "srv", 2, N, "corpo_de_funcao"),
    ("lambda", "f = lambda: srv\n", "srv", 1, N, "corpo_de_lambda"),
    ("ramo do if", "if cond:\n    print(srv)\n", "srv", 2, N, "dentro_de_if"),
    ("guarda __main__", "if __name__ == '__main__':\n    print(srv)\n", "srv", 2, N, "dentro_de_if"),
    ("else do if", "if cond:\n    pass\nelse:\n    print(srv)\n", "srv", 4, N, "dentro_de_if"),
    ("corpo do for", "for i in []:\n    print(srv)\n", "srv", 2, N, "dentro_de_for"),
    ("corpo do while", "while cond:\n    print(srv)\n", "srv", 2, N, "dentro_de_while"),
    ("handler do except", "try:\n    pass\nexcept ValueError:\n    print(srv)\n", "srv", 4, N, "dentro_de_except_ou_else"),
    ("else do try", "try:\n    pass\nexcept ValueError:\n    pass\nelse:\n    print(srv)\n", "srv", 6, N, "dentro_de_except_ou_else"),
    ("try com except Exception", "try:\n    print(srv)\nexcept Exception:\n    pass\n", "srv", 2, N, "capturado_por_except"),
    ("try com except bare", "try:\n    print(srv)\nexcept:\n    pass\n", "srv", 2, N, "capturado_por_except"),
    ("try com except NameError", "try:\n    print(srv)\nexcept NameError:\n    pass\n", "srv", 2, N, "capturado_por_except"),
    ("try com tupla que inclui Exception", "try:\n    print(srv)\nexcept (OSError, Exception):\n    pass\n", "srv", 2, N, "capturado_por_except"),
    ("try com except de nome desconhecido", "try:\n    print(srv)\nexcept erro_qualquer:\n    pass\n", "srv", 2, N, "capturado_por_except"),
    ("finally de try dentro de função", "def f():\n    try:\n        pass\n    finally:\n        srv.shutdown()\n", "srv", 5, N, "corpo_de_funcao"),
    ("segundo operando de and", "x = cond and srv\n", "srv", 1, N, "curto_circuito"),
    ("ramo do ifexp", "x = 1 if cond else srv\n", "srv", 1, N, "curto_circuito"),
    ("comparação encadeada: terceiro operando", "x = 1 < a < srv\n", "srv", 1, N, "curto_circuito"),
    ("comparação: primeiro operando", "x = 1 < srv\n", "srv", 1, B, ""),
    ("teste do assert", "assert srv\n", "srv", 1, B, ""),
    ("mensagem do assert", "assert ok, srv\n", "srv", 1, N, "dentro_de_assert_msg"),
    ("elemento da compreensão", "x = [srv for i in []]\n", "srv", 1, N, "dentro_de_compreensao"),
    ("segundo iterável da compreensão", "x = [i for i in [] for j in srv]\n", "srv", 1, N, "dentro_de_compreensao"),
    ("depois de sys.exit", "import sys\nsys.exit(0)\nprint(srv)\n", "srv", 3, N, "depois_de_saida"),
    ("depois de raise", "raise SystemExit\nprint(srv)\n", "srv", 2, N, "depois_de_saida"),
    ("depois de while True sem break", "while True:\n    pass\nprint(srv)\n", "srv", 3, N, "depois_de_saida"),
    ("with suppress", "from contextlib import suppress\nwith suppress(NameError):\n    print(srv)\n", "srv", 3, N, "with_suppress"),
    ("anotação com future adia", "from __future__ import annotations\ndef f(a: Tipo):\n    pass\n", "Tipo", 2, N, "anotacao_adiada"),
    ("anotação de variável com future", "from __future__ import annotations\nx: Tipo = 1\n", "Tipo", 2, N, "anotacao_adiada"),
    ("corpo de método", "class A:\n    def m(self):\n        return srv\n", "srv", 3, N, "corpo_de_funcao"),
    ("match: ramo", "match 1:\n    case 1:\n        print(srv)\n", "srv", 3, N, "dentro_de_match"),
    ("match: sujeito", "match srv:\n    case _:\n        pass\n", "srv", 1, B, ""),
    # --- linha, sintaxe, ausência
    ("linha que não tem o nome", "print(1)\nprint(srv)\n", "srv", 1, N, "sem_uso_na_linha"),
    ("nome ausente", "print(1)\n", "srv", None, N, "sem_uso"),
    ("não compila", "def (\n", "srv", 1, N, "nao_compila"),
    ("só a linha pedida decide", "def f():\n    print(srv)\nprint(srv)\n", "srv", 2, N, "corpo_de_funcao"),
    ("uso incondicional em outra linha da mesma", "def f():\n    print(srv)\nprint(srv)\n", "srv", 3, B, ""),
)


def _selftest_lote() -> list[str]:
    """O lote: só vira «adjudicado» o falso cujos achados deny SÃO todos nome adjudicados; truncado e id fora do corpus não."""
    falhas = []
    with tempfile.TemporaryDirectory() as tmp:
        corpus = Path(tmp)
        (corpus / "snap").mkdir()
        textos = {"a": "try:\n    pass\nfinally:\n    srv.shutdown()\n", "b": "print(x)\n", "c": "print(srv)\n", "d": "if c:\n    print(srv)\n", "e": "print(srv)\nprint(srv2)\nprint(srv3)\n"}
        linhas = []
        for i, t in textos.items():
            (corpus / "snap" / f"{i}.py").write_text(t)
            linhas.append(json.dumps({"id": i, "rotulo": "negativo", "origem": "atribuido", "snap": f"snap/{i}.py"}))
        (corpus / "entradas.jsonl").write_text("\n".join(linhas) + "\n")
        nome = lambda n, l: {"classe": "nome", "nome": n, "linha": l, "severidade": "deny"}  # noqa: E731
        falsos = [
            {"id": "a", "balde": "arquivo", "path": "/p/a.py", "achados": [nome("srv", 4)]},
            {"id": "b", "balde": "arquivo", "path": "/p/b.py", "achados": [nome("x", 1), {"classe": "atributo", "nome": "x", "linha": 1, "severidade": "deny"}]},
            {"id": "c", "balde": "inline", "path": "<stdin>", "achados": [nome("srv", 1)]},
            {"id": "d", "balde": "inline", "path": "<stdin>", "achados": [nome("srv", 2)]},
            {"id": "e", "balde": "inline", "path": "<stdin>", "achados": [nome("srv", 1), nome("srv2", 2), nome("srv3", 3)]},
            {"id": "z", "balde": "inline", "path": "<stdin>", "achados": [nome("srv", 1)]},
        ]
        lote = adjudica_lote({"falsos": falsos, "negativos": {"arquivo": 10, "inline": 20}}, corpus)
    esperado = {"a": B, "b": N, "c": B, "d": N, "e": N, "z": N}
    falhas += [f"lote: {i} = {lote['por_id'][i]['veredito']} ≠ {v}" for i, v in esperado.items() if lote["por_id"][i]["veredito"] != v]
    if lote["por_id"]["e"]["motivo"] != "achados_possivelmente_truncados" or lote["por_id"]["z"]["motivo"] != "id_fora_do_corpus":
        falhas.append("lote: motivo de truncado/fora do corpus")
    pb = lote["por_balde"]
    if (pb["arquivo"]["falsos_brutos"], pb["arquivo"]["falsos_apos_adjudicacao"]) != (2, 1):
        falhas.append(f"lote: balde arquivo {pb['arquivo']}")
    if (pb["inline"]["falsos_brutos"], pb["inline"]["falsos_apos_adjudicacao"]) != (4, 3):
        falhas.append(f"lote: balde inline {pb['inline']}")
    return falhas


def selftest() -> int:
    falhas = []
    for caso, texto, nome, linha, esperado, trecho in CASOS:
        r = adjudica(texto, nome, linha)
        if r["veredito"] != esperado or trecho not in r["motivo"]:
            falhas.append(f"{caso}: {r['veredito']}/{r['motivo']} ≠ {esperado}/{trecho or '*'}")
    falhas += _selftest_lote()
    for f in falhas:
        print(f"  FALHA {f}")
    ok = not falhas
    print(f"selftest: {f'{len(CASOS)}/{len(CASOS)} casos + lote' if ok else f'REPROVOU ({len(falhas)} falhas)'}")
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--snap", type=Path)
    ap.add_argument("--nome")
    ap.add_argument("--linha", type=int)
    ap.add_argument("--juiz-json", type=Path)
    ap.add_argument("--corpus", type=Path)
    ap.add_argument("--saida", type=Path)
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    if a.snap and a.nome:
        print(json.dumps(adjudica(a.snap.read_text(encoding="utf-8", errors="ignore"), a.nome, a.linha), ensure_ascii=False))
        return 0
    if a.juiz_json and a.corpus:
        lote = adjudica_lote(json.loads(a.juiz_json.read_text(encoding="utf-8")), a.corpus)
        if a.saida:
            a.saida.write_text(json.dumps(lote, ensure_ascii=False, indent=1), encoding="utf-8")
        for balde, t in lote["por_balde"].items():
            lim = f" · CP95 bruta {t['limite_superior_95_bruta']:.2%} | adjudicada {t['limite_superior_95_adjudicada']:.2%}" if "limite_superior_95_bruta" in t else ""
            print(f"{balde}: bruta {t['falsos_brutos']}/{t['n']} | adjudicada {t['falsos_apos_adjudicacao']}/{t['n']} ({t['adjudicados_bug_latente']} bug latente){lim}")
        return 0
    ap.print_usage(sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
