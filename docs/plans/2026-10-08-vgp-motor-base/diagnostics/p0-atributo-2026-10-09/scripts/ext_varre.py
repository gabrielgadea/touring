"""Submodulos compilados (.so) no 1o nivel de cada pacote do venv: `from pkg import ext` nao pode negar."""

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

SP = Path("/home/gabrielgadea/projects/analise/.venv/lib/python3.12/site-packages")
ROOT = "/home/gabrielgadea/projects/analise"
BIN = sys.argv[1]

progs = {}
for d in sorted(SP.iterdir()):
    if not d.is_dir() or not (d / "__init__.py").exists() or d.name.startswith("_") or "-" in d.name:
        continue
    exts = sorted({re.sub(r"\..*$", "", p.name) for p in d.glob("*.so")})
    exts = [e for e in exts if e.isidentifier() and not (d / f"{e}.py").exists() and not (d / e).is_dir()]
    if exts:
        progs[d.name] = exts

tot = neg = 0
exemplos = []
for pkg, exts in progs.items():
    prog = "".join(f"from {pkg} import {e}\n" for e in exts)
    with tempfile.TemporaryDirectory() as t:
        a = Path(t) / "c.py"
        a.write_text(prog)
        r = subprocess.run(
            [BIN, "vgp", "check", "--path", "/tmp/x_ext.py", "--content-file", str(a),
             "--project-root", ROOT, "--lang", "python", "-j"],
            capture_output=True, text=True)
    j = json.loads(r.stdout)
    den = {f["nome"] for f in j["findings"] if f["severidade"] == "deny" and f["classe"] == "import"}
    tot += len(exts)
    neg += len(den)
    if den:
        exemplos.append((pkg, sorted(den)[:4], len(den), len(exts)))
sys.stdout.write(
    f"pacotes com .so no 1o nivel (sem .py/pasta homonima): {len(progs)} | imports de extensao: {tot} | negados (deny): {neg}\n"
)
for e in exemplos[:25]:
    sys.stdout.write(f"   {e}\n")
