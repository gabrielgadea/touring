#!/usr/bin/env python3
# #tags: kind:script purpose:replay-periodico domain:code-mode
"""W7 S-7.1 — replay dos instrumentos contra a baseline congelada (S-0.2).

Roda diag_gate.py, extrai as métricas-chave (o mesmo extrator do freeze) e
compara com data/baseline-pre-w1.json. Emite FACT= por métrica e
NEW_FINDINGS=<n de deltas significativos> — o contrato do loop dry do scout.
"""
import json, subprocess, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from freeze_baseline import extract_key_metrics  # noqa: E402

SIGNIFICANCIA = 0.15  # delta relativo que conta como finding

def main() -> int:
    proc = subprocess.run([sys.executable, str(HERE / "diag_gate.py")],
                          capture_output=True, text=True, timeout=600)
    if proc.returncode != 0:
        print("NEW_FINDINGS=0")
        print(f"FACT=REPLAY_ERRO=exit_{proc.returncode}")
        return 0
    atual = extract_key_metrics(proc.stdout)
    base_path = HERE.parent / "data" / "baseline-pre-w1.json"
    base = json.loads(base_path.read_text())
    base_km = base["instruments"].get("diag_gate", {}).get("key_metrics", {})
    findings = 0
    for chave, valor in sorted(atual.items()):
        print(f"FACT={chave.upper()}={valor}")
        antes = base_km.get(chave)
        if isinstance(antes, (int, float)) and antes:
            delta = (valor - antes) / antes
            if abs(delta) >= SIGNIFICANCIA:
                findings += 1
                print(f"FACT=DELTA_{chave.upper()}={delta:+.0%}")
    if not atual:
        # instrumento mudou de formato: isso É um finding, nunca silêncio
        findings += 1
        print("FACT=REPLAY_SEM_METRICAS=formato_do_relatorio_mudou")
    print(f"NEW_FINDINGS={findings}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
