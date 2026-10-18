"""Sela um held-out: grava o `selo.json` no formato do 20261011 (sha_tree, arquivos, bytes, sha256 dos tres indices) com o
MESMO `sha_tree` de `scripts/confere_selo.py` (importado, nao reescrito), e confere o selo recem-gravado contra o corpus
(`conferir` tem de devolver vazio). Campos extras (`--extra chave=valor`, repetivel) entram no selo e nao sao checados por
`confere_selo` — servem de proveniencia (sha do adjudicador, do estado no instante, dos congelados).

Uso: python3 -I -B selar_holdout.py <corpus> <selo.json> [--extra chave=valor]...
Nao roda motor nem juiz: le arquivos e hasheia.
"""

import argparse
import hashlib
import importlib.util
import json
import sys
from pathlib import Path

CONFERE = Path(__file__).resolve().parents[2] / "scripts" / "confere_selo.py"
spec = importlib.util.spec_from_file_location("confere_selo", CONFERE)
confere = importlib.util.module_from_spec(spec)
spec.loader.exec_module(confere)

ap = argparse.ArgumentParser()
ap.add_argument("corpus", type=Path)
ap.add_argument("selo", type=Path)
ap.add_argument("--extra", action="append", default=[], metavar="CHAVE=VALOR")
a = ap.parse_args()
corpus = a.corpus.expanduser()
assert not a.selo.exists(), f"{a.selo} ja existe: um selo nao se reescreve"
arquivos = [p for p in corpus.rglob("*") if p.is_file()]
selo = {
    "corpus": str(corpus),
    "sha_tree": confere.sha_tree(corpus),
    "arquivos": len(arquivos),
    "bytes": sum(p.stat().st_size for p in arquivos),
}
for campo, nome in confere.INDICES.items():
    selo[campo] = hashlib.sha256((corpus / nome).read_bytes()).hexdigest()
selo["selado"] = "o motor e o juiz NAO rodaram neste corpus; abrir so depois do P0 e conferir o sha_tree antes (scripts/confere_selo.py)"
for par in a.extra:
    chave, _, valor = par.partition("=")
    selo[chave] = valor
violacoes = confere.conferir(corpus, selo)
assert not violacoes, f"selo recem-gravado nao confere: {violacoes}"
a.selo.write_text(json.dumps(selo, indent=1, ensure_ascii=False), encoding="utf-8")
json.dump(selo, sys.stdout, indent=1, ensure_ascii=False)
print()
