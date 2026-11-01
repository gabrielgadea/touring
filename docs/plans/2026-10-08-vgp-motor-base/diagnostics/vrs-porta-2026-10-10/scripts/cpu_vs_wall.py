"""CPU (user/sys) e parede de `vgp check` num snap, por binario. Uso: python3 -I cpu_vs_wall.py PATH SNAP N BIN..."""

import json
import resource
import subprocess
import sys
import time

caminho, snap, n, *bins = sys.argv[1:]
RAIZ = "/home/gabrielgadea/projects/touring"
for b in bins:
    u0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    t0 = time.time()
    el = []
    for _ in range(int(n)):
        r = subprocess.run(
            [b, "vgp", "check", "--path", caminho, "--content-file", snap, "--project-root", RAIZ, "--lang", "rust", "-j"],
            capture_output=True,
            text=True,
            check=False,
        )
        el.append(json.loads(r.stdout).get("elapsed_ms", -1.0))
    u1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    k = int(n)
    print(
        b.split("/")[-2],
        "motor min %.0f" % min(el),
        "| por run: parede %.0f ms, user %.0f ms, sys %.0f ms" % ((time.time() - t0) / k * 1e3, (u1.ru_utime - u0.ru_utime) / k * 1e3, (u1.ru_stime - u0.ru_stime) / k * 1e3),
        "| majflt/run %.0f, minflt/run %.0f, inblock/run %.0f" % ((u1.ru_majflt - u0.ru_majflt) / k, (u1.ru_minflt - u0.ru_minflt) / k, (u1.ru_inblock - u0.ru_inblock) / k),
    )
