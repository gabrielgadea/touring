"""Segunda rodada de causas dos 7 falsos deny do e935 no disjunto. Leitura apenas.

1. PIL.Image: como `LANCZOS` chega ao modulo (globals().update? __getattr__?).
2. TSEClient: existe em scripts/external_apis/tse*? algum commit o introduziu/removeu?
3. Os 3 snapshots com nome sem definicao: a edicao SEGUINTE do mesmo arquivo define o nome? o arquivo foi EXECUTADO entre as duas?
"""

import re
import subprocess
import sys
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
sys.path.insert(0, str(SCRIPTS))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import corpus_v0d as c0
import exclusoes_disjunto as ex

ANALISE = Path.home() / "projects/analise"
SITE = next((ANALISE / ".venv/lib").glob("python3*/site-packages"))
HOLD = ex.VGP / "corpus_holdout_disjunto"


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


img = (SITE / "PIL/Image.py").read_text(encoding="utf-8")
out("1. PIL.Image: globals().update:", re.findall(r".*globals\(\)\.update.*", img)[:2], "| def __getattr__:", bool(re.search(r"^def __getattr__", img, re.M)),
    "| 'LANCZOS' em linhas de topo:", [ln.strip()[:70] for ln in img.splitlines() if re.match(r"(NEAREST|LANCZOS)\b|.*\bLANCZOS\s*=", ln) and not ln.startswith(" ")][:3])
r = subprocess.run(["git", "-C", str(ANALISE), "log", "--oneline", "-S", "TSEClient", "--", "scripts/external_apis"], capture_output=True, text=True)
out("2. TSEClient: ls scripts/external_apis/tse*:", sorted(p.name for p in (ANALISE / "scripts/external_apis").glob("tse*")), "| commits que tocam o simbolo:", r.stdout.strip().splitlines()[:5] or "NENHUM")
g = subprocess.run(["grep", "-rn", "TSEClient", str(ANALISE / "scripts/external_apis"), "--include=*.py", "-m", "2"], capture_output=True, text=True).stdout.strip().splitlines()[:4]
out("   ocorrencias em scripts/external_apis hoje:", [x.replace(str(ANALISE), "") [:90] for x in g] or "NENHUMA")

indice = c0.varrer_transcripts()
ents = {e["id"]: e for e in ex.entradas(HOLD)}
for eid, nome in (("ddbb2cfb4afc", "ComputeProcessContext"), ("74f04a24de12", "WS"), ("c35d78e3eb25", "WIRING")):
    e = ents[eid]
    arq, ts = e["path"], e["ts"]
    regs = indice[arq]
    tempos = sorted(c0._edicoes(regs).get(arq, []))
    seguintes = [t for t in tempos if t > ts]
    definido_em = None
    for t in seguintes[:12]:
        txt = c0._conteudo_depois(arq, t, indice) or ""
        if re.search(rf"(^|\n)\s*{re.escape(nome)}\s*[:=]|(^|\n)\s*(from\s+\S+\s+)?import\s+.*\b{re.escape(nome)}\b|(^|\n)\s*(def|class)\s+{re.escape(nome)}\b", txt):
            definido_em = t
            break
    base = Path(arq).name
    entre = 0
    if definido_em:
        for rec in regs:
            if ts < rec.get("timestamp", "") < definido_em:
                for c in (rec.get("message") or {}).get("content") or []:
                    if isinstance(c, dict) and c.get("type") == "tool_use" and c.get("name") == "Bash" and base in str((c.get("input") or {}).get("command", "")):
                        entre += 1
    out(f"3. {eid} {base} `{nome}`: edicoes depois do snap: {len(seguintes)} | primeira que DEFINE o nome: {definido_em} "
        f"(+{'?' if not definido_em else round((__import__('datetime').datetime.fromisoformat(definido_em.replace('Z', '+00:00')) - __import__('datetime').datetime.fromisoformat(ts.replace('Z', '+00:00'))).total_seconds())}s) | Bash que cita o arquivo ENTRE as duas: {entre}")
