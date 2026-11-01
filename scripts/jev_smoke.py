#!/usr/bin/env python3
"""jev_smoke — prova de vida end-to-end da API TypeSafe System One (S-0.3).

1 Noul + 1 Choice sobre 3 linhas do Cargo.toml do workspace. Exit 0 com resposta
tipada válida; sem chave/rede → exit 0 com `skipped` (fail-open, nunca falso verde).
Custo estimado impresso a partir de input_tokens medidos (US$ 0,042/MTok, saída grátis).
"""

from __future__ import annotations

import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import jev_client  # noqa: E402

PRICE_PER_INPUT_TOKEN = 0.042 / 1_000_000  # US$ por token de entrada (docs 30/09)


def main() -> int:
    cargo = Path("Cargo.toml").read_text().splitlines()[:3]
    state = {"file": "Cargo.toml", "head": "\n".join(cargo)}
    questions = {
        "is_manifest": {
            "type": "noul",
            "instructions": "Does `state.head` look like the header of a Rust workspace manifest (Cargo.toml)?",
        },
        "ecosystem": {
            "type": "choice",
            "instructions": "Which ecosystem does `state.head` belong to?",
            "criteria": {
                "rust": "Cargo.toml / workspace manifest",
                "python": "pyproject.toml or setup",
                "other": "None of the above",
            },
        },
    }
    started = time.monotonic()
    answers = jev_client.system_one(state, questions)
    latency_ms = (time.monotonic() - started) * 1000

    if answers is None:
        print(json.dumps({"status": "skipped", "reason": "no_key_or_network_or_sdk"}))
        return 0  # fail-open: skip honesto, nunca falso verde

    # Validação de schema (best practice #23): choice dentro das opções,
    # probabilities cobrem as opções e somam ≈ 1 (±0,02), choice é o argmax.
    eco = answers.get("ecosystem", {})
    probs = eco.get("probabilities", {})
    total = sum(probs.values())
    checks = {
        "noul_in_range": 0.0 <= answers.get("is_manifest", {}).get("noul", -1) <= 1.0,
        "choice_in_options": eco.get("choice") in questions["ecosystem"]["criteria"],
        "probs_cover_options": set(probs) == set(questions["ecosystem"]["criteria"]),
        "probs_sum_approx_1": abs(total - 1.0) <= 0.02,
        "choice_is_argmax": eco.get("choice") == max(probs, key=probs.get) if probs else False,
    }
    journal_path = jev_client.DEFAULT_JOURNAL_FILE
    last_journal = {}
    if journal_path.exists():
        last_journal = json.loads(journal_path.read_text().strip().splitlines()[-1])
    input_tokens = last_journal.get("input_tokens") or 0
    cost_usd = input_tokens * PRICE_PER_INPUT_TOKEN

    ok = all(checks.values())
    print(
        json.dumps(
            {
                "status": "ok" if ok else "schema_violation",
                "checks": checks,
                "latency_ms": round(latency_ms, 1),
                "input_tokens": input_tokens,
                "cost_usd": round(cost_usd, 8),
                "model": last_journal.get("model_reported"),
                "answers": answers,
            },
            ensure_ascii=False,
            indent=1,
        )
    )
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
