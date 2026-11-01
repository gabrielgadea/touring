#!/usr/bin/env python3
"""reconcilia_int97i.py — every per-id divergence of the integrated release against INT97 must have a cause.

The integrated tree is INT97 + P0B + FRAG + P0B-d + routing + the ECC patches. Each of the first ones was judged on its
own (P0B acceptance, FRAG base x frag), so a verdict that changed between INT97 and the integrated release is
EXPLAINED when the same change, in the same direction, already shows in one of those isolated runs, or when it was
predicted before the run (`--previsto id:de:para`, e.g. P0B-d's d94f8090d150 warn -> deny). Anything left is a finding:
an interaction between changes, or an effect nobody judged (the heredoc fix of the ECC patches reaches the motor
through vgp_execucao.rs).

Comparison = the R1 ruler of comparar_porid2.py: verdict + sorted (class, name, line, severity) of the findings.

Usage:
  reconcilia_int97i.py --int INT.porid.json --novo NOVO.porid.json [--p0b P0B.porid.json]
                       [--frag-base BASE.porid.json --frag FRAG.porid.json] [--previsto id:de:para ...] --json OUT
Exit: 0 every divergence explained · 1 some divergence has no cause · 3 an input is missing or unreadable.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def ler(caminho: str) -> dict[str, dict]:
    """Per-id records, from both shapes in use: {"por_id": {...}} (juizes) or {id: record} (FRAG)."""
    dado = json.loads(Path(caminho).read_text(encoding="utf-8"))
    return dado["por_id"] if isinstance(dado, dict) and "por_id" in dado else dado


def r1(rec: dict | None) -> tuple | None:
    if rec is None:
        return None
    if "_erro" in rec:
        return ("erro", rec["_erro"])
    achados = sorted(
        (f.get("classe"), f.get("nome"), f.get("linha"), f.get("severidade")) for f in rec.get("findings") or []
    )
    return (rec.get("verdict"), tuple(achados))


def mudou_igual(antes: dict, depois: dict, ident: str, alvo: tuple | None) -> bool:
    """The isolated run changed this id (antes != depois) and landed exactly where the integrated release did."""
    a, d = r1(antes.get(ident)), r1(depois.get(ident))
    return a != d and d == alvo


TIMEOUT_CAMADA2 = "unknown: prazo de"


def _so_perdeu(respondeu: dict, estourou: dict) -> bool:
    """`respondeu` got a layer-2 answer, `estourou` timed out, and the timed-out side only LACKS findings."""
    if respondeu.get("camada2") is not None or not str(estourou.get("camada2") or "").startswith(TIMEOUT_CAMADA2):
        return False
    return _assinatura(estourou) < _assinatura(respondeu)


def so_perdeu_por_timeout(rec_base: dict | None, rec_novo: dict | None) -> bool:
    """Layer 2 timed out on ONE side (either run) and that side only lost findings the answering side has.

    A mechanical cause, not a root cause: whether timeouts follow the binary or the machine is a separate question,
    answered by running both binaries interleaved in one machine window (timeouts_v3_abba.sh, 18/10/2026: they
    followed the machine). The base can be the side that timed out: INT97's own run lost findings too."""
    if not rec_base or not rec_novo:
        return False
    return _so_perdeu(rec_base, rec_novo) or _so_perdeu(rec_novo, rec_base)


def _assinatura(rec: dict) -> set[tuple]:
    return {(f.get("classe"), f.get("nome"), f.get("linha")) for f in rec.get("findings") or []}


def _previsto(ident: str, base: tuple | None, novo: tuple | None, previstos: dict, fontes: dict) -> bool:
    """A prediction is written against the state it was made on: P0B-d's against P0B, when the P0B run is given."""
    prev = previstos.get(ident)
    de = r1(fontes["p0b"][ident]) if ident in fontes.get("p0b", {}) else base
    return bool(prev) and (de or (None,))[0] == prev[0] and (novo or (None,))[0] == prev[1]


def causa(ident: str, base: tuple | None, novo: tuple | None, fontes: dict, previstos: dict) -> str | None:
    """The first cause that explains the divergence, in a fixed order: isolated runs, prediction, layer-2 timeout."""
    checagens = (
        ("P0B", lambda: "p0b" in fontes and r1(fontes["p0b"].get(ident)) == novo and novo != base),
        ("FRAG", lambda: "frag" in fontes and mudou_igual(fontes["frag_base"], fontes["frag"], ident, novo)),
        ("previsto ({} -> {})".format(*previstos.get(ident, ("", ""))), lambda: _previsto(ident, base, novo, previstos, fontes)),
        ("camada 2 sem resposta (timeout): só perdeu achados",
         lambda: so_perdeu_por_timeout(fontes["_int"].get(ident), fontes["_novo"].get(ident))),
    )
    return next((nome for nome, ok in checagens if ok()), None)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--int", dest="int_", required=True)
    ap.add_argument("--novo", required=True)
    ap.add_argument("--p0b")
    ap.add_argument("--frag-base")
    ap.add_argument("--frag")
    ap.add_argument("--previsto", action="append", default=[], help="id:de:para (verdicts)")
    ap.add_argument("--json", required=True)
    a = ap.parse_args(argv)
    if bool(a.frag_base) != bool(a.frag):
        ap.error("--frag-base and --frag go together")
    try:
        base, novo = ler(a.int_), ler(a.novo)
        fontes = {k: ler(v) for k, v in (("p0b", a.p0b), ("frag_base", a.frag_base), ("frag", a.frag)) if v}
    except (OSError, ValueError, KeyError) as exc:
        print(f"entrada ilegível: {exc}", file=sys.stderr)
        return 3
    previstos = {p.split(":")[0]: tuple(p.split(":")[1:3]) for p in a.previsto}
    fontes |= {"_int": base, "_novo": novo}
    divergencias = diverge(base, novo, fontes, previstos)
    saida = resumo(base, novo, divergencias, previstos)
    Path(a.json).write_text(json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps({k: saida[k] for k in ("ids_int97", "ids_integrado", "divergencias", "por_causa",
                                            "previstos_observados")}, ensure_ascii=False))
    return 1 if saida["sem_causa"] else 0


def diverge(base: dict, novo: dict, fontes: dict, previstos: dict) -> list[dict]:
    """Every id whose R1 signature differs between INT97 and the integrated release, with its cause (or None)."""
    out = []
    for ident in sorted(set(base) | set(novo)):
        rb, rn = r1(base.get(ident)), r1(novo.get(ident))
        if rb != rn:
            out.append({"id": ident, "int97": rb and rb[0], "integrado": rn and rn[0],
                        "achados_int97": rb and len(rb[1]), "achados_integrado": rn and len(rn[1]),
                        "causa": causa(ident, rb, rn, fontes, previstos)})
    return out


def resumo(base: dict, novo: dict, divergencias: list[dict], previstos: dict) -> dict:
    por_causa: dict[str, int] = {}
    for d in divergencias:
        chave = d["causa"] or "SEM CAUSA"
        por_causa[chave] = por_causa.get(chave, 0) + 1
    vistos = {d["id"] for d in divergencias}
    return {"ids_int97": len(base), "ids_integrado": len(novo), "divergencias": len(divergencias),
            "por_causa": por_causa, "previstos_observados": {i: i in vistos for i in previstos},
            "sem_causa": [d for d in divergencias if d["causa"] is None], "todas": divergencias}


if __name__ == "__main__":
    sys.exit(main())
