"""Corpus HIBRIDO para o juiz: negativos do HOLDOUT (frescos) + positivos do VIVO que reproduzem, so como CONTROLE.

O juiz recusa corpus sem positivos ("corpus degenerado") e sem positivo de simbolo de arquivo — a guarda e o controle
positivo (um motor que libera tudo passaria um holdout so de negativos). O corpus_holdout fica PURO; o hibrido mora no
scratchpad e e montado so por COPIA (vivo e holdout sao lidos, nunca escritos). O recall do hibrido e do conjunto de
AJUSTE: serve para provar que o motor segue detectando, nao para medir generalizacao.
Uso: python3 -I montar_holdout_ctl.py
"""

import json
import shutil
from pathlib import Path

VGP = Path.home() / ".claude/touring/vgp"
VIVO, HOLD = VGP / "corpus_v0d", VGP / "corpus_holdout"
CTL = Path(__file__).resolve().parent / "corpus_holdout_ctl"


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def ler(arq: Path) -> list[dict]:
    return [json.loads(ln) for ln in arq.read_text(encoding="utf-8").splitlines() if ln.strip()]


assert not CTL.exists(), f"{CTL} ja existe — outro nome, nunca sobrescrever"
shutil.copytree(HOLD, CTL)  # parte do holdout puro
pos = [e for e in ler(VIVO / "entradas.jsonl") if e["rotulo"] == "positivo" and str(e.get("reproduz")).lower() == "true"]
man_v = json.loads((VIVO / "estado_no_instante/manifesto.json").read_text(encoding="utf-8"))
man_c = json.loads((CTL / "estado_no_instante/manifesto.json").read_text(encoding="utf-8"))
(CTL / "snap").mkdir(exist_ok=True)
copiados = 0
for e in pos:
    arquivos = [e["snap"]] + [c["snap"] for c in e.get("contexto") or []]
    estado = man_v.get(e["id"], {"disk_file": None, "overlays": [], "motivo": "sem manifesto no vivo"})
    arquivos += [o["file"] for o in estado.get("overlays", [])] + ([estado["disk_file"]] if estado.get("disk_file") else [])
    for rel in arquivos:
        destino = CTL / rel
        if destino.exists():
            continue  # snap de contexto e compartilhado entre entradas (mesmo sha): copia uma vez
        destino.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(VIVO / rel, destino)
        copiados += 1
    assert e["id"] not in man_c, "id do vivo colide com o holdout"
    man_c[e["id"]] = estado
neg_h = [e for e in ler(HOLD / "entradas.jsonl") if e["rotulo"] == "negativo"]
(CTL / "entradas.jsonl").write_text("".join(json.dumps(e, ensure_ascii=False) + "\n" for e in pos + neg_h), encoding="utf-8")
(CTL / "estado_no_instante/manifesto.json").write_text(json.dumps(man_c, ensure_ascii=False, indent=1), encoding="utf-8")
(CTL / "CONTROLE.md").write_text(
    "Corpus hibrido do juiz: negativos = holdout fresco; positivos = vivo que reproduz, SO como controle de liveness.\n", encoding="utf-8")
out("hibrido:", CTL, "| positivos de controle (vivo, reproduzem):", len(pos), "| negativos de arquivo (holdout):", len(neg_h),
    "| inline/tmp (holdout):", len(ler(CTL / "entradas_inline_neg.jsonl")), "| arquivos copiados do vivo:", copiados)
