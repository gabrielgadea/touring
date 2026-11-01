"""Como rodar_juiz_instrumentado.py, mas grava o elapsed_ms do motor POR ID (para achar a cauda do p95).

Uso: python3 -I juiz_tempos.py <juiz.py> <saida.json> [args do juiz ...]
"""

import importlib.util
import json
import sys
from pathlib import Path

JUIZ, SAIDA, ARGS = Path(sys.argv[1]).resolve(), Path(sys.argv[2]), sys.argv[3:]
spec = importlib.util.spec_from_file_location("juiz_sob_teste", JUIZ)
juiz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(juiz)

tempos: dict[str, float] = {}
camada2: dict[str, str | None] = {}
original = juiz.rodar_motor


def rodar_motor_cronometrado(touring, entrada, corpus):
    saida = original(touring, entrada, corpus)
    tempos[entrada["id"]] = saida.get("elapsed_ms")
    camada2[entrada["id"]] = saida.get("camada2")
    return saida


juiz.rodar_motor = rodar_motor_cronometrado
sys.argv = [str(JUIZ), *ARGS]
rc = juiz.main()
SAIDA.write_text(json.dumps(tempos, sort_keys=True), encoding="utf-8")
SAIDA.with_suffix(".camada2.json").write_text(json.dumps(camada2, sort_keys=True), encoding="utf-8")
raise SystemExit(rc)
