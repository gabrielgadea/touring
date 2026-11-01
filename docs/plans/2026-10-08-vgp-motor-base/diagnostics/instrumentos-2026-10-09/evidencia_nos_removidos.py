"""Os positivos inline do corpus VIVO que o extrator sem evidencia derrubou: o que a evidencia do traceback decide.

Para cada um: (nome_esperado, linha) -> escolhido / ambiguo / recusa, e se o texto escolhido e o snap antigo.
Roda FORA do sandbox (reconstruir `--file` le os transcripts). Leitura apenas.
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"))
import corpus_v0d as c0

VIVO = c0.VIVO
STAGING = Path(sys.argv[1])


def ler(p: Path) -> list[dict]:
    return [json.loads(ln) for ln in p.read_text(encoding="utf-8").splitlines() if ln.strip()]


def inline(base: Path) -> dict[str, dict]:
    return {e["id"]: e for e in ler(base / "entradas.jsonl") if e["rotulo"] == "positivo" and e["origem"] == "inline"}


pv, ps = inline(VIVO), inline(STAGING)
for eid, e in pv.items():
    if eid in ps:
        continue
    cmd = e.get("command_original") or e.get("command", "")
    evid = (e["nome_esperado"], e["linha"]) if e.get("nome_esperado") and isinstance(e.get("linha"), int) else None
    novo, motivo = c0.programa_inline_motivo(cmd, e["session"], e["ts"], evid)
    antigo = (VIVO / e["snap"]).read_text(encoding="utf-8")
    igual = None if novo is None else novo.strip() == antigo.strip()
    print(eid, e["classe"], "reproduz" if e.get("reproduz") else "nao-reproduz", "| evid:", evid,
          "| ->", "ESCOLHIDO" if novo is not None else "recusa", "| igual ao snap antigo:", igual, "|", motivo[:70])
