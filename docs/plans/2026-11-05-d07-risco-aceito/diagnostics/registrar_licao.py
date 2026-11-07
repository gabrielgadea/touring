"""Persiste as lições verificadas do D07 com leitura de volta."""

from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(os.environ.get("ANALISE_ROOT", "/home/gabrielgadea/projects/analise"))
sys.path.insert(0, str(ROOT))

from scripts.memoria.grafo_memoria import No, gravar


def main() -> None:
    nodes = [
        No(
            tipo="licao",
            slug="risco-aceito-exige-data-id-e-evidencia",
            corpo=(
                "Sessão analise-36, D07: ignorar vulnerabilidade sem declarar o motivo apaga a "
                "medição. O filtro F2.5 aceita apenas id/alias com hoje <= expira, lista pacote, "
                "data e controle, e mantém vencidos abertos. Testes cobrem fronteira 2026-12-05/06, "
                "id errado, alias, parser e arquivo ausente; 518 unitários + 27 integração/doc verdes."
            ),
            facetas={"kind": "lesson", "domain": "security", "process": "verification", "status": "verified"},
            procedencia=("decomp:task_1793900961378191950::D07",),
        ),
        No(
            tipo="diagnostico",
            slug="wrapper-antigo-evita-regressao-sem-arquivo-de-risco",
            corpo=(
                "Sessão analise-36, D07: manter score_pip_audit como wrapper sem riscos preservou "
                "a evidência anterior quando o arquivo não existe e evitou dead_code no Clippy. A "
                "prova real no analise passou Diamond 1.000 e listou PYSEC-2026-2447 até 2026-12-05, "
                "usando a cache OSV de 18 h, sem refresh de rede."
            ),
            facetas={"kind": "lesson", "domain": "security", "process": "verification", "status": "verified"},
            procedencia=("decomp:task_1793900961378191950::D07",),
        ),
    ]
    receipts = [gravar(node) for node in nodes]
    for receipt in receipts:
        print(receipt.json(), flush=True)
    if not all(receipt.integro for receipt in receipts):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
