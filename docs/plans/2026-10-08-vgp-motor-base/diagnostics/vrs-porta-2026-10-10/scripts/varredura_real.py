#!/usr/bin/env python3
"""Varredura de NEGATIVOS REAIS: o motor Rust (binario congelado) sobre todo `.rs` de crates reais que
compilam (touring-wt-semeado, 88c7cc8b, limpo). Cada achado e, por construcao, um falso positivo
(o arquivo compila) -- mede o ruido do adaptador fora do corpus sintetico.

So o agregado vai ao stdout; o detalhe vai ao --out.
Uso: python3 -I varredura_real.py --touring <bin> --out <json> [--max N]
"""

from __future__ import annotations

import argparse
import collections
import json
import subprocess
from pathlib import Path

RAIZ = Path("/home/gabrielgadea/projects/touring-wt-semeado")
CRATES = ["touring-code", "touring-identity", "touring-pyright"]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--touring", required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--max", type=int, default=0)
    a = ap.parse_args()
    arquivos: list[Path] = []
    for c in CRATES:
        d = RAIZ / "crates" / c
        if d.is_dir():
            arquivos += sorted(p for p in d.rglob("*.rs") if "target" not in p.parts)
    if a.max:
        arquivos = arquivos[: a.max]
    veredito = collections.Counter()
    por_classe = collections.Counter()
    por_sev = collections.Counter()
    motivos = collections.Counter()
    tempos = []
    detalhe = []
    for p in arquivos:
        cmd = [
            a.touring, "vgp", "check", "--path", str(p), "--content-file", str(p),
            "--project-root", str(RAIZ), "--lang", "rust", "-j",
        ]
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=60, check=False)
        try:
            o = json.loads(r.stdout)
        except json.JSONDecodeError:
            veredito["ERRO"] += 1
            detalhe.append({"arquivo": str(p), "erro": r.stdout[:120] + r.stderr[:120]})
            continue
        veredito[o["verdict"]] += 1
        tempos.append(o.get("elapsed_ms", 0.0))
        if o.get("motivo"):
            motivos[o["motivo"].split(":")[0][:80]] += 1
        for f in o.get("findings", []):
            por_classe[f["classe"]] += 1
            por_sev[f["severidade"]] += 1
            detalhe.append(
                {
                    "arquivo": str(p.relative_to(RAIZ)),
                    "linha": f["linha"],
                    "classe": f["classe"],
                    "nome": f["nome"],
                    "severidade": f["severidade"],
                    "sugestao": f["sugestao"][:140],
                }
            )
    a.out.write_text(json.dumps(detalhe, ensure_ascii=False, indent=1))
    tempos.sort()
    p95 = tempos[min(len(tempos) - 1, int(0.95 * len(tempos)))] if tempos else None
    arquivos_com_achado = len({d["arquivo"] for d in detalhe if "classe" in d})
    print(
        json.dumps(
            {
                "arquivos": len(arquivos),
                "veredito": dict(veredito),
                "arquivos_com_achado": arquivos_com_achado,
                "achados_por_classe": dict(por_classe),
                "achados_por_severidade": dict(por_sev),
                "motivos_nao_medido": dict(motivos.most_common(6)),
                "p95_ms": p95,
            },
            ensure_ascii=False,
            indent=1,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
