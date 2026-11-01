#!/usr/bin/env python3
"""w43_calibrate — temperatura do aluno ajustada no VAL split (S-4.3).

O trainer calibrou em 56 itens do próprio treino (ECE 0,206 no teste — acima do
piso 0,10). A receita do finetune.md é explícita: temperatura se ajusta em fatia
HELD-OUT — o nosso val split (120 itens, intocado pelo treino, seed 20260922).
Reusa `build_model`/`collate`/`fit_temperature` do notebook MPS: logits do val →
LBFGS no log-temperature → grava em rl_agent_config.json (posição choice).
"""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

import torch
from safetensors.torch import load_file
from transformers import AutoTokenizer

MPS_SCRIPT = Path("/tmp/laya-repo/notebooks/laya_finetune_typed_decisions_mps.py")
WORK = Path("~/.claude/touring/laya-w42-intent").expanduser()
OUT = WORK / "output"
# QTYPES do MPS: índice 0 = choice (as amostras do trainer foram [choice, score, noul])
CHOICE_IDX = 0


def main() -> int:
    spec = importlib.util.spec_from_file_location("mps_finetune", MPS_SCRIPT)
    mps = importlib.util.module_from_spec(spec)
    sys.modules["mps_finetune"] = mps
    spec.loader.exec_module(mps)

    cfg = json.loads((OUT / "rl_agent_config.json").read_text())
    tokenizer = AutoTokenizer.from_pretrained(OUT / "tokenizer")
    model = mps.build_model(cfg, encoder_dir=OUT / "encoder")
    model.load_state_dict(load_file(str(OUT / "model.safetensors")), strict=True)
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    model.to(device).eval()

    items = torch.load(WORK / "items_val.pt", map_location="cpu", weights_only=False)
    samples: list = []
    with torch.no_grad():
        for start in range(0, len(items), 8):
            chunk = items[start : start + 8]
            ids, attention, positions, mask, _t, qtype = mps.collate(chunk, tokenizer.pad_token_id)
            logits, _ = model(ids.to(device), attention.to(device), positions.to(device), mask.to(device), qtype.to(device))
            for i, item in enumerate(chunk):
                if item["qtype"] == CHOICE_IDX:
                    samples.append((logits[i, : len(item["markers"])].cpu(), item["target"]))
    print(f"amostras de calibração (choice): {len(samples)}")

    before = cfg.get("temperature", [1.2, 1.2, 1.2])
    fitted = mps.fit_temperature(samples)
    temps = list(before)
    temps[CHOICE_IDX] = fitted
    cfg["temperature"] = temps
    cfg.pop("temperature_by_options", None)  # nunca mascara o novo fit (finetune.md)
    for path in (OUT / "rl_agent_config.json", OUT / "checkpoint_latest" / "rl_agent_config.json"):
        if path.exists():
            path.write_text(json.dumps(cfg, indent=2))
    print(json.dumps({"before": before[CHOICE_IDX], "fitted": fitted, "temps": temps}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
