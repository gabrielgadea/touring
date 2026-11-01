#!/usr/bin/env python3
"""g2_calibration — curva confidence×acurácia por tarefa + τ recomendado (S-2.2 follow-up).

Nasce da auditoria da touring-02 (01/10): o τ que o W5.2 herda não existia como
campo, e o split_hash não reconciliava com o sha256 do arquivo. Este script:
(1) re-emite `split_sha256` (sha256 dos BYTES do split.json, o contrato dela);
(2) computa a curva por tarefa a partir das chamadas Jev (cache do adapter —
zero custo de API); (3) recomenda `threshold_recommended` por tarefa, ou null
com razão nomeada quando nenhuma faixa justifica automação.

Regra do τ (declarada): a menor faixa de confidence com acurácia ≥ 0,85 E
suporte ≥ 15% dos itens. Sem essa faixa → null + razão (nunca chutar número).
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import g2_baselines as g2  # noqa: E402
import jev_client  # noqa: E402

CORPUS = Path("~/.claude/touring/g1-corpus").expanduser()
RESULTS = Path("docs/plans/2026-09-30-jev-harness-potencializacao/experiments/g2-baselines/results.json")
TAU_MIN_ACCURACY = 0.85
TAU_MIN_SUPPORT = 0.15


def curve_for_choice(items: list[dict], spec: dict) -> list[dict]:
    """Recomputa confidence×acurácia por faixa (chamadas Jev saem do cache)."""
    labels = sorted({spec["gold"](it) for it in items})
    questions = {spec["qid"]: {"type": "choice", "instructions": spec["question"], "criteria": {lb: None for lb in labels} | {"other": None}}}
    rows: list[tuple[float, bool]] = []
    for it in items:
        ans = jev_client.system_one(spec["state"](it), questions)
        if ans is None:
            continue
        got = ans.get(spec["qid"], {})
        conf = got.get("confidence")
        if conf is None:
            probs = got.get("probabilities") or {}
            conf = max(probs.values()) if probs else None
        if conf is None:
            continue
        rows.append((float(conf), got.get("choice") == spec["gold"](it)))
    bins: list[dict] = []
    for lo, hi in ((0.0, 0.2), (0.2, 0.4), (0.4, 0.6), (0.6, 0.8), (0.8, 1.01)):
        bucket = [(c, ok) for c, ok in rows if lo <= c < hi]
        if bucket:
            bins.append({
                "range": [round(lo, 2), round(min(hi, 1.0), 2)],
                "n": len(bucket),
                "support": round(len(bucket) / len(rows), 4) if rows else 0,
                "accuracy": round(sum(1 for _, ok in bucket if ok) / len(bucket), 4),
                "mean_conf": round(sum(c for c, _ in bucket) / len(bucket), 4),
            })
    return bins


def recommend_tau(bins: list[dict]) -> tuple[float | None, str]:
    """A menor faixa com acurácia ≥ alvo E suporte ≥ mínimo; senão null + razão.

    As três bandas decididas EM CÓDIGO (doc oficial, S-10 do plano r2):
    τ ≥ 0,85–0,90 → execução autônoma · 0,50–0,85 → verificação suave/confirmação ·
    < 0,50 → escala (humano/Sistema 2). Um limiar por AÇÃO, pelo custo de errar —
    nunca um τ único para o sistema inteiro. E a banda média é outcome roteável
    ("uncertain"), não binário forçado (cookbook self-consistency).
    """
    for b in reversed(bins):  # da mais confiante para baixo — a primeira que fecha
        if b["accuracy"] >= TAU_MIN_ACCURACY and b["support"] >= TAU_MIN_SUPPORT:
            return b["range"][0], f"faixa {b['range']}: acc={b['accuracy']}, support={b['support']}"
    return None, f"nenhuma faixa com acurácia ≥ {TAU_MIN_ACCURACY} e suporte ≥ {TAU_MIN_SUPPORT} — revisão humana, não automação"


def main() -> int:
    data = g2.load_corpus(CORPUS)
    results = json.loads(RESULTS.read_text())

    # (1) sha256 dos bytes do arquivo — o contrato da touring-02 (S-2.0).
    results["split_sha256"] = hashlib.sha256((CORPUS / "split.json").read_bytes()).hexdigest()

    # (2)+(3) curva e τ por tarefa.
    results["threshold_recommended"] = {}
    for task_name, spec in g2.CHOICE_TASKS.items():
        if task_name == "prompt_intent":
            results["threshold_recommended"][task_name] = {
                "tau": None,
                "reason": "trilha professor→aluno (veredito S-2.2 §2.3) — Jev não é o decisor deste ponto",
            }
            continue
        items = g2.test_items(data, spec["set"], spec["items_key"], spec["labels_key"])
        bins = curve_for_choice(items, spec)
        tau, reason = recommend_tau(bins)
        results["threshold_recommended"][task_name] = {"tau": tau, "reason": reason, "calibration_curve": bins}
    results["threshold_recommended"]["recall_relevance"] = {
        "tau": None,
        "reason": "ECE 0,709 medido no G2 — Noul proibido como τ (veredito S-2.2 §2.2); usar só ranking/corte binário",
    }

    RESULTS.write_text(json.dumps(results, ensure_ascii=False, indent=1))
    print(json.dumps({
        "split_sha256": results["split_sha256"][:16],
        "tau": {k: v.get("tau") for k, v in results["threshold_recommended"].items()},
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
