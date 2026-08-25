#!/usr/bin/env python3
"""S-0.2 — congela a baseline pré-W1 em data/baseline-pre-w1.json.

Roda os instrumentos da pasta evidence/ (diag_tools, diag_gate, diag_mem,
diag_gate_sim) via subprocess direto (fora do sandbox: este script É o
agregador; os instrumentos são read-only por contrato declarado no README)
e congela um envelope único com schema_version. O "antes" do delta R5 do
plano inteiro — W7 (replay) compara contra este arquivo.

Uso: python3 freeze_baseline.py [--out data/baseline-pre-w1.json]
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
INSTRUMENTS = ["diag_tools", "diag_gate", "diag_mem", "diag_gate_sim"]


def run_instrument(name: str) -> dict:
    """Executa um instrumento e congela o RELATÓRIO DE TEXTO que ele emite.

    Os instrumentos são repórteres de texto (medido 24/08 — a 1ª versão deste
    assembler supôs JSON e marcou os 4 como ERROR com os números certos no
    stdout). O texto integral é o artefato comparável; `key_metrics` extrai
    por regex os números que o plano cita, para o replay do W7 comparar barato.
    """
    proc = subprocess.run(
        [sys.executable, str(HERE / f"{name}.py")],
        capture_output=True,
        text=True,
        timeout=600,
    )
    if proc.returncode != 0:
        return {"error": f"exit {proc.returncode}", "stderr": proc.stderr[-800:]}
    text = proc.stdout.strip()
    if not text:
        return {"error": "empty stdout"}
    return {"report_text": text, "key_metrics": extract_key_metrics(text)}


import re

# métricas que o plano cita, por padrão casado com o formato REAL dos
# relatórios (verificado contra o texto congelado de 24/08, não suposto)
KEY_PATTERNS = {
    "read_sem_localizar": r"ReadSemLocalizar\s+(\d+)",
    "edit_sem_read": r"EditSemRead\s+(\d+)",
    "bash_classificados": r"BASH_CLASSIFICADOS=(\d+)",
    "code_mode_pct": r"code_mode\s+\d+\s+(\d+)%",
    "inspecao_pct": r"inspecao\s+\d+\s+(\d+)%",
    "memorias_total": r"MEMORIAS=(\d+)",
    "staleness_pct": r"staleness\s+\d+\s+(\d+)%",
    "instrumento_errado_pct": r"instrumento_errado\s+\d+\s+(\d+)%",
    "p9_pct": r"P9[^\d]*(\d+)\s*%",
    "g1_fires": r"G1\D+(\d+)",
    "g2_fires": r"G2\D+(\d+)",
    "maior_rajada": r"maior rajada\D+(\d+)",
}


def extract_key_metrics(text: str) -> dict:
    found = {}
    for key, pat in KEY_PATTERNS.items():
        m = re.search(pat, text, re.IGNORECASE)
        if m:
            found[key] = int(m.group(1))
    return found


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=str(HERE.parent / "data" / "baseline-pre-w1.json"))
    args = ap.parse_args()

    envelope: dict = {
        "schema_version": "baseline-pre-w1/1",
        "frozen_at": datetime.now(timezone.utc).isoformat(),
        "instruments": {},
    }
    failures = []
    for name in INSTRUMENTS:
        result = run_instrument(name)
        envelope["instruments"][name] = result
        if "error" in result:
            failures.append(name)

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(envelope, ensure_ascii=False, indent=1))

    # agregado curto para o contexto (R1): 1 linha por instrumento
    summary = {"out": str(out_path), "bytes": out_path.stat().st_size, "failures": failures}
    for name, res in envelope["instruments"].items():
        keys = [k for k in res.keys() if k != "error"][:6]
        summary[name] = "ERROR" if "error" in res else f"ok ({len(res)} chaves: {','.join(keys)})"
    print(json.dumps(summary, ensure_ascii=False, indent=1))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
