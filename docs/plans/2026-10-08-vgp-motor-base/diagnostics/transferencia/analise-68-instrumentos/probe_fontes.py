"""Sonda das FONTES do corpus semeado — antes de escrever qualquer gerador.

Mede, sem suposição: o que existe em disco, o que compila limpo, e o que o classificador
do juiz faz com a mensagem REAL de cada linguagem (importado, nunca editado).
"""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"))
from juiz_motor_vgp import classe_erro  # noqa: E402  (import, não edição)

ANALISE = Path.home() / "projects/analise"
FRONTEND = ANALISE / "frontend"
PORTAL = Path.home() / "projects/konverter/portal"


def conta(raiz: Path, exts: tuple[str, ...]) -> tuple[int, int]:
    """(quantos, bytes) fora de node_modules/dist/.git."""
    n = b = 0
    for e in exts:
        for p in raiz.rglob(f"*{e}"):
            partes = p.parts
            if any(x in partes for x in ("node_modules", ".git", "dist", ".next", "build")):
                continue
            if not p.is_file():
                continue
            n += 1
            b += p.stat().st_size
    return n, b


def tsc(root: Path) -> dict:
    """A linha de base do projeto: `tsc --noEmit -p .` com o tsconfig DELE."""
    binario = root / "node_modules/.bin/tsc"
    if not binario.is_file():
        return {"erro": f"sem tsc em {binario}"}
    ini = time.time()
    r = subprocess.run(
        [str(binario), "--noEmit", "-p", "."],
        cwd=root,
        capture_output=True,
        text=True,
        timeout=600,
    )
    dt = time.time() - ini
    linhas = [x for x in (r.stdout or "").splitlines() if "error TS" in x]
    return {
        "rc": r.returncode,
        "erros": len(linhas),
        "segundos": round(dt, 1),
        "primeiras": linhas[:3],
        "tsconfig": (root / "tsconfig.json").is_file(),
        "node": subprocess.run(["node", "--version"], capture_output=True, text=True).stdout.strip(),
    }


saida = {}
for nome, raiz, exts in [
    ("frontend", FRONTEND, (".tsx", ".ts")),
    ("portal", PORTAL, (".tsx", ".ts")),
    ("analise_js", ANALISE, (".js", ".mjs")),
    ("analise_html", ANALISE, (".html",)),
]:
    if raiz.is_dir():
        saida[nome] = {"existe": True, **dict(zip(("n", "bytes"), conta(raiz, exts)))}
    else:
        saida[nome] = {"existe": False}

for nome in ("frontend", "portal"):
    if saida[nome].get("existe"):
        saida[nome]["tsc_linha_de_base"] = tsc(ANALISE / nome if nome == "frontend" else PORTAL)

# O QUE O JUIZ FAE COM A MENSAGEM REAL DE CADA LINGUAGEM — a pergunta que decide se o corpus mede algo.
amostras = [
    ("ts2304", "src/x.ts(3,5): error TS2304: Cannot find name 'inventado'."),
    ("ts2305", "src/x.ts(1,10): error TS2305: Module '\"./mod\"' has no exported member 'inventado'."),
    ("ts2307", "src/x.ts(1,10): error TS2307: Cannot find module './nao-existe' or its corresponding type declarations."),
    ("ts2339", "src/x.ts(5,3): error TS2339: Property 'inventado' does not exist on type 'Config'."),
    ("ts2353", "src/x.ts(5,3): error TS2353: Object literal may only specify known properties, and 'inventado' does not exist in type 'Props'."),
    ("ts2322", "src/x.ts(5,3): error TS2322: Type 'string' is not assignable to type 'number'."),
    ("ts7026", "src/x.jsx(3,5): error TS7026: JSX element implicitly has type 'any' because no interface 'JSX.IntrinsicElements' exists."),
    ("html_recurso", "src/h.html: recurso local inexistente: ./nao-existe.js"),
    ("html_ancora", "src/h.html: id 'alvo' ausente no documento (href=\"#alvo\")"),
    ("html_cdn", "https://cdn.jsdelivr.net/npm/x@9.9.99/dist/x.min.js -> HTTP 404"),
    ("html_classe", "classe 'btn-primari' não definida em nenhuma folha carregada"),
]
saida["juiz_classifica"] = {
    k: {"mensagem": v, "classe_erro": classe_erro(v)} for k, v in amostras
}

print(json.dumps(saida, ensure_ascii=False, indent=1))