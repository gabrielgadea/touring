#!/usr/bin/env python3
"""Roda o motor (binario congelado) em CADA entrada de um corpus e grava, por id, o que ele respondeu.

Reusa `rodar_motor` do juiz (so leitura, importado por caminho) para que o argv seja BYTE A BYTE o do
juiz. Saida: {id: {rotulo, classe, nome_esperado, verdict, motivo, findings:[{classe,nome,linha,severidade}]}}.
Uso: python3 -I medir_ids.py --touring <bin> --corpus <dir> --out <json>
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import sys
from pathlib import Path

JUIZ = Path(
    "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/juiz_motor_vgp.py"
)


def carregar_juiz():
    spec = importlib.util.spec_from_file_location("juiz_motor_vgp_ro", JUIZ)
    mod = importlib.util.module_from_spec(spec)
    sys.modules["juiz_motor_vgp_ro"] = mod
    spec.loader.exec_module(mod)
    return mod


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--touring", required=True)
    ap.add_argument("--corpus", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args()
    juiz = carregar_juiz()
    sha_juiz = hashlib.sha256(JUIZ.read_bytes()).hexdigest()[:16]
    pos, neg = juiz.carregar(a.corpus)
    saida = {}
    for e in pos + neg:
        r = juiz.rodar_motor([a.touring], e, a.corpus)
        if "_erro" in r:
            saida[e["id"]] = {"erro": r["_erro"], "rotulo": e["rotulo"]}
            continue
        saida[e["id"]] = {
            "rotulo": e["rotulo"],
            "classe": e.get("classe"),
            "nome_esperado": e.get("nome_esperado"),
            "verdict": r.get("verdict"),
            "motivo": r.get("motivo"),
            "elapsed_ms": r.get("elapsed_ms"),
            "findings": [
                {k: f.get(k) for k in ("classe", "nome", "linha", "severidade", "sugestao")}
                for f in r.get("findings", [])
            ],
        }
    a.out.write_text(
        json.dumps(
            {"juiz_sha16": sha_juiz, "touring": a.touring, "n": len(saida), "ids": saida},
            ensure_ascii=False,
            indent=1,
            sort_keys=True,
        )
    )
    print(f"{len(saida)} ids -> {a.out} (juiz sha16 {sha_juiz})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
