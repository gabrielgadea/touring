"""Roda o `main()` do juiz INTEIRO e grava o `elapsed_ms` do motor POR ID (forma do rodar_juiz_instrumentado.py, que
descarta o tempo). Serve a investigação do p95 do v2 do P0B no bak (175,6 ms > 150; o INT97 deu 137,7): qual entrada
ficou mais lenta. Nada do juiz é alterado: o wrapper só embrulha `rodar_motor`.

Uso: python3 -I rodar_juiz_tempos.py <juiz.py> <tempos.json> [args do juiz ...]
"""

import importlib.util
import json
import sys
from pathlib import Path

JUIZ, SAIDA, ARGS = Path(sys.argv[1]).resolve(), Path(sys.argv[2]), sys.argv[3:]

spec = importlib.util.spec_from_file_location("juiz_sob_teste", JUIZ)
juiz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(juiz)

tempos: dict[str, dict] = {}
original = juiz.rodar_motor


def rodar_motor_cronometrando(touring, entrada, corpus):
    saida = original(touring, entrada, corpus)
    tempos[entrada["id"]] = {
        "ms": saida.get("elapsed_ms"),
        "classe": entrada.get("classe"),
        "rotulo": entrada.get("rotulo"),
        "origem": entrada.get("origem"),
        "verdict": saida.get("verdict"),
    }
    return saida


juiz.rodar_motor = rodar_motor_cronometrando
sys.argv = [str(JUIZ), *ARGS]
rc = juiz.main()
SAIDA.write_text(json.dumps({"juiz": str(JUIZ), "tempos": tempos}, ensure_ascii=False, sort_keys=True), encoding="utf-8")
print(f"[tempos] {len(tempos)} chamadas cronometradas em {SAIDA}")
raise SystemExit(rc)
