#!/usr/bin/env python3
"""w44_support_fit — temperature scaling para SUPORTE do aluno (S-1 do plano r2).

A curva do aluno é sub-confiante (suporte 22,5% @ τ=0,6, acc 0,926 — medido W4.2):
o logit drift pós-fine-tune achata a distribuição; a documentação oficial prescreve
UM escalar T<1 ajustado em fatia held-out. Diferente do w43 (LBFGS em NLL), aqui o
objetivo é o da decisão: **max suporte@τ s.t. precisão-no-suportado ≥ 0,90** — a
acurácia global é INVARIANTE sob T (argmax não muda); o que se move é quanto da
distribuição cruza o gate. Grade 1-D determinística (sem otimizador, sem aleatoriedade).
Reusa o caminho de logits do w43_calibrate.py (mesma val congelada, seed 20260922).
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
TAU = 0.6
MIN_PRECISION = 0.90
GRID = (0.4, 1.2, 0.02)


def grid_search_t(
    samples: list[tuple[torch.Tensor, int]],
    tau: float = TAU,
    min_precision: float = MIN_PRECISION,
    grid: tuple[float, float, float] = GRID,
) -> tuple[dict | None, list[dict]]:
    """Grade 1-D sobre T: (melhor ponto, curva completa).

    O melhor ponto maximiza suporte@τ com precisão-no-suportado ≥ min_precision;
    None quando nenhum ponto fecha o piso (declarado, nunca um T de mentira).
    A acurácia global é invariante sob T e sai na curva como prova.
    """
    lo, hi, step = grid
    n = len(samples)
    rows: list[dict] = []
    best: dict | None = None
    k = 0
    while True:
        t = lo + k * step
        if t > hi + 1e-9:
            break
        sup = corr_sup = corr_all = 0
        for logits, target in samples:
            probs = torch.softmax(logits.float() / t, dim=-1)
            p, pred = probs.max(dim=-1)
            if pred.item() == target:
                corr_all += 1
            if p.item() >= tau:
                sup += 1
                if pred.item() == target:
                    corr_sup += 1
        support = sup / n if n else 0.0
        precision = corr_sup / sup if sup else 0.0
        rows.append({
            "t": round(t, 4),
            "support": round(support, 4),
            "precision_supported": round(precision, 4),
            "accuracy": round(corr_all / n if n else 0.0, 4),
        })
        if sup and precision >= min_precision and (best is None or support > best["support"]):
            best = rows[-1]
        k += 1
    return best, rows


def _load_val_samples(split: str = "val") -> tuple[list[tuple[torch.Tensor, int]], dict]:
    """O caminho do w43: modelo + split congelada → [(logits_choice, label)].

    `split="test"` é DIAGNÓSTICO apenas — ajustar T na teste é leak (o juiz congelado
    nunca alimenta a decisão; a val é a fatia held-out legítima, receita finetune.md).
    """
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
    items = torch.load(WORK / f"items_{split}.pt", map_location="cpu", weights_only=False)
    samples: list[tuple[torch.Tensor, int]] = []
    with torch.no_grad():
        for start in range(0, len(items), 8):
            chunk = items[start : start + 8]
            ids, attention, positions, mask, _t, qtype = mps.collate(chunk, tokenizer.pad_token_id)
            logits, _ = model(ids.to(device), attention.to(device), positions.to(device), mask.to(device), qtype.to(device))
            for i, item in enumerate(chunk):
                if item["qtype"] == CHOICE_IDX:
                    # o alvo duro é item["label"]; item["target"] é a DISTRIBUIÇÃO
                    # soft RLCD (8 floats) — compará-la com o argmax dava acc=0,0
                    # (instrumento desalinhado, pego pelo próprio grid 01/10)
                    samples.append((logits[i, : len(item["markers"])].cpu(), item["label"]))
    return samples, cfg


def main() -> int:
    split = "test" if "--diagnose-test" in sys.argv else "val"
    samples, cfg = _load_val_samples(split)
    best, rows = grid_search_t(samples)
    temps_before = cfg.get("temperature", [1.2, 1.2, 1.2])
    current_t = temps_before[CHOICE_IDX]
    report = {
        "split": split,
        "samples": len(samples),
        "tau": TAU,
        "min_precision": MIN_PRECISION,
        "current_temperature": current_t,
        "baseline_at_current_t": min(rows, key=lambda r: abs(r["t"] - current_t)),
        "fitted": best,
        "curve": rows,
    }
    if split == "test":
        # diagnóstico: NUNCA escreve config (leak) — só imprime a curva do juiz
        print(json.dumps(report, indent=2))
        return 0
    if best is None:
        print(json.dumps(report, indent=2))
        print("NENHUM ponto fecha precisão ≥ alvo — T inalterada, revisão humana", file=sys.stderr)
        return 1
    temps = list(temps_before)
    temps[CHOICE_IDX] = best["t"]
    cfg["temperature"] = temps
    cfg.pop("temperature_by_options", None)  # nunca mascara o novo fit (finetune.md)
    for path in (OUT / "rl_agent_config.json", OUT / "checkpoint_latest" / "rl_agent_config.json"):
        if path.exists():
            path.write_text(json.dumps(cfg, indent=2))
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
