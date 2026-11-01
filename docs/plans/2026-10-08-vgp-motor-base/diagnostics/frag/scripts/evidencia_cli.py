#!/usr/bin/env python3
"""Os casos do pedido FRAG pelo CLI `touring vgp check`, nos DOIS binários (base = espelho pristino, frag), com a
camada 2 real (pyright) onde o caso a exige. Tabela: caso x (binário, contexto) -> veredito e achados.

    evidencia_cli.py <touring-base> <touring-frag> <dir-de-trabalho> [saida.json]
"""

import json
import subprocess
import sys
from pathlib import Path

RAIZ = "/home/gabrielgadea/projects/analise"  # o projeto com venv e pyright (a camada 2 real)
FRAGMENTO = "resultado = 1\nprint(nome_do_projeto_inexistente)\n"
PATHX = "import os\nprint(os.pathx.join('a', 'b'))\n"
LOADZ = "import json\nprint(json.loadz('{}'))\n"
COMBENDING = "import unicodedata\nprint(unicodedata.combending('a'))\n"
LIMPO = "import os\nprint(os.path.join('a', 'b'))\n"

# (rótulo, path virtual, conteúdo, camadas)
CASOS = [
    ("a) fragmento em /tmp, nome do projeto [L1]", "/tmp/frag.py", FRAGMENTO, "1"),
    ("a') o mesmo, com a camada 2 real [L1+L2]", "/tmp/frag.py", FRAGMENTO, "1,2"),
    ("d) controle limpo em /tmp [L1+L2]", "/tmp/ok.py", LIMPO, "1,2"),
    ("e) /tmp/x.py com os.pathx [L1+L2]", "/tmp/x.py", PATHX, "1,2"),
    ("e') /tmp/x.py com json.loadz [L1+L2]", "/tmp/x.py", LOADZ, "1,2"),
    ("e'') /tmp com stdlib que a L1 modela [L1]", "/tmp/x.py", COMBENDING, "1"),
    ("inline <string>, nome do projeto [L1+L2]", "<string>", FRAGMENTO, "1,2"),
    ("inline <string>, os.pathx [L1+L2]", "<string>", PATHX, "1,2"),
    ("scratch do projeto (.cache/), nome do projeto [L1+L2]", f"{RAIZ}/.cache/frag-x/a.py", FRAGMENTO, "1,2"),
    ("arquivo do projeto, nome do projeto [L1+L2]", f"{RAIZ}/scripts/frag_x_a.py", FRAGMENTO, "1,2"),
]
RODADAS = [("base", None), ("frag", "escrita"), ("frag", "execucao")]


def rodar(binario: str, arquivo: Path, path: str, camadas: str, contexto: str | None) -> dict:
    cmd = [binario, "vgp", "check", "--path", path, "--content-file", str(arquivo), "--project-root", RAIZ,
           "--camadas", camadas, "-j"]
    if contexto:
        cmd += ["--contexto", contexto]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=120, check=False)
    try:
        o = json.loads(r.stdout)
    except json.JSONDecodeError:
        return {"verdict": "ERRO", "achados": [r.stdout[:80], r.stderr[:120]], "camada2": None}
    return {"verdict": o["verdict"], "camada2": o.get("camada2"),
            "achados": [(f["classe"], f["nome"], f["severidade"]) for f in o.get("findings", [])]}


def main(base: str, frag: str, trabalho: Path, saida: Path | None) -> int:
    trabalho.mkdir(parents=True, exist_ok=True)
    tabela = []
    for i, (rotulo, path, conteudo, camadas) in enumerate(CASOS):
        arq = trabalho / f"caso{i}.py"
        arq.write_text(conteudo)
        linha = {"caso": rotulo, "path": path, "camadas": camadas}
        for lado, contexto in RODADAS:
            linha[f"{lado}:{contexto or '-'}"] = rodar(base if lado == "base" else frag, arq, path, camadas, contexto)
        tabela.append(linha)
        print(f"\n{rotulo}  [{path}]")
        for chave in [k for k in linha if ":" in k]:
            r = linha[chave]
            print(f"   {chave:16s} {r['verdict']:6s} {r['achados']}  camada2={str(r['camada2'])[:60]}")
    if saida:
        saida.write_text(json.dumps(tabela, ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    a = sys.argv
    sys.exit(main(a[1], a[2], Path(a[3]), Path(a[4]) if len(a) > 4 else None))
