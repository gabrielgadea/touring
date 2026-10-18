"""HORUST-REAL — «abrir 3 antes de agregar»: de cada classe, 3 positivos escolhidos por hash (determinístico, sem olhar o resultado), com a linha
original, a linha injetada, a mensagem do rustc e 2 linhas de contexto, para LEITURA HUMANA. Nenhum motor roda aqui.

Uso: python3 -I -B abrir_tres.py <corpus_dir> [classe ...]   (sem classes: as 4 do corpus principal)
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import selecao_horust as sel  # noqa: E402

POR_CLASSE = 3


def escolhidos(entradas: list[dict], classe: str) -> list[dict]:
    positivos = [e for e in entradas if e["rotulo"] == "positivo" and e["classe"] == classe]
    return sorted(positivos, key=lambda e: hashlib.sha256(f"abrir3|{e['id']}".encode()).hexdigest())[:POR_CLASSE]


def trecho(texto: str, linha: int, janela: int = 2) -> list[str]:
    linhas = texto.splitlines()
    return [f"{i + 1:5d}{'>' if i + 1 == linha else ' '}| {linhas[i]}" for i in range(max(0, linha - 1 - janela), min(len(linhas), linha + janela))]


def main() -> int:
    corpus = Path(sys.argv[1])
    entradas = [json.loads(x) for x in (corpus / "entradas.jsonl").read_text().splitlines() if x.strip()]
    for classe in sys.argv[2:] or list(sel.PESOS):
        for e in escolhidos(entradas, classe):
            novo, velho = (corpus / e["snap"]).read_text(), (corpus / e["base"]).read_text()
            print(f"== {classe} {e['id']} {e['operador']} {e['codigo']} crate={e['crate']} {e['path'].split('/crates/')[-1]}:{e['linha']}")
            print(f"   {e['nome_original']} -> {e['nome_esperado']} | {e['mensagem'][:160]}")
            print("   ORIGINAL"), print("\n".join("   " + x for x in trecho(velho, e["linha"])))
            print("   INJETADO"), print("\n".join("   " + x for x in trecho(novo, e["linha"])))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
