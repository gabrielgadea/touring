"""A/B de latencia do motor (elapsed_ms que o proprio motor reporta): binario base x novo, rodadas alternadas.

Uso: python3 bench_ab.py <base> <novo> [n]
"""

import json
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = "/home/gabrielgadea/projects/analise"
BASE, NOVO = sys.argv[1], sys.argv[2]
N = int(sys.argv[3]) if len(sys.argv) > 3 else 30
PROGS = {
    "numpy": "import numpy as np\nx = np.array([1])\nprint(np.where(x), np.zeros(3), np.arrai)\n",
    "pandas": "import pandas as pd\nprint(pd.DataFrame, pd.read_csv, pd.concat, pd.zzz)\n",
    "scipy+sklearn": "import scipy\nimport sklearn\nprint(scipy.stats, sklearn.metrics, scipy.zzz)\n",
    "requests": "import requests\nprint(requests.get, requests.Session, requests.zzz)\n",
    "pillow+lxml": "from PIL import Image\nfrom lxml import etree\nprint(Image.new, Image.LANCZOS, etree.fromstring)\n",
    "projeto": "import scripts.aco.rust_bridge as rb\nprint(rb.RUST_AVAILABLE)\n",
}


def uma(binario, arq):
    r = subprocess.run(
        [binario, "vgp", "check", "--path", "/tmp/_bench.py", "--content-file", str(arq),
         "--project-root", ROOT, "--lang", "python", "-j"],
        capture_output=True, text=True)
    return json.loads(r.stdout)["elapsed_ms"]


with tempfile.TemporaryDirectory() as d:
    for nome, prog in PROGS.items():
        arq = Path(d) / f"{nome.replace('+', '_')}.py"
        arq.write_text(prog)
        uma(BASE, arq)
        uma(NOVO, arq)  # aquecimento
        b, n = [], []
        for _ in range(N):
            b.append(uma(BASE, arq))
            n.append(uma(NOVO, arq))
        q = lambda xs, p: sorted(xs)[min(len(xs) - 1, int(p * len(xs)))]
        sys.stdout.write(
            f"{nome:14s} base med {statistics.median(b):7.2f} p95 {q(b, .95):7.2f} | "
            f"novo med {statistics.median(n):7.2f} p95 {q(n, .95):7.2f} ms\n"
        )
