"""Corpus HIBRIDO para o juiz: negativos de um held-out + positivos do VIVO que reproduzem, so como CONTROLE de liveness.

Igual a montar_holdout_ctl.py, parametrizado: o juiz recusa corpus sem positivos (guarda de controle positivo). O held-out
fica PURO; o hibrido mora no scratchpad e e montado so por COPIA (vivo e held-out sao lidos, nunca escritos).
Uso: python3 -I montar_ctl_generico.py <holdout_dir> <ctl_dir>
"""

import json
import shutil
import sys
from pathlib import Path

VIVO = Path.home() / ".claude/touring/vgp/corpus_v0d"
HOLD, CTL = Path(sys.argv[1]), Path(sys.argv[2])
assert HOLD.resolve() != VIVO.resolve() and not CTL.exists(), f"{CTL} ja existe ou holdout == vivo"


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def ler(arq: Path) -> list[dict]:
    return [json.loads(ln) for ln in arq.read_text(encoding="utf-8").splitlines() if ln.strip()]


shutil.copytree(HOLD, CTL)
pos = [e for e in ler(VIVO / "entradas.jsonl") if e["rotulo"] == "positivo" and str(e.get("reproduz")).lower() == "true"]
man_v = json.loads((VIVO / "estado_no_instante/manifesto.json").read_text(encoding="utf-8"))
man_c = json.loads((CTL / "estado_no_instante/manifesto.json").read_text(encoding="utf-8"))
(CTL / "snap").mkdir(exist_ok=True)
copiados = 0
for e in pos:
    estado = man_v.get(e["id"], {"disk_file": None, "overlays": [], "motivo": "sem manifesto no vivo"})
    arquivos = [e["snap"]] + [c["snap"] for c in e.get("contexto") or []] + [o["file"] for o in estado.get("overlays", [])]
    arquivos += [estado["disk_file"]] if estado.get("disk_file") else []
    for rel in arquivos:
        destino = CTL / rel
        if not destino.exists():
            destino.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(VIVO / rel, destino)
            copiados += 1
    assert e["id"] not in man_c, "id do vivo colide com o held-out"
    man_c[e["id"]] = estado
neg = [e for e in ler(HOLD / "entradas.jsonl") if e["rotulo"] == "negativo"]
(CTL / "entradas.jsonl").write_text("".join(json.dumps(e, ensure_ascii=False) + "\n" for e in pos + neg), encoding="utf-8")
(CTL / "estado_no_instante/manifesto.json").write_text(json.dumps(man_c, ensure_ascii=False, indent=1), encoding="utf-8")
(CTL / "CONTROLE.md").write_text("Hibrido do juiz: negativos = held-out; positivos = vivo que reproduz, SO como controle.\n", encoding="utf-8")
out("hibrido:", CTL, "| positivos de controle:", len(pos), "| negativos de arquivo:", len(neg),
    "| inline/tmp:", len(ler(CTL / "entradas_inline_neg.jsonl")), "| arquivos copiados do vivo:", copiados)
