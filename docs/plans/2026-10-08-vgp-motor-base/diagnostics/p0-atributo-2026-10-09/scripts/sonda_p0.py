"""Sonda do P0 (atributo dinamico): 3 baldes x casos; imprime veredito e achados por (balde, caso).

Uso: python3 sonda_p0.py <binario-do-touring> [--json saida.json]
Os baldes: 'proj' = arquivo virtual DENTRO do projeto (P2: alvo nao-externo vira warn),
'tmp' = arquivo virtual em /tmp (deny natural), 'inline' = <string> (deny natural).
"""
import json, subprocess, sys, tempfile
from pathlib import Path

ROOT = "/home/gabrielgadea/projects/analise"
CASOS = {
    "numpy np.array+np.where": "import numpy as np\n\nx = np.array([1, 0, 2])\nprint(np.where(x))\n",
    "PIL Image.LANCZOS": "from PIL import Image\n\nim = Image.new('RGB', (4, 4))\nim.thumbnail((2, 2), Image.LANCZOS)\n",
    "rust_bridge.RUST_AVAILABLE": "import scripts.aco.rust_bridge as rb\n\nprint(rb.RUST_AVAILABLE)\n",
    "CTL np.arrai (deve ser deny)": "import numpy as np\n\nprint(np.arrai([1]))\n",
    "CTL os.path.join (deve ser allow)": "import os\n\nprint(os.path.join('a', 'b'))\n",
    "OBS os.pathh": "import os\n\nprint(os.pathh)\n",
    "OBS math.sqr (stdlib fechado)": "import math\n\nprint(math.sqr(2))\n",
}
BALDES = {
    "proj": f"{ROOT}/scripts/_sonda_p0_virtual.py",
    "tmp": "/tmp/_sonda_p0_virtual.py",
    "inline": "<string>",
}


def rodar(binario, path, conteudo):
    with tempfile.TemporaryDirectory() as d:
        arq = Path(d) / "c.py"
        arq.write_text(conteudo, encoding="utf-8")
        r = subprocess.run(
            [binario, "vgp", "check", "--path", path, "--content-file", str(arq),
             "--project-root", ROOT, "--lang", "python", "-j"],
            capture_output=True, text=True, timeout=60)
    try:
        d = json.loads(r.stdout)
    except json.JSONDecodeError:
        return {"verdict": f"NAO-JSON rc={r.returncode}", "achados": [], "stderr": r.stderr[:200]}
    return {"verdict": d.get("verdict"),
            "achados": [(f["severidade"], f["classe"], f["nome"]) for f in d.get("findings", [])]}


if __name__ == "__main__":
    binario = sys.argv[1]
    saida = {}
    for caso, codigo in CASOS.items():
        for balde, path in BALDES.items():
            r = rodar(binario, path, codigo)
            saida[f"{caso} | {balde}"] = r
            print(f"{caso:38s} {balde:6s} -> {r['verdict']:6s} {r['achados'] or ''}")
    if "--json" in sys.argv:
        Path(sys.argv[sys.argv.index("--json") + 1]).write_text(json.dumps(saida, indent=1, ensure_ascii=False))
