#!/usr/bin/env python3
"""Calibra o τ de semelhança (SimHash 64 bits + hamming) para o gate de recriação.

Pergunta medida: qual limiar de distância separa "mesmo propósito reescrito"
(near-dup: renomear var, trocar ordem, adicionar linhas) de "programa diferente"?
Corpus: os exemplares campeões reais (champions_content/) + mutações sintéticas
determinísticas deles (rename de identificador, drop de comentários, shuffle de
linhas independentes) como pares positivos, e pares cruzados entre campeões
diferentes como negativos.

Saída: UM JSON com a matriz de distâncias e o τ recomendado.
"""
from __future__ import annotations

import json
import re
from pathlib import Path

CHAMPIONS = Path(__file__).resolve().parent / "champions_content"


def normalize(code: str) -> list[str]:
    """Tokens do corpo sem comentários/blank — a forma que a recriação preserva."""
    out = []
    for ln in code.splitlines():
        s = ln.strip()
        if not s or s.startswith(("#", "//")):
            continue
        out.extend(re.findall(r"[A-Za-z_]+|\d+|[^\sA-Za-z_\d]", s))
    return out


def simhash64(tokens: list[str]) -> int:
    """SimHash clássico: bit i = sinal da soma dos hashes dos shingles de 3 tokens."""
    if len(tokens) < 3:
        tokens = tokens + tokens  # corpos mínimos ainda produzem shingles
    v = [0] * 64
    for i in range(len(tokens) - 2):
        h = hash(" ".join(tokens[i:i + 3])) & 0xFFFFFFFFFFFFFFFF
        for b in range(64):
            v[b] += 1 if (h >> b) & 1 else -1
    return sum((1 << b) for b in range(64) if v[b] > 0)


def hamming(a: int, b: int) -> int:
    """Distância de Hamming entre dois simhashes de 64 bits."""
    return bin(a ^ b).count("1")


def mutacoes(code: str) -> dict[str, str]:
    """Pares positivos determinísticos: a recriação real muda nomes e ruído, não a espinha."""
    renomeado = re.sub(r"\bresultado\b|\bdata\b|\bitems\b", lambda m: {"resultado": "out", "data": "d", "items": "xs"}[m.group(0)], code)
    sem_comentarios = "\n".join(l for l in code.splitlines() if not l.strip().startswith("#"))
    linhas = [l for l in code.splitlines() if l.strip()]
    head, tail = linhas[: len(linhas) // 2], linhas[len(linhas) // 2:]
    tail_invertido = "\n".join(head + list(reversed(tail)))
    return {"renomeado": renomeado, "sem_comentarios": sem_comentarios, "tail_invertido": tail_invertido}


def main() -> None:
    """Mede distâncias intra-campeão (mutações) × inter-campeões e recomenda τ."""
    corpos = {f.stem: json.loads(f.read_text())["content"] for f in sorted(CHAMPIONS.glob("*.json"))}
    hashes = {k: simhash64(normalize(c)) for k, c in corpos.items()}

    positivos = []
    for nome, corpo in corpos.items():
        for kind, mut in mutacoes(corpo).items():
            positivos.append({"par": f"{nome}×{kind}", "dist": hamming(hashes[nome], simhash64(normalize(mut)))})
    negativos = []
    nomes = list(corpos)
    for i in range(len(nomes)):
        for j in range(i + 1, len(nomes)):
            negativos.append({"par": f"{nomes[i]}×{nomes[j]}", "dist": hamming(hashes[nomes[i]], hashes[nomes[j]])})

    pd = sorted(p["dist"] for p in positivos)
    nd = sorted(n["dist"] for n in negativos)
    print(json.dumps({
        "positivos_max": pd[-1], "positivos_p95": pd[int(len(pd) * 0.95)],
        "positivos_piores": sorted(positivos, key=lambda p: -p["dist"])[:5],
        "negativos_min": nd[0], "negativos_p5": nd[max(0, int(len(nd) * 0.05))],
        "negativos_mais_proximos": sorted(negativos, key=lambda n: n["dist"])[:5],
        "tau_recomendado": min(pd[-1] + 2, nd[0] - 1),
    }, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
