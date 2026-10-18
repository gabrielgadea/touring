"""HORUST-REAL — integridade do corpus ANTES de selar, e o selo. Cada propriedade pré-registrada (disjunção por arquivo, 1 injeção por
arquivo, n mínimo, teto de concentração, bytes do worktree intactos, snap == o que o oráculo validou) vira uma verificação que devolve a
violação por extenso. Lista vazia = íntegro. O selo reusa o `sha_tree` do `confere_selo.py` (mesmo método do held-out 20261011).
"""

from __future__ import annotations

import collections
import hashlib
import json
import os
import sys
from pathlib import Path

import selecao_horust as sel

_SCRIPTS = Path(os.environ.get("HORUST_SCRIPTS_DIR") or Path(__file__).resolve().parents[2] / "scripts")
if str(_SCRIPTS) not in sys.path:
    sys.path.insert(0, str(_SCRIPTS))
import confere_selo  # noqa: E402

CODIGOS_DA_CLASSE = {
    "nome": {"E0425", "E0412"},
    "import": {"E0432", "E0433"},
    "atributo": {"E0609", "E0599"},
    "kwarg": {"E0560", "E0063"},
}


def _sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def _teto(contagem: collections.Counter, total: int, teto: float, rotulo: str) -> list[str]:
    return [f"teto: {rotulo} crate {c}: {n} de {total} (> {int(teto * total)})" for c, n in contagem.items() if n > int(teto * total)]


def _positivo(e: dict, corpus: Path, raiz: Path, rel: str) -> list[str]:
    v = []
    disco = raiz / rel
    if not disco.is_file() or _sha(disco.read_bytes()) != e.get("sha_original"):
        v.append(f"{e['id']}: sha_original não bate com o arquivo atual do worktree ({rel})")
    snap = corpus / e["snap"]
    if not snap.is_file() or _sha(snap.read_bytes()) != e.get("snap_sha256"):
        v.append(f"{e['id']}: snap_sha256 não bate com o snap")
    base = corpus / "base" / f"{e['id']}.rs"
    if not base.is_file() or _sha(base.read_bytes()) != e.get("sha_original"):
        v.append(f"{e['id']}: base/ não é o original (sha_original)")
    if e.get("codigo_esperado") not in CODIGOS_DA_CLASSE.get(e.get("classe", ""), set()):
        v.append(f"{e['id']}: código {e.get('codigo_esperado')} fora da classe {e.get('classe')}")
    if not e.get("mensagem", "").startswith(f"error[{e.get('codigo_esperado')}]"):
        v.append(f"{e['id']}: mensagem não traz o código {e.get('codigo_esperado')}")
    return v


def _negativo(e: dict, corpus: Path, raiz: Path, rel: str) -> list[str]:
    snap, disco = corpus / e["snap"], raiz / rel
    v = []
    if not snap.is_file() or not disco.is_file() or snap.read_bytes() != disco.read_bytes():
        v.append(f"{e['id']}: negativo difere do arquivo real ({rel})")
    if snap.is_file() and _sha(snap.read_bytes()) != e.get("snap_sha256"):
        v.append(f"{e['id']}: snap_sha256 não bate com o snap do negativo")
    return v


def verificar(corpus: Path, raiz: Path, dev: set[str], controle: set[str], minimo_por_classe: int, minimo_negativos: int, teto: float, teto_negativos: float | None = None) -> list[str]:
    entradas = [json.loads(x) for x in (corpus / "entradas.jsonl").read_text().splitlines() if x.strip()]
    v: list[str] = []
    ids = collections.Counter(e["id"] for e in entradas)
    v += [f"id duplicado: {i}" for i, n in ids.items() if n > 1]
    prefixo = str(raiz) + "/"
    rels = {e["id"]: e["path"][len(prefixo) :] if e["path"].startswith(prefixo) else e["path"] for e in entradas}
    for e in entradas:
        rel = rels[e["id"]]
        if m := sel.motivo_exclusao(rel, dev, controle):
            v.append(f"{e['id']}: arquivo excluído ({m}): {rel}")
        v += _positivo(e, corpus, raiz, rel) if e["rotulo"] == "positivo" else _negativo(e, corpus, raiz, rel)
    paths = collections.Counter(rels.values())
    v += [f"arquivo repetido ({n}x): {p}" for p, n in paths.items() if n > 1]
    pos = {rels[e["id"]] for e in entradas if e["rotulo"] == "positivo"}
    neg = {rels[e["id"]] for e in entradas if e["rotulo"] == "negativo"}
    v += [f"arquivo em positivo e negativo: {p}" for p in sorted(pos & neg)]
    por_classe = collections.Counter(e["classe"] for e in entradas if e["rotulo"] == "positivo")
    v += [f"classe {c}: n={por_classe[c]} abaixo de {minimo_por_classe}" for c in CODIGOS_DA_CLASSE if por_classe[c] < minimo_por_classe]
    if len(neg) < minimo_negativos:
        v.append(f"negativos: {len(neg)} abaixo do mínimo {minimo_negativos}")
    for c in CODIGOS_DA_CLASSE:
        crates = collections.Counter(e["crate"] for e in entradas if e["rotulo"] == "positivo" and e["classe"] == c)
        v += _teto(crates, por_classe[c], teto, f"classe {c}")
    crates_neg = collections.Counter(e["crate"] for e in entradas if e["rotulo"] == "negativo")
    v += _teto(crates_neg, len(neg), teto if teto_negativos is None else teto_negativos, "negativos")
    return v


def verificar_prefixo(pre_registro: Path, tamanho_original: int, sha_original: str) -> bool:
    """O pré-registro ORIGINAL é o prefixo do arquivo atual: adendos só acrescentam ao fim, nunca mexem nos bytes de antes."""
    dados = pre_registro.read_bytes()
    return len(dados) >= tamanho_original and _sha(dados[:tamanho_original]) == sha_original


def montar_selo(corpus: Path, pre_registro: Path, arquivos_gerador: list[Path], extra: dict) -> dict:
    arqs = [p for p in corpus.rglob("*") if p.is_file()]
    gerador = hashlib.sha256()
    for p in sorted(arquivos_gerador, key=lambda x: x.name):
        gerador.update(p.name.encode() + hashlib.sha256(p.read_bytes()).digest())
    return {
        "corpus": str(corpus), "sha_tree": confere_selo.sha_tree(corpus), "arquivos": len(arqs), "bytes": sum(p.stat().st_size for p in arqs),
        "entradas_jsonl_sha256": _sha((corpus / "entradas.jsonl").read_bytes()), "holdout_meta_sha256": _sha((corpus / "holdout_meta.json").read_bytes()),
        "pre_registro_sha256": _sha(pre_registro.read_bytes()), "gerador_sha256": gerador.hexdigest(),
        "selado": "o motor e o juiz NAO rodaram neste corpus; quem julga e a coordenadora, uma vez, com o binario integrado final (conferir o sha_tree antes: scripts/confere_selo.py)",
        **extra,
    }
