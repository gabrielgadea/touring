"""Verificacoes finais do holdout. Leitura apenas."""

import hashlib
import json
import statistics
import time
from datetime import datetime
from pathlib import Path

VGP = Path.home() / ".claude/touring/vgp"
VIVO, HOLD = VGP / "corpus_v0d", VGP / "corpus_holdout"
SP = Path(__file__).resolve().parent
BUNDLE = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def sha_tree(raiz: Path) -> str:
    h = hashlib.sha256()
    for f in sorted(p for p in raiz.rglob("*") if p.is_file()):
        h.update(str(f.relative_to(raiz)).encode() + hashlib.sha256(f.read_bytes()).digest())
    return h.hexdigest()[:16]


meta = json.loads((HOLD / "holdout_meta.json").read_text(encoding="utf-8"))
agora = sha_tree(VIVO)
out("vivo sha_tree: antes (meta)", meta["vivo_sha_tree_antes"], "| agora", agora, "| IDENTICO:", agora == meta["vivo_sha_tree_antes"])
man = {}
for ln in (SP / "vivo_manifest_antes.txt").read_text().splitlines():
    h, _, rel = ln.partition("  ")
    man[rel] = h
atual = {"./" + str(f.relative_to(VIVO)): hashlib.sha256(f.read_bytes()).hexdigest() for f in VIVO.rglob("*") if f.is_file()}
out("vivo, manifesto de hashes da tarefa anterior:", len(man), "arquivos | agora", len(atual), "| diferentes:", sum(1 for k in man if atual.get(k) != man[k]),
    "| novos:", len(set(atual) - set(man)), "| sumidos:", len(set(man) - set(atual)))

t_meta = (HOLD / "holdout_meta.json").stat().st_mtime
mexidos = [str(f.relative_to(HOLD)) for f in HOLD.rglob("*") if f.is_file() and f.stat().st_mtime > t_meta + 1]
out("holdout: arquivos modificados DEPOIS do holdout_meta.json:", len(mexidos), mexidos[:5], "| total de arquivos:", sum(1 for f in HOLD.rglob("*") if f.is_file()))
out("holdout sha_tree agora:", sha_tree(HOLD))

for nome in ("corpus_v0d.py", "corpus_v5_inline_neg.py", "test_corpus_v0d.py", "shell_inline.py"):
    p = BUNDLE / nome
    out(f"  {nome}: mtime {time.strftime('%d/%m %H:%M:%S', time.localtime(p.stat().st_mtime))} sha {hashlib.sha256(p.read_bytes()).hexdigest()[:16]}")
out("  (o build do holdout comecou as", meta["ate"], "UTC = 18:12 BRT)")

ate = datetime.fromisoformat(meta["ate"].replace("Z", "+00:00"))
ents = [json.loads(ln) for ln in (HOLD / "entradas.jsonl").read_text().splitlines() if ln.strip()]
idade_min = [(ate - datetime.fromisoformat(e["ts"].replace("Z", "+00:00"))).total_seconds() / 60 for e in ents if e["rotulo"] == "negativo"]
out("negativos de arquivo: idade da edicao no congelamento (min): mediana", round(statistics.median(idade_min)), "| < 30 min:", sum(1 for x in idade_min if x < 30),
    "de", len(idade_min), "| < 10 min:", sum(1 for x in idade_min if x < 10))
