"""HORUST-REAL — determinismo do plano: refaz cada rodada gravada SEM oráculo (mesmos insumos, mesma semente, só os registros das rodadas
ANTERIORES) duas vezes e compara com os ids que `rodar`/`lotes` gravaram em `estado/resultados.jsonl`. Pré-registro, seção 7 (5).
Nenhum cargo roda aqui.

Uso: python3 -I -B determinismo_horust.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import construir_horust_real as cr  # noqa: E402


def ids_da_rodada(k: int, arquivos, tokens, registros: list[dict]) -> list[str]:
    anteriores = [r for r in registros if r["rodada"] < k]
    return [t.inj.id for t in cr.plano_da_rodada(k, arquivos, tokens, anteriores)]


def main() -> int:
    arquivos, _ = cr.montar_inventario()
    tokens = cr.tokens_por_pacote(arquivos)
    registros = cr.ler_registros()
    rodadas = sorted({r["rodada"] for r in registros})
    saida: dict = {"rodadas": {}}
    for k in rodadas:
        a, b = ids_da_rodada(k, arquivos, tokens, registros), ids_da_rodada(k, arquivos, tokens, registros)
        gravados = [r["id"] for r in registros if r["rodada"] == k]
        saida["rodadas"][str(k)] = {
            "refeito_duas_vezes_identico": a == b, "n_plano": len(a), "n_gravados": len(gravados),
            "gravados_iguais_ao_plano": sorted(a) == sorted(gravados), "so_no_plano": sorted(set(a) - set(gravados))[:10],
            "so_nos_gravados": sorted(set(gravados) - set(a))[:10],
        }
    saida["ok"] = bool(rodadas) and all(v["refeito_duas_vezes_identico"] and v["gravados_iguais_ao_plano"] for v in saida["rodadas"].values())
    (HERE / "determinismo.json").write_text(json.dumps(saida, indent=1))
    print(json.dumps(saida))
    return 0 if saida["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
