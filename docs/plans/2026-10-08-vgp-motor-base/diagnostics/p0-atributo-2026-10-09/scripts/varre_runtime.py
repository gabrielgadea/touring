"""Varredura do binario contra o runtime: para cada pacote do venv, TODO nome publico de dir(modulo) vira
`m.<nome>` num programa; o motor nao pode negar nenhum (cada nome existe de verdade). Mede o falso deny de
atributo de modulo por pacote, no binario base (e935aaf1) e no novo.

Uso: python3 varre_runtime.py <binario> <saida.json> [pacote ...]   (sem pacotes: o allowlist do censo)
"""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import censo  # noqa: E402

ROOT = "/home/gabrielgadea/projects/analise"
BIN, SAIDA = sys.argv[1], Path(sys.argv[2])
PACOTES = sys.argv[3:]


def alvo_pacotes():
    if PACOTES:
        return PACOTES
    return [Path(p).parent.name for p in Path(Path(__file__).parent / "censo_lista.txt").read_text().split()]


def checar(pkg, nomes):
    prog = f"import {pkg} as m\n" + "".join(f"m.{n}\n" for n in nomes)
    with tempfile.TemporaryDirectory() as d:
        arq = Path(d) / "c.py"
        arq.write_text(prog, encoding="utf-8")
        r = subprocess.run(
            [BIN, "vgp", "check", "--path", "/tmp/_varre_runtime.py", "--content-file", str(arq),
             "--project-root", ROOT, "--lang", "python", "-j"],
            capture_output=True, text=True, timeout=120)
    try:
        d = json.loads(r.stdout)
    except json.JSONDecodeError:
        return None
    return [f["nome"] for f in d.get("findings", []) if f["classe"] == "atributo" and f["severidade"] == "deny"]


def main():
    res = {}
    for pkg in alvo_pacotes():
        rt, erro = censo.runtime(pkg)
        if rt is None:
            res[pkg] = {"erro": erro}
            continue
        sub = {p.stem for p in (censo.SP / pkg).iterdir()}
        nomes = sorted(n for n in rt if n.isidentifier() and not n.startswith("_") and n not in sub)
        negados = checar(pkg, nomes)
        res[pkg] = {"n": len(nomes), "negados": negados if negados is None else negados[:30],
                    "n_negados": None if negados is None else len(negados)}
    SAIDA.write_text(json.dumps(res, indent=1, ensure_ascii=False))
    com = {k: v for k, v in res.items() if v.get("n_negados")}
    print(f"pacotes: {len(res)} | com runtime: {sum('n' in v for v in res.values())} | "
          f"com falso deny de atributo: {len(com)} | nomes negados no total: {sum(v['n_negados'] for v in com.values())}")
    for k, v in sorted(com.items()):
        print(f"  {k}: {v['n_negados']}/{v['n']} ex {v['negados'][:6]}")


main()
