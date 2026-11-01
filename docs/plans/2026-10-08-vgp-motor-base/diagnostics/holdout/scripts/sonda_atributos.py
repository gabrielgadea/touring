"""Sonda de generalidade dos 3 falsos deny de ATRIBUTO do e935: o deny e geral (todo `np.array`) ou depende do contexto?

Roda o binario congelado em 3 programas minimos, cada um com UM atributo suspeito, mais um controle positivo
(atributo inexistente de verdade) e um controle negativo (atributo de topo comum). Leitura apenas: nada e gravado fora do scratchpad.
"""

import json
import subprocess
import tempfile
from pathlib import Path

SP = Path(__file__).resolve().parent
BIN = SP / "touring_kimi_e935"
ROOT = "/home/gabrielgadea/projects/analise"

CASOS = {
    "numpy np.array + np.where (suspeito)": "import numpy as np\n\nx = np.array([1, 0, 2])\nprint(np.where(x))\n",
    "PIL Image.LANCZOS (suspeito)": "from PIL import Image\n\nim = Image.new('RGB', (4, 4))\nim.thumbnail((2, 2), Image.LANCZOS)\n",
    "rust_bridge.RUST_AVAILABLE (suspeito)": "import scripts.aco.rust_bridge as rb\n\nprint(rb.RUST_AVAILABLE)\n",
    "CONTROLE negativo: os.path.join": "import os\n\nprint(os.path.join('a', 'b'))\n",
    "CONTROLE positivo: np.arrai (typo)": "import numpy as np\n\nprint(np.arrai([1]))\n",
    "CONTROLE positivo: os.pathh": "import os\n\nprint(os.pathh)\n",
}


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


with tempfile.TemporaryDirectory() as tmp:
    for rotulo, codigo in CASOS.items():
        arq = Path(tmp) / "probe.py"
        arq.write_text(codigo, encoding="utf-8")
        r = subprocess.run(
            [str(BIN), "vgp", "check", "--path", str(arq), "--content-file", str(arq), "--project-root", ROOT, "--lang", "python", "-j"],
            capture_output=True, text=True, timeout=60,
        )
        try:
            d = json.loads(r.stdout)
        except json.JSONDecodeError:
            out(f"{rotulo}: rc {r.returncode} stdout nao-JSON {r.stdout[:100]!r} stderr {r.stderr[:120]!r}")
            continue
        achados = [(f.get("severidade"), f.get("classe"), f.get("nome")) for f in d.get("findings", d.get("achados", []))]
        out(f"{rotulo}: veredito={d.get('verdict')} achados={achados or 'nenhum'}")
