"""Os 37 negativos de arquivo do projeto touring no holdout levam ~670 ms por chamada (mediana). Por arquivo e por
tamanho do snap: e o arquivo, o diretorio ou o tamanho? Leitura apenas; espera a V3 sair.
"""

import subprocess
import sys
import time
from collections import defaultdict
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
sys.path.insert(0, str(SCRIPTS))
import juiz_motor_vgp as jm

SP = Path(__file__).resolve().parent
KIMI = str(SP / "touring_kimi_e935")
CORPUS = SP / "corpus_holdout_ctl"
VIVO = Path.home() / ".claude/touring/vgp/corpus_v0d"


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


while subprocess.run(["pgrep", "-f", "[j]uiz_motor_vgp.py.*camadas"], capture_output=True).returncode == 0:
    time.sleep(5)


def medir(corpus: Path) -> dict[str, list[tuple[float, int, str]]]:
    _, neg = jm.carregar(corpus)
    por: dict[str, list[tuple[float, int, str]]] = defaultdict(list)
    for e in neg:
        if Path(e["project_root"]).name != "touring" or jm.balde(e) != "arquivo" or e.get("origem") == "tmp":
            continue
        t0 = time.monotonic()
        jm.rodar_motor([KIMI], e, corpus)
        ms = 1000 * (time.monotonic() - t0)
        diretorio = str(Path(e["path"]).parent).replace(str(Path.home()), "~")
        por[f"{Path(e['path']).name} @ {diretorio}"].append((ms, len((corpus / e["snap"]).read_text(errors="ignore")), e["id"]))
    return por


for nome, corpus in (("HOLDOUT", CORPUS), ("VIVO", VIVO)):
    out(f"== {nome}: negativos de arquivo do projeto touring, por arquivo")
    for arq, xs in sorted(medir(corpus).items(), key=lambda kv: -sum(x[0] for x in kv[1]) / len(kv[1])):
        ms = [x[0] for x in xs]
        out(f"   n={len(xs):2d} media={sum(ms) / len(ms):6.0f}ms max={max(ms):6.0f}ms snap~{sum(x[1] for x in xs) // len(xs):6d}c  {arq}")
