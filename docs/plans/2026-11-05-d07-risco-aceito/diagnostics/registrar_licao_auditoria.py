"""Persiste a lição da auditoria independente do D07 com leitura de volta."""

from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(os.environ.get("ANALISE_ROOT", "/home/gabrielgadea/projects/analise"))
sys.path.insert(0, str(ROOT))

from scripts.memoria.grafo_memoria import No, gravar


def main() -> None:
    node = No(
        tipo="licao",
        slug="auditoria-independente-testa-fronteira-de-parser-e-calendario",
        corpo=(
            "Sessão analise-36, D07: a suíte verde inicial não cobria três bordas. "
            "Uma tabela TOML simples [em_onda] herdava o [[aceito]] anterior; aceite sem "
            "motivo produzia evidência vazia; UTC antecipava o vencimento em São Paulo. "
            "Os três testes semeados fizeram RED 16 passed/3 failed e GREEN 19 passed/0 "
            "failed; a suíte final fechou 518 unitários + 27 integração/doc, e a reauditoria "
            "devolveu FULFILLS sem findings. Toda seção termina em qualquer '[', motivo é "
            "obrigatório e a data é derivada com offset UTC-03:00."
        ),
        facetas={
            "kind": "lesson",
            "domain": "security",
            "process": "verification",
            "status": "verified",
        },
        procedencia=("decomp:task_1793900961378191950::D07",),
    )
    receipt = gravar(node)
    print(receipt.json(), flush=True)
    if not receipt.integro:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
