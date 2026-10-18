"""HORUST-REAL — confere, SEM tocar no corpus selado, que os negativos são disjuntos de TODO arquivo tentado nas rodadas de geração (1, 2 e a
revalidação regrava os mesmos ids): pré-registro §5. Só leitura. Grava `negativos_tentados.json` ao lado do relatório.

Uso: python3 -I -B verificar_negativos_tentados.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def negativos_tentados(corpus: Path, raiz: Path, tentados: set[str]) -> list[str]:
    prefixo = str(raiz) + "/"
    saida = []
    for linha in (corpus / "entradas.jsonl").read_text().splitlines():
        e = json.loads(linha) if linha.strip() else None
        if e and e["rotulo"] == "negativo":
            rel = e["path"][len(prefixo) :] if e["path"].startswith(prefixo) else e["path"]
            if rel in tentados:
                saida.append(f"{e['id']}: negativo é um arquivo tentado em alguma rodada de geração ({rel})")
    return saida


def resumo(corpus: Path, raiz: Path, tentados: set[str]) -> dict:
    entradas = [json.loads(x) for x in (corpus / "entradas.jsonl").read_text().splitlines() if x.strip()]
    v = negativos_tentados(corpus, raiz, tentados)
    return {"negativos": sum(e["rotulo"] == "negativo" for e in entradas), "arquivos_tentados": len(tentados), "violacoes": len(v), "detalhe": v[:10]}


def main() -> int:
    import construir_horust_real as cr

    tentados = {r["rel"] for r in cr.ler_registros()}
    r = resumo(cr.CORPUS, cr.RAIZ, tentados)
    (HERE / "negativos_tentados.json").write_text(json.dumps(r, ensure_ascii=False, indent=1))
    print(json.dumps(r, ensure_ascii=False))
    return 0 if r["violacoes"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
