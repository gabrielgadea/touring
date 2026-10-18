"""Mutantes do adjudicador de rotulo (HO13): cada um aplicado a uma COPIA de `adjudica_rotulo.py` (o original nunca e escrito;
sha conferido antes e depois) e o `--selftest` da copia TEM de reprovar. Controle: a copia sem mutacao passa.
Cada mutante quebra UMA regra do pre-registro (autocontido / nome nunca ligado / execucao incondicional / lote); matar todos
prova que cada regra e exercida por um caso do selftest — nao que a regra e a certa (isso e do pre-registro).

Dois ramos do adjudicador sao deliberadamente SEM mutante: o desvio `construto_desconhecido` (todo caminho a ele ja e barrado
mais acima por uma regra que devolve motivo — equivalente) e o `Match` com campo != subject (os `cases` barram em `match_case`).

Uso: python3 -I mutantes_adjudica_rotulo.py <adjudica_rotulo.py> <saida.json>
Exit 0 so se todos morreram E o controle passou E o original nao mudou.
"""

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ADJ, SAIDA = Path(sys.argv[1]).resolve(), Path(sys.argv[2])

SETATTR = 'if not (isinstance(primeiro, ast.Name) and primeiro.id in ("self", "cls")):'
ANOT = 'return None if ANOTACAO_AVALIADA_NO_DEF and not futuras else "anotacao_adiada"'
MUTANTES = (
    # --- autocontido
    ("a1", "ignora import estrela", 'any(a.name == "*" for a in no.names)', "False"),
    ("a2", "ignora exec/eval/globals/locals", 'NOMES_DINAMICOS = frozenset({"exec", "eval", "globals", "locals", "vars", "__builtins__"})', "NOMES_DINAMICOS = frozenset()"),
    ("a3", "ignora __dict__/__builtins__", 'no.attr == "__dict__" or no.attr == "__builtins__"', "False"),
    ("a4", "ignora builtins.exec", 'no.value.id == "builtins"', "False"),
    ("a5", "ignora setattr de modulo", SETATTR, "if False:"),
    ("a6", "setattr em self tambem recusa", SETATTR, "if True:"),
    ("a7", "ignora sys.modules mutado", 'if isinstance(alvo, ast.Attribute) and alvo.attr == "modules":', "if False:"),
    ("a8", "ignora __getattr__ de modulo", 'return "getattr_de_modulo" if e_getattr_de_modulo(no, pais) else None', "return None"),
    # --- nome nunca ligado
    ("b1", "atribuicao nao liga", "[n.id] if isinstance(n.ctx, (ast.Store, ast.Del)) else []", "[]"),
    ("b2", "parametro nao liga", "(ast.arg, lambda n: [n.arg]),", "(ast.arg, lambda n: []),"),
    ("b3", "def/class nao ligam", "((ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef), lambda n: [n.name]),", "((ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef), lambda n: []),"),
    ("b4", "import nao liga", '(ast.alias, lambda n: [(n.asname or n.name).split(".")[0]]),', "(ast.alias, lambda n: []),"),
    ("b5", "import pontilhado liga o nome inteiro", '.split(".")[0]', ""),
    ("b6", "except-as nao liga", "(ast.ExceptHandler, lambda n: [n.name] if n.name else []),", "(ast.ExceptHandler, lambda n: []),"),
    ("b7", "global/nonlocal nao ligam", "((ast.Global, ast.Nonlocal), lambda n: list(n.names)),", "((ast.Global, ast.Nonlocal), lambda n: []),"),
    ("b8", "match capture nao liga", "((ast.MatchAs, ast.MatchStar), lambda n: [n.name] if n.name else []),", "((ast.MatchAs, ast.MatchStar), lambda n: []),"),
    ("b9", "builtin/implicito nao conta", "if nome in IMPLICITOS:", "if False:"),
    ("b10", "ligado em algum escopo nao conta", "if nome in nomes_ligados(arvore):", "if False:"),
    ("b11", "ignora a linha do achado", "(linha is None or n.lineno == linha)", "True"),
    # --- execucao incondicional
    ("c1", "corpo de funcao e incondicional", 'return "corpo_de_funcao"', "return None"),
    ("c2", "corpo de lambda e incondicional", 'return "corpo_de_lambda" if campo == "body" else None', "return None"),
    ("c3", "ramo do if e incondicional", 'return None if campo == "test" else "dentro_de_if"', "return None"),
    ("c4", "corpo do while e incondicional", 'return None if campo == "test" else "dentro_de_while"', "return None"),
    ("c5", "corpo do for e incondicional", 'return None if campo == "iter" else "dentro_de_for"', "return None"),
    ("c6", "try captura NameError mas segue", 'return "capturado_por_except" if captura_nameerror(pai) else None', "return None"),
    ("c7", "finally tratado como condicional", 'return None if campo == "finalbody" else "dentro_de_except_ou_else"', 'return "dentro_de_except_ou_else"'),
    ("c8", "handler/else do try e incondicional", 'return None if campo == "finalbody" else "dentro_de_except_ou_else"', "return None"),
    ("c9", "with suppress ignorado", 'return "with_suppress" if campo == "body" and com_suppress(pai) else None', "return None"),
    ("c10", "ramo do match e incondicional", 'return "dentro_de_match"', "return None"),
    ("c11", "curto-circuito do BoolOp ignorado", 'return None if idx == 0 else "curto_circuito"', "return None"),
    ("c12", "ramo do IfExp ignorado", 'return None if campo == "test" else "curto_circuito"', "return None"),
    ("c13", "comparacao encadeada ignorada", 'return None if campo == "left" or idx == 0 else "curto_circuito"', "return None"),
    ("c14", "compreensao: 2o iteravel e incondicional", 'return None if campo == "iter" and primeira else "dentro_de_compreensao"', "return None"),
    ("c15", "compreensao: elemento e incondicional", 'return None if campo == "generators" else "dentro_de_compreensao"', "return None"),
    ("c16", "mensagem do assert e incondicional", 'return None if campo == "test" else "dentro_de_assert_msg"', "return None"),
    ("c17", "future annotations ignorado", ANOT, "return None"),
    ("c18", "anotacao sempre adiada", ANOT, 'return "anotacao_adiada"'),
    ("c19", "detector de future sempre falso", "    return any(\n        isinstance(n, ast.ImportFrom) and n.module", "    return False and any(\n        isinstance(n, ast.ImportFrom) and n.module"),
    ("c20", "raise/return nao encerram o fluxo", "if isinstance(cmd, (ast.Raise, ast.Return, ast.Break, ast.Continue)):\n        return True", "if isinstance(cmd, (ast.Raise, ast.Return, ast.Break, ast.Continue)):\n        return False"),
    ("c21", "sys.exit nao encerra o fluxo", 'return (base == "" and nome in SAIDAS_BASE_VAZIA) or (base, nome) in SAIDAS_DE_MODULO', "return False"),
    ("c22", "while True nao encerra o fluxo", "return not any(isinstance(n, ast.Break) for n in ast.walk(cmd))", "return False"),
    ("c23", "codigo depois de saida e incondicional", "any(termina(c) for c in getattr(pai, campo)[:idx])", "False"),
    ("c24", "except bare nao captura", "if h.type is None:\n            return True", "if h.type is None:\n            return False"),
    ("c25", "except Exception nao captura", "if issubclass(NameError, classe):\n                return True", "if False:\n                return True"),
    ("c26", "except de nome desconhecido nao captura", "if not isinstance(classe, type):\n                return True", "if False:\n                return True"),
    ("c27", "sujeito do match deixa de ser seguro", "ast.withitem, ast.keyword, ast.expr, ast.Slice, ast.Match,", "ast.withitem, ast.keyword, ast.expr, ast.Slice,"),
    # --- lote
    ("d1", "lote: basta UM achado adjudicado", 'if all(v["veredito"] == "bug_latente" for v in por_achado):', 'if any(v["veredito"] == "bug_latente" for v in por_achado):'),
    ("d2", "lote: achado que nao e nome entra como nome", 'if a.get("classe") == "nome" else nao("classe_nao_e_nome")', "if True else nao('classe_nao_e_nome')"),
    ("d3", "lote: ignora a possivel truncagem em 3", 'if len(falso.get("achados", [])) >= 3:', "if False:"),
)


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def rodar(texto: str) -> dict:
    with tempfile.TemporaryDirectory() as tmp:
        copia = Path(tmp) / "adj_mut.py"
        copia.write_text(texto, encoding="utf-8")
        r = subprocess.run([sys.executable, "-I", "-B", str(copia), "--selftest"], capture_output=True, text=True, timeout=120, cwd="/tmp")
    falhas = [ln.strip() for ln in r.stdout.splitlines() if "FALHA" in ln][:2]
    return {"rc": r.returncode, "falhas": falhas, "traceback": "Traceback" in r.stderr, "reprovou": "selftest: REPROVOU" in r.stdout,
            "resumo": (r.stdout.strip().splitlines() or [""])[-1]}


def main() -> int:
    antes, texto = sha(ADJ), ADJ.read_text(encoding="utf-8")
    controle = rodar(texto)
    resultados = []
    for nome, descricao, velho, novo in MUTANTES:
        if texto.count(velho) != 1:
            resultados.append({"mutante": nome, "descricao": descricao, "erro": f"trecho aparece {texto.count(velho)}x (esperado 1x): nao aplicou"})
            continue
        r = rodar(texto.replace(velho, novo, 1))
        resultados.append({"mutante": nome, "descricao": descricao, **r, "morto": r["rc"] != 0 and (r["reprovou"] or r["traceback"])})
    mortos = sum(1 for r in resultados if r.get("morto"))
    saida = {"adjudicador": str(ADJ), "sha_antes": antes, "sha_depois": sha(ADJ), "controle_sem_mutacao": controle,
             "mutantes_mortos": f"{mortos}/{len(resultados)}", "mutantes": resultados}
    SAIDA.write_text(json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8")
    for r in resultados:
        if not r.get("morto"):
            print(f"  VIVO  {r['mutante']} {r['descricao']}: {r.get('erro') or r.get('resumo')}")
    ok_ctl = controle["rc"] == 0
    print(f"controle sem mutacao: {'passou' if ok_ctl else 'REPROVOU'} ({controle['resumo']}) | sha do original igual antes/depois: {saida['sha_antes'] == saida['sha_depois']}")
    print(f"{mortos}/{len(resultados)} mutantes mortos")
    return 0 if mortos == len(resultados) and ok_ctl and saida["sha_antes"] == saida["sha_depois"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
