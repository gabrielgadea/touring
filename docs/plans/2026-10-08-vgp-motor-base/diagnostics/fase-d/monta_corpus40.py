"""Reduced corpus for the phase D smoke: ~40 ids sampled (seed 1) from corpus_v0d, via symlinks (the original is read-only here)."""
import json
import os
import random
import warnings
from pathlib import Path

ORIG = Path("/home/gabrielgadea/.claude/touring/vgp/corpus_v0d")
DEST = Path("/tmp/claude-1000/-home-gabrielgadea-projects-touring/efe418c2-95f7-4a62-8bac-57080ed455b8/scratchpad/corpus_40")


def compila(e):
    if e.get("lang", "python") != "python":
        return True
    txt = (ORIG / e["snap"]).read_text(errors="ignore")
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            compile(txt, e["id"], "exec")
    except (SyntaxError, ValueError):
        return False
    return True


def le(nome):
    return [json.loads(x) for x in (ORIG / nome).read_text().splitlines() if x.strip()]


principal = [e for e in le("entradas.jsonl")
             if (e["rotulo"] == "negativo" or str(e.get("reproduz")).lower() == "true") and compila(e)]
inline = [e for e in le("entradas_inline_neg.jsonl") if compila(e)]
rng = random.Random(1)
amostra_p = rng.sample(principal, 20)
amostra_i = rng.sample(inline, 20)
DEST.mkdir(parents=True, exist_ok=True)
for item in ORIG.iterdir():
    if item.name in ("entradas.jsonl", "entradas_inline_neg.jsonl"):
        continue
    alvo = DEST / item.name
    if not alvo.exists():
        os.symlink(item, alvo)
(DEST / "entradas.jsonl").write_text("".join(json.dumps(e, ensure_ascii=False) + "\n" for e in amostra_p))
(DEST / "entradas_inline_neg.jsonl").write_text("".join(json.dumps(e, ensure_ascii=False) + "\n" for e in amostra_i))
print(len(amostra_p), len(amostra_i), sorted({(e.get("lang", "python"), e["rotulo"]) for e in amostra_p + amostra_i}))
