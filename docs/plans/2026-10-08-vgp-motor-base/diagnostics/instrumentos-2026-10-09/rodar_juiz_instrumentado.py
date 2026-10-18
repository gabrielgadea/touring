"""Roda o `main()` de um juiz (qualquer versao de `juiz_motor_vgp.py`) INTEIRO — argparse, impressao, JSON, exit code — e
grava, ao lado, o veredito do motor POR ID (o JSON do juiz so traz os ids que viraram falso deny / perdido / etc.).
Serve a prova de nao-regressao: juiz antigo e juiz novo, mesmo corpus, mesmo binario -> o dict por id tem de ser igual.

Nada do juiz e alterado: o wrapper so embrulha `rodar_motor` (o ponto unico onde o juiz chama o motor) para registrar
{id: saida do motor sem `elapsed_ms`}.

Uso: python3 -I rodar_juiz_instrumentado.py <juiz.py> <porid.json> [args do juiz ...]
"""

import importlib.util
import json
import sys
from pathlib import Path

JUIZ, POR_ID, ARGS = Path(sys.argv[1]).resolve(), Path(sys.argv[2]), sys.argv[3:]

spec = importlib.util.spec_from_file_location("juiz_sob_teste", JUIZ)
juiz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(juiz)

registro: dict[str, dict] = {}
original = juiz.rodar_motor


def rodar_motor_registrando(touring, entrada, corpus):
    saida = original(touring, entrada, corpus)
    registro[entrada["id"]] = {k: v for k, v in saida.items() if k != "elapsed_ms"}
    return saida


juiz.rodar_motor = rodar_motor_registrando
sys.argv = [str(JUIZ), *ARGS]
rc = juiz.main()
POR_ID.write_text(json.dumps({"juiz": str(JUIZ), "chamadas": len(registro), "por_id": registro}, ensure_ascii=False, sort_keys=True), encoding="utf-8")
print(f"[instrumentado] {len(registro)} veredictos do motor gravados em {POR_ID}")
raise SystemExit(rc)
