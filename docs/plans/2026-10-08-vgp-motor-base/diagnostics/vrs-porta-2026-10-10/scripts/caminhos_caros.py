"""Acha quais caminhos (a::b::c) de um arquivo custam caro no motor: testa cada caminho distinto sozinho.

Uso: python3 -I caminhos_caros.py BIN PATH_REAL SNAP [limiar_ms]"""

import collections
import json
import re
import subprocess
import sys
import tempfile

binario, caminho, snap, *resto = sys.argv[1:]
limiar = float(resto[0]) if resto else 25.0
RAIZ = "/home/gabrielgadea/projects/touring"
src = open(snap).read()
cab = "".join(l for l in src.splitlines(True) if l.startswith("use "))
cont = collections.Counter(re.findall(r"\b[A-Za-z_]\w*(?:::[A-Za-z_]\w*)+", src))
print("caminhos distintos:", len(cont), "usos:", sum(cont.values()))


def medir(corpo):
    with tempfile.NamedTemporaryFile("w", suffix=".rs", dir=".", delete=False) as f:
        f.write(corpo)
        nome = f.name
    r = subprocess.run(
        [binario, "vgp", "check", "--path", caminho, "--content-file", nome, "--project-root", RAIZ, "--lang", "rust", "-j"],
        capture_output=True,
        text=True,
        check=False,
    )
    try:
        o = json.loads(r.stdout)
        return o.get("elapsed_ms", -1.0), o["verdict"]
    except Exception:  # noqa: BLE001
        return -1.0, "erro"


base, _ = medir(cab + "fn main() {}\n")
print("base (so use):", round(base))
caros = []
for c, n in cont.most_common():
    t = min(medir(cab + "fn main() { let _ = %s; }\n" % c)[0] for _ in range(2))
    if t > limiar:
        caros.append((t, c, n))
for t, c, n in sorted(caros, reverse=True)[:25]:
    print(round(t), c, "x%d" % n)
print("caros:", len(caros))
