"""Helpers shared by the r3 replays (`r3_replay_a1.py`, `r3_replay_a2.py`).

Both replays budget their API calls with the same append-only prediction cache and read the
same two answer wire formats; those pieces were pasted into each. They live here once. The
cache VERSION stays a per-replay constant (`VERSAO_CACHE`) and is passed in, so bumping it in
one replay still invalidates only that replay's cache.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path


def chave_cache(versao: int, arm: str, chave: str, spec: str) -> str:
    """A chave de uma predição: braço + item + VERSÃO da pergunta.

    Inclui o `spec` (a pergunta) para que mudar a pergunta invalide o cache em vez
    de devolver a predição da pergunta antiga — o re-run sem mudança de entrada
    tem de dar 0 chamadas, e o re-run com a pergunta mudada tem de dar N.
    """
    return f"{versao}|{arm}|{chave}|{hashlib.sha256(spec.encode()).hexdigest()[:10]}"


def carregar_cache(caminho: Path) -> dict:
    if not caminho.exists():
        return {}
    out = {}
    for l in caminho.read_text(encoding="utf-8").splitlines():
        if not l.strip():
            continue
        try:
            r = json.loads(l)
        except ValueError:
            continue  # cache truncado: descarta a linha, não o arquivo
        out[r["cache_key"]] = r["pred"]
    return out


def registrar_cache(caminho: Path, cache_key: str, pred: dict) -> None:
    caminho.parent.mkdir(parents=True, exist_ok=True)
    with open(caminho, "a", encoding="utf-8") as f:
        f.write(json.dumps({"cache_key": cache_key, "pred": pred}, ensure_ascii=False) + "\n")


def corpo_da_resposta(corpo, qid: str) -> dict | None:
    """O corpo da resposta como dict, ou None se não for um.

    `DecisionResult` (laya) é um objeto com `.values`/`.answers`; o Jev HTTP é um dict.
    Os dois formatos, a mesma extração.
    """
    for atr in ("answers", "values"):
        sub = getattr(corpo, atr, None)
        if isinstance(sub, dict) and qid in sub:
            corpo = {qid: sub[qid]}
            break
    return corpo if isinstance(corpo, dict) else None


def valor_da_resposta(ans) -> float:
    """O número de UMA resposta: o escalar, ou a primeira chave numérica entre
    `noul`/`max_prob`/`score`; 0.0 se não houver."""
    if isinstance(ans, (int, float)) and not isinstance(ans, bool):
        return float(ans)
    if not isinstance(ans, dict):
        return 0.0
    for chave in ("noul", "max_prob", "score"):
        v = ans.get(chave)
        if isinstance(v, (int, float)) and not isinstance(v, bool):
            return float(v)
    return 0.0
