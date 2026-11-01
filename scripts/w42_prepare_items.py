#!/usr/bin/env python3
"""w42_prepare_items — corpus G1 (intent) → itens RLCD para o fine-tune Laya (S-4.2).

Plano: docs/plans/2026-09-30-jev-harness-potencializacao/plan.md.
RLCD treina sobre a DISTRIBUIÇÃO do professor, não sobre o rótulo duro (docs/
finetune.md do repo laya). O G1 guarda (label, confidence) do professor; a
distribuição alvo é `confidence` na classe do professor e o restante dividido
igualmente entre as demais 7 — síntese declarada, não medição.
Reutiliza `build_training_item` do notebook MPS (o schema é o contrato do trainer).
"""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

import torch
from transformers import AutoTokenizer

CORPUS = Path("~/.claude/touring/g1-corpus").expanduser()
MPS_SCRIPT = Path("/tmp/laya-repo/notebooks/laya_finetune_typed_decisions_mps.py")
OUT_DIR = Path("~/.claude/touring/laya-w42-intent").expanduser()
INTENT_CLASSES = ["general", "code", "debug", "refactor", "test", "analysis", "plan", "creative"]

# S-9 (plan r2, restrição permanente da documentação): flat choice degrada após
# ~20 opções (77 rótulos planos = 0,425 medido). Acima do teto, o caminho é a
# decomposição hierárquica com beam search paralelo — nunca alargar a lista e
# deixar a acurácia cair em silêncio. Falha ALTO no import, não no treino.
MAX_FLAT_LABELS = 20


def check_flat_label_limit(n_labels: int) -> None:
    """Fail-loud acima do teto documentado — a mensagem nomeia a rota hierárquica."""
    if n_labels > MAX_FLAT_LABELS:
        raise ValueError(
            f"{n_labels} rótulos num choice plano excede o teto documentado de "
            f"{MAX_FLAT_LABELS} (77 flat = 0,425 medido) — decomponha hierarquicamente "
            f"com beam search paralelo, não alargue a lista"
        )


check_flat_label_limit(len(INTENT_CLASSES))

QUESTION = {
    "type": "choice",
    "instructions": "What kind of work is the user asking for in `prompt`? Answer with the dominant intent.",
    "criteria": {c: None for c in INTENT_CLASSES},
}


def load_split(name: str) -> list[str]:
    split = json.loads((CORPUS / "split.json").read_text())
    return sorted(k for k, v in split["sets"]["prompts"]["assignment"].items() if v == name)


def gold_distribution(label: str, confidence: float) -> dict[str, float]:
    """Professor vira distribuição: confidence na classe dele, resto dividido."""
    conf = min(max(float(confidence), 0.55), 0.97)  # nunca 0 nem 1 — o CE precisa de massa
    rest = (1.0 - conf) / (len(INTENT_CLASSES) - 1)
    return {c: (conf if c == label else rest) for c in INTENT_CLASSES}


def main() -> int:
    spec = importlib.util.spec_from_file_location("mps_finetune", MPS_SCRIPT)
    mps = importlib.util.module_from_spec(spec)
    sys.modules["mps_finetune"] = mps
    spec.loader.exec_module(mps)

    # model_dir = subdir multilingual do snapshot baixado pelo Agent (prova de 01/10)
    hf_home = Path("~/.cache/huggingface/hub").expanduser()
    snaps = sorted(hf_home.glob("models--convaiinnovations--laya/snapshots/*"))
    model_dir = snaps[-1] / "multilingual"
    assert (model_dir / "model.safetensors").exists(), f"snapshot multilingual ausente em {snaps}"
    cfg = json.loads((model_dir / "rl_agent_config.json").read_text())
    cfg.update({"max_len": 1024, "head_max_len": 256})
    tokenizer = AutoTokenizer.from_pretrained(model_dir / "tokenizer")

    prompts = {}
    for line in (CORPUS / "prompts.jsonl").open():
        d = json.loads(line)
        prompts[d["id"]] = d["text"]
    labels = {}
    for line in (CORPUS / "labels_prompt_intent.jsonl").open():
        d = json.loads(line)
        labels[d["id"]] = d

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    stats = {"built": 0, "skipped": 0, "missing_label": 0}
    for split_name in ("train", "val", "test"):
        items = []
        for iid in load_split(split_name):
            lab = labels.get(iid)
            if lab is None:
                stats["missing_label"] += 1
                continue
            item = mps.build_training_item(
                tokenizer,
                cfg,
                {"prompt": prompts[iid]},
                QUESTION,
                {"probabilities": gold_distribution(lab["label"], lab.get("confidence", 0.8))},
            )
            if item is None:
                stats["skipped"] += 1
                continue
            items.append(item)
            stats["built"] += 1
        torch.save(items, OUT_DIR / f"items_{split_name}.pt")
        print(f"{split_name}: {len(items)} itens -> {OUT_DIR / f'items_{split_name}.pt'}")
    print("stats:", stats)
    return 0


if __name__ == "__main__":
    sys.exit(main())
