"""HORUST-REAL / estrato cfg — integridade do corpus ANTES de selar. Cada propriedade do adendo vira uma verificação que devolve a violação
por extenso; lista vazia = íntegro. Diferente do principal: o negativo é o ORIGINAL do positivo (par), então o mesmo arquivo aparece nos dois
lados por construção, e o snap do positivo tem de ser o original MAIS uma linha `#[cfg(test)]` acima da função nomeada.
"""

from __future__ import annotations

import collections
import hashlib
import json
from pathlib import Path

import operador_cfg as oc_
import selecao_horust as sel

ESTRATO = "horust-real-cfg"


def _sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def piloto(n_positivos: int, minimo: int) -> bool:
    """Abaixo do mínimo o estrato é selado mesmo assim, mas declarado piloto: sem taxa (adendo, seção 5)."""
    return n_positivos < minimo


def _insercao(velho: list[str], novo: list[str]) -> int | None:
    """Índice em que `novo` é `velho` mais UMA linha `#[cfg(test)]`; None se for qualquer outra coisa."""
    if len(novo) != len(velho) + 1:
        return None
    k = next((i for i in range(len(velho)) if velho[i] != novo[i]), len(velho))
    return k if novo[k] == oc_.LINHA_CFG and novo[k + 1 :] == velho[k:] else None


def _positivo(e: dict, corpus: Path, raiz: Path, rel: str) -> list[str]:
    ident, v = e["id"], []
    if e.get("classe") != oc_.CODIGO_CFG[0]:
        v.append(f"{ident}: classe {e.get('classe')!r} (esperado 'cfg')")
    if e.get("codigo_esperado") != oc_.CODIGO_CFG[1] or not e.get("mensagem", "").startswith(f"error[{oc_.CODIGO_CFG[1]}]"):
        v.append(f"{ident}: o código esperado tem de ser {oc_.CODIGO_CFG[1]} (código {e.get('codigo_esperado')}, mensagem {e.get('mensagem', '')[:30]!r})")
    disco = raiz / rel
    if not disco.is_file() or _sha(disco.read_bytes()) != e.get("sha_original"):
        v.append(f"{ident}: sha_original não bate com o arquivo atual do worktree ({rel})")
    snap, base = corpus / e["snap"], corpus / "base" / f"{ident}.rs"
    if not snap.is_file() or _sha(snap.read_bytes()) != e.get("snap_sha256"):
        v.append(f"{ident}: snap_sha256 não bate com o snap")
    if not base.is_file() or _sha(base.read_bytes()) != e.get("sha_original"):
        v.append(f"{ident}: base/ não é o original (sha_original)")
    if v or not snap.is_file() or not base.is_file():
        return v
    velho, novo = base.read_text().split("\n"), snap.read_text().split("\n")
    k = _insercao(velho, novo)
    if k is None:
        return [*v, f"{ident}: o snap não é o original mais UMA linha {oc_.LINHA_CFG} (cfg)"]
    m = oc_._DEF.match(velho[k])
    if not m or m.group(1) != e.get("nome_esperado"):
        v.append(f"{ident}: o cfg está acima de {velho[k]!r}, não da função {e.get('nome_esperado')!r}")
    linha = e.get("linha", 0)
    if not 1 <= linha <= len(novo) or f"{e.get('nome_esperado')}(" not in novo[linha - 1]:
        v.append(f"{ident}: a linha do erro ({linha}) não traz a chamada de {e.get('nome_esperado')}")
    return v


def _negativo(e: dict, corpus: Path, raiz: Path, rel: str, positivos: dict[str, dict]) -> list[str]:
    ident, v = e["id"], []
    par = positivos.get(e.get("pareado_com", ""))
    if par is None:
        return [f"{ident}: pareado_com {e.get('pareado_com')!r} não é um positivo do corpus (par ausente)"]
    snap, base = corpus / e["snap"], corpus / "base" / f"{par['id']}.rs"
    if not snap.is_file() or _sha(snap.read_bytes()) != e.get("snap_sha256"):
        v.append(f"{ident}: snap_sha256 não bate com o snap do negativo")
    if not snap.is_file() or not base.is_file() or snap.read_bytes() != base.read_bytes():
        v.append(f"{ident}: o negativo não é o original do seu par {par['id']}")
    disco = raiz / rel
    if not disco.is_file() or not snap.is_file() or snap.read_bytes() != disco.read_bytes():
        v.append(f"{ident}: negativo difere do arquivo real ({rel})")
    if e["path"] != par["path"]:
        v.append(f"{ident}: o par {par['id']} aponta para outro arquivo")
    return v


def verificar(corpus: Path, raiz: Path, dev: set[str], controle: set[str], principal: set[str], minimo: int, maximo: int, teto: float) -> list[str]:
    entradas = [json.loads(x) for x in (corpus / "entradas.jsonl").read_text().splitlines() if x.strip()]
    v: list[str] = []
    v += [f"id duplicado: {i}" for i, n in collections.Counter(e["id"] for e in entradas).items() if n > 1]
    v += [f"{e['id']}: estrato {e.get('estrato')!r} (esperado {ESTRATO!r})" for e in entradas if e.get("estrato") != ESTRATO]
    prefixo = str(raiz) + "/"
    rels = {e["id"]: e["path"][len(prefixo) :] if e["path"].startswith(prefixo) else e["path"] for e in entradas}
    positivos = {e["id"]: e for e in entradas if e["rotulo"] == "positivo"}
    for e in entradas:
        rel = rels[e["id"]]
        if m := sel.motivo_exclusao(rel, dev, controle):
            v.append(f"{e['id']}: arquivo excluído ({m}): {rel}")
        if rel in principal:
            v.append(f"{e['id']}: arquivo do corpus principal ou tentado nele: {rel}")
        v += _positivo(e, corpus, raiz, rel) if e["rotulo"] == "positivo" else _negativo(e, corpus, raiz, rel, positivos)
    pos_rels = collections.Counter(rels[i] for i in positivos)
    v += [f"arquivo com mais de um positivo ({n}x): {p}" for p, n in pos_rels.items() if n > 1]
    pareados = collections.Counter(e["pareado_com"] for e in entradas if e["rotulo"] == "negativo")
    v += [f"positivo {i} sem negativo pareado" for i in positivos if pareados[i] == 0]
    v += [f"positivo {i} com {n} negativos pareados" for i, n in pareados.items() if n > 1 and i in positivos]
    if len(positivos) > maximo:
        v.append(f"máximo: {len(positivos)} positivos acima de {maximo}")
    crates = collections.Counter(e["crate"] for e in positivos.values())
    v += [f"teto: crate {c}: {n} de {len(positivos)} (> {int(teto * len(positivos))})" for c, n in crates.items() if n > int(teto * len(positivos))]
    return v
