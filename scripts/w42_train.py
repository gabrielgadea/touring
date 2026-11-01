#!/usr/bin/env python3
"""w42_train — driver do fine-tune RLCD do laya-multilingual no corpus G1 (S-4.2).

Dirige o `train()` do notebook MPS do repo laya com os itens do w42_prepare_items.
Knobs para 8 GB (RTX 4060): micro_batch=1, grad_accum=64 (batch efetivo 64),
gradient checkpointing ligado, 4 épocas — a receita de 16 GB do finetune.md,
dividida pelo micro-batch mínimo. Calibração (temperatura por tipo) roda ao fim
na fatia held-out que o próprio trainer separa (seed 20260922).
"""

from __future__ import annotations

import argparse
import importlib.util
import sys
from pathlib import Path

MPS_SCRIPT = Path("/tmp/laya-repo/notebooks/laya_finetune_typed_decisions_mps.py")
WORK = Path("~/.claude/touring/laya-w42-intent").expanduser()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--epochs", type=int, default=4)
    parser.add_argument("--micro-batch", type=int, default=1)
    parser.add_argument("--grad-accum", type=int, default=64)
    parser.add_argument("--calib-max", type=int, default=400)
    parser.add_argument("--device", default="cuda")
    parser.add_argument("--items", default=str(WORK / "items_train.pt"))
    parser.add_argument("--output-dir", default=str(WORK / "output"))
    args, _ = parser.parse_known_args()
    args.no_checkpointing = False

    spec = importlib.util.spec_from_file_location("mps_finetune", MPS_SCRIPT)
    mps = importlib.util.module_from_spec(spec)
    sys.modules["mps_finetune"] = mps
    spec.loader.exec_module(mps)

    hf_home = Path("~/.cache/huggingface/hub").expanduser()
    snaps = sorted(hf_home.glob("models--convaiinnovations--laya/snapshots/*"))
    model_dir = snaps[-1] / "multilingual"

    import torch

    # R15 (medido 01/10, OOM no 1º step): fp32 AdamW ≈ 322M×16 B = 5,2 GB e o
    # cartão tem ~2 GB de inquilinos — não fecha. PagedAdamW8bit (bitsandbytes,
    # hash verificado) mantém os estados em 8-bit e os pagina para a RAM (64 GB)
    # — é o PagedAdamW padrão para treino em GPU única de 8 GB.
    import bitsandbytes as bnb

    torch.optim.AdamW = bnb.optim.PagedAdamW8bit

    device = torch.device(args.device if torch.cuda.is_available() else "cpu")
    print(f"device={device} · vram={torch.cuda.get_device_properties(0).total_memory / 1e9:.1f} GB" if device.type == "cuda" else f"device={device}")
    mps.train(args, model_dir, args.items, device)
    return 0


if __name__ == "__main__":
    sys.exit(main())
