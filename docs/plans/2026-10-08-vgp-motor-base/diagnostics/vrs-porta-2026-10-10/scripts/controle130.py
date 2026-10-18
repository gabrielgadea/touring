#!/usr/bin/env python3
"""Controle negativo em codigo que COMPILA: o motor Rust sobre os 130 `.rs` fixos de touring-wt-semeado@88c7cc8b.

Manifesto fixo (caminho relativo + sha256 de cada arquivo): `--gera-manifesto` o escreve uma vez; as
rodadas seguintes exigem que cada arquivo ainda tenha o MESMO sha (senao o controle e invalido).
Todo achado e falso positivo por construcao (o arquivo compila). So agregados vao ao stdout; o detalhe
vai ao --out.

Uso: python3 -I controle130.py --manifesto M.json --gera-manifesto
     python3 -I controle130.py --manifesto M.json --touring BIN --out detalhe.json
"""

import argparse
import collections
import hashlib
import json
import subprocess
from pathlib import Path

RAIZ = Path("/home/gabrielgadea/projects/touring-wt-semeado")
CRATES = ["touring-code", "touring-identity", "touring-pyright"]


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def lista() -> list[Path]:
    arq: list[Path] = []
    for c in CRATES:
        d = RAIZ / "crates" / c
        if d.is_dir():
            arq += sorted(p for p in d.rglob("*.rs") if "target" not in p.parts)
    return arq


def zona(rel: str) -> str:
    partes = Path(rel).parts
    for z in ("tests", "benches", "examples"):
        if z in partes:
            return z
    return "src"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--manifesto", type=Path, required=True)
    ap.add_argument("--gera-manifesto", action="store_true")
    ap.add_argument("--touring")
    ap.add_argument("--out", type=Path)
    a = ap.parse_args()
    if a.gera_manifesto:
        ents = [{"arquivo": str(p.relative_to(RAIZ)), "sha256": sha(p)} for p in lista()]
        corpo = {"raiz": str(RAIZ), "commit": "88c7cc8b", "n": len(ents), "arquivos": ents}
        a.manifesto.write_text(json.dumps(corpo, indent=1))
        print(f"manifesto: {len(ents)} arquivos; sha256 do manifesto {sha(a.manifesto)[:16]}")
        return 0
    man = json.loads(a.manifesto.read_text())
    mudou = [e["arquivo"] for e in man["arquivos"] if sha(RAIZ / e["arquivo"]) != e["sha256"]]
    if mudou:
        print(f"CONTROLE INVALIDO: {len(mudou)} arquivo(s) mudaram de sha: {mudou[:5]}")
        return 2
    veredito = collections.Counter()
    por_classe = collections.Counter()
    por_zona = collections.defaultdict(lambda: [0, 0])  # zona -> [arquivos, com achado]
    tempos = []
    detalhe = []
    for e in man["arquivos"]:
        rel = e["arquivo"]
        p = RAIZ / rel
        cmd = [
            a.touring, "vgp", "check", "--path", str(p), "--content-file", str(p),
            "--project-root", str(RAIZ), "--lang", "rust", "-j",
        ]
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=120, check=False)
        z = zona(rel)
        por_zona[z][0] += 1
        try:
            o = json.loads(r.stdout)
        except json.JSONDecodeError:
            veredito["ERRO"] += 1
            detalhe.append({"arquivo": rel, "erro": (r.stdout + r.stderr)[:160]})
            continue
        veredito[o["verdict"]] += 1
        tempos.append(o.get("elapsed_ms", 0.0))
        achados = o.get("findings", [])
        if achados:
            por_zona[z][1] += 1
        for f in achados:
            por_classe[f["classe"]] += 1
            detalhe.append(
                {
                    "arquivo": rel, "zona": z, "linha": f["linha"], "classe": f["classe"],
                    "nome": f["nome"], "severidade": f["severidade"], "sugestao": f["sugestao"][:140],
                }
            )
        if o.get("motivo"):
            detalhe.append({"arquivo": rel, "zona": z, "motivo": o["motivo"][:300], "verdict": o["verdict"]})
    if a.out:
        a.out.write_text(json.dumps(detalhe, ensure_ascii=False, indent=1))
    tempos.sort()
    n_achados = sum(por_classe.values())
    com_achado = len({d["arquivo"] for d in detalhe if "classe" in d})
    print(
        json.dumps(
            {
                "manifesto_sha16": sha(a.manifesto)[:16],
                "arquivos": man["n"],
                "arquivos_com_achado": com_achado,
                "achados": n_achados,
                "achados_por_classe": dict(por_classe),
                "veredito": dict(veredito),
                "por_zona_[arquivos,com_achado]": dict(por_zona),
                "p95_ms": tempos[min(len(tempos) - 1, int(0.95 * len(tempos)))] if tempos else None,
            },
            ensure_ascii=False,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
