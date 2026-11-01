"""Judge of the ECC exploration coverage (round H, 17/10/2026): how much of the FUNCTIONAL tree the analysis touched.

Usage: cobertura_ecc.py <ecc_repo> <analise_dir> [--piso 0.85]
Read-only on the clone. Functional = every file except doc translations and images. Two levels:
  L1 = the body was read (path in a findings file's "lidos") or cited as evidence (findings, RELATORIO, COMPARATIVO);
  L0 = only classified (catalog entry of inventario/inventario-ecc.json or a findings file's "classificados").
A file counts for its 2nd-level dir too (a skill dir cited covers the skill's files). Exit 1 when L0 ∪ L1 < piso.
"""
import argparse
import collections
import json
import sys
from pathlib import Path

TRADUCOES = ("docs/ja-JP/", "docs/zh-CN/", "docs/zh-TW/", "docs/ko-KR/", "docs/tr/", "docs/es/", "docs/pt-BR/")


def funcionais(ecc: Path) -> list[str]:
    out = []
    for p in ecc.rglob("*"):
        r = str(p.relative_to(ecc))
        if p.is_file() and not r.startswith((".git/", "assets/")) and not r.startswith(TRADUCOES):
            out.append(r)
    return sorted(out)


def evidencias(analise: Path) -> tuple[str, set[str], set[str]]:
    texto, lidos, classificados = [], set(), set()
    for f in sorted((analise / "achados").glob("*.json")):
        d = json.loads(f.read_text(encoding="utf-8"))
        texto += [json.dumps(x, ensure_ascii=False) for x in d.get("findings", []) + d.get("linhas", [])]
        lidos |= {str(x).split(":")[0] for x in d.get("lidos", [])}
        classificados |= {str(x.get("caminho", "")) for x in d.get("classificados", []) if isinstance(x, dict)}
    for f in ("RELATORIO.md", "COMPARATIVO.md"):
        texto.append((analise / f).read_text(encoding="utf-8"))
    inv = analise / "inventario" / "inventario-ecc.json"
    if inv.exists():
        dados = json.loads(inv.read_text(encoding="utf-8"))
        classificados |= {e["caminho"] for e in dados["catalogo"]}
        # an adapter file is classified by its pairing with the canonical source (name match + body similarity)
        classificados |= {f["caminho"] for v in dados.get("adaptadores", {}).values() for f in v.get("por_arquivo", [])}
    return "\n".join(texto), lidos, classificados


def dir2(r: str) -> str:
    parts = r.split("/")
    return "/".join(parts[:2]) if len(parts) > 2 else r


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("ecc", type=Path)
    ap.add_argument("analise", type=Path)
    ap.add_argument("--piso", type=float, default=0.85)
    a = ap.parse_args()
    arquivos = funcionais(a.ecc)
    blob, lidos, classificados = evidencias(a.analise)
    l1_dirs = {dir2(x) for x in lidos} | {d for d in {dir2(r) for r in arquivos} if d in blob}
    l0_dirs = {dir2(x) for x in classificados}
    nivel = {}
    for r in arquivos:
        if r in lidos or r in blob or dir2(r) in l1_dirs:
            nivel[r] = "L1"
        elif r in classificados or dir2(r) in l0_dirs:
            nivel[r] = "L0"
        else:
            nivel[r] = "-"
    c = collections.Counter(nivel.values())
    n = len(arquivos)
    falta = collections.Counter(dir2(r) for r, v in nivel.items() if v == "-")
    res = {"funcionais": n, "L1": c["L1"], "L0": c["L0"], "nenhum": c["-"],
           "pct_L1": round(c["L1"] / n, 3), "pct_L0_ou_L1": round((c["L1"] + c["L0"]) / n, 3),
           "sem_cobertura_por_dir": falta.most_common(40)}
    (a.analise / "inventario" / "cobertura.json").write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"funcionais {n} · L1 {c['L1']} ({res['pct_L1']:.1%}) · L0 {c['L0']} · nenhum {c['-']} · "
          f"L0∪L1 {res['pct_L0_ou_L1']:.1%} (piso {a.piso:.0%})")
    print("sem cobertura (maiores):", falta.most_common(15))
    return 0 if res["pct_L0_ou_L1"] >= a.piso else 1


if __name__ == "__main__":
    sys.exit(main())
