"""Conjuntos de EXCLUSAO do held-out disjunto: tudo o que o corpus VIVO e o holdout TEMPORAL ja contem, nas tres chaves.

  ids          o `id` da entrada
  tool_use_id  o evento de origem (inline/positivos); arquivo-negativo nao tem
  sha          o `snap_sha256` do snapshot E o sha de cada `contexto` (irmaos que o motor viu junto)
  comandos     o texto do comando (command / command_original), como gravado

Importado por construir_disjunto.py e verificar_sobreposicao.py. Leitura apenas.
"""

import hashlib
import json
from pathlib import Path

VGP = Path.home() / ".claude/touring/vgp"
VIVO, TEMPORAL = VGP / "corpus_v0d", VGP / "corpus_holdout"


def ler(arq: Path) -> list[dict]:
    return [json.loads(ln) for ln in arq.read_text(encoding="utf-8").splitlines() if ln.strip()] if arq.is_file() else []


def entradas(base: Path) -> list[dict]:
    return ler(base / "entradas.jsonl") + ler(base / "entradas_inline_neg.jsonl")


def chaves(base: Path) -> dict[str, set[str]]:
    ids, tids, shas, cmds = set(), set(), set(), set()
    for e in entradas(base):
        ids.add(e["id"])
        if e.get("tool_use_id"):
            tids.add(e["tool_use_id"])
        if e.get("snap_sha256"):
            shas.add(e["snap_sha256"])
        for c in e.get("contexto") or []:
            if isinstance(c, dict) and c.get("snap") and (base / c["snap"]).is_file():
                shas.add(hashlib.sha256((base / c["snap"]).read_text(encoding="utf-8").encode()).hexdigest())
        for campo in ("command", "command_original"):
            if (e.get(campo) or "").strip():
                cmds.add(e[campo].strip())
    return {"ids": ids, "tool_use_ids": tids, "shas": shas, "comandos": cmds}


def exclusoes() -> dict[str, set[str]]:
    v, t = chaves(VIVO), chaves(TEMPORAL)
    return {k: v[k] | t[k] for k in v}
