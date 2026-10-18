"""Tempo (elapsed_ms do motor, minimo de N) de `vgp check` num trecho sintetico POSICIONADO num arquivo real.

Uso: touring run --file tempo_trechos.py --args '[BIN1,BIN2, PATH_REAL, N, "trecho1", "trecho2", ...]'
Um trecho que comeca por `@` e o caminho de um arquivo cujo conteudo inteiro e o trecho."""

import json
import subprocess
import sys
import tempfile

bins, caminho, n, *trechos = sys.argv[1:]
n = int(n)
RAIZ = "/home/gabrielgadea/projects/touring"
for t in trechos:
    if t.startswith("@"):
        corpo = open(t[1:]).read()
    else:
        corpo = t.replace("\\n", "\n")
    with tempfile.NamedTemporaryFile("w", suffix=".rs", dir=".", delete=False) as f:
        f.write(corpo)
        nome = f.name
    linha = [repr(t[:70])]
    for b in bins.split(","):
        ts = []
        ver = "?"
        for _ in range(n):
            r = subprocess.run(
                [b, "vgp", "check", "--path", caminho, "--content-file", nome, "--project-root", RAIZ, "--lang", "rust", "-j"],
                capture_output=True,
                text=True,
                check=False,
            )
            try:
                o = json.loads(r.stdout)
                ts.append(o.get("elapsed_ms", -1.0))
                ver = o["verdict"] + str([x["nome"] for x in o["findings"]][:3])
            except Exception:  # noqa: BLE001
                ts.append(-1.0)
        linha.append("%s: min %.0f %s" % (b.split("/")[-2][:6], min(ts), ver))
    print(" | ".join(linha))
