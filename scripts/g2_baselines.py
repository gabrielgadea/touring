#!/usr/bin/env python3
"""g2_baselines — medição comparativa dos backends de decisão (S-2.1, gate G2).

Plano: docs/plans/2026-09-30-jev-harness-potencializacao/plan.md.
Roda as 3 tarefas do corpus G1 (prompt_intent, memory_kind, recall_relevance) no
split de TESTE congelado (split.json, seed 20260922) contra os backends:

- current  : o sistema de hoje (classify-intent CLI p/ intent; weak_label p/ kind)
- jev      : Jev via scripts/jev_client.py (fail-open; cache; journal)
- professor: Claude via `claude -p` com JSON forçado (subamostra fixa — lento)
- laya     : Laya zero-shot local, se o pacote/pesos existirem (senão: skipped)

Métricas: acurácia, ECE por pergunta (só backends probabilísticos), p50/p99 de
latência, custo (Jev medido via journal; professor = unmeasured). Célula
indisponível é "skipped:<razão>", nunca inventada. Determinístico: ordem fixa
por id; nenhuma amostragem sem seed.
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).parent))
import jev_client  # noqa: E402

PROFESSOR_MODEL = "sonnet"
PROFESSOR_SUBSAMPLE = 40  # por tarefa — o professor é lento; Jev/Laya rodam tudo
ECE_BINS = 10

# TRÊS defeitos de instrumento até o baseline honesto (01/10):
# (1) ler "action=" (outra enum); (2) classify-intent é um TERCEIRO classificador
# (RegexSet 48 padrões — não o de produção) e sua saída não tem o header MODE,
# então o parser caía no fallback "general" nos 120 itens: 0,375 = 45/120 = a
# classe majoritária; (3) o classificador de PRODUÇÃO do UserPromptSubmit é
# prompt_enhance::classify, alcançado pelo shim `touring-hook prompt-enhance`
# (binário velho — sem o emissor S-3.2, sem efeito colateral no journal).
# O header "-- X MODE" desse additionalContext usa Intent::as_str, 1:1 com o
# corpus (general/code/debug/refactor/test/analysis/plan/creative).
PROD_HOOK_SHIM = str(Path.home() / ".claude/hooks/touring-hook")


def backend_current_intent(text: str) -> str:
    """O classificador de PRODUÇÃO do prompt: shim prompt-enhance, header MODE."""
    proc = subprocess.run(
        [PROD_HOOK_SHIM, "prompt-enhance"],
        input=json.dumps({"prompt": text}),
        capture_output=True,
        text=True,
        timeout=20,
    )
    payload = json.loads(proc.stdout)
    ctx = payload.get("hookSpecificOutput", {}).get("additionalContext", "")
    for line in ctx.splitlines():
        if "PROMPT ENHANCEMENT" in line and "--" in line and "MODE" in line:
            return line.split("--", 1)[1].strip().split()[0].strip().lower()
    raise ValueError("MODE header ausente — instrumento não provado")


def load_jsonl(path: Path) -> list[dict[str, Any]]:
    with path.open() as fh:
        return [json.loads(line) for line in fh if line.strip()]


def load_corpus(corpus: Path) -> dict[str, Any]:
    split = json.loads((corpus / "split.json").read_text())
    return {
        "split": split,
        "prompts": {p["id"]: p for p in load_jsonl(corpus / "prompts.jsonl")},
        "labels_intent": {l["id"]: l for l in load_jsonl(corpus / "labels_prompt_intent.jsonl")},
        "memories": {m["id"]: m for m in load_jsonl(corpus / "memories.jsonl")},
        "labels_kind": {l["id"]: l for l in load_jsonl(corpus / "labels_memory_kind.jsonl")},
        "pairs": {p["id"]: p for p in load_jsonl(corpus / "pairs.jsonl")},
        "labels_recall": {l["id"]: l for l in load_jsonl(corpus / "labels_recall_relevance.jsonl")},
    }


def test_items(data: dict[str, Any], set_name: str, items_key: str, labels_key: str) -> list[dict[str, Any]]:
    """Itens rotulados do split de teste, em ordem determinística (por id)."""
    assignment = data["split"]["sets"][set_name]["assignment"]
    test_ids = sorted(k for k, v in assignment.items() if v == "test")
    out = []
    for iid in test_ids:
        item = data[items_key].get(iid)
        label = data[labels_key].get(iid)
        if item is not None and label is not None:
            out.append({"id": iid, "item": item, "label": label})
    return out


# ---------------------------------------------------------------- backends


def backend_professor(state_text: str, question: str, options: list[str] | None) -> dict[str, Any] | None:
    """Professor Claude via CLI, JSON forçado. None em falha de parse/timeout."""
    schema = (
        f'Reply ONLY with JSON: {{"answer": <one of {json.dumps(options)}>}}'
        if options
        else 'Reply ONLY with JSON: {"probability_yes": <float 0..1>}'
    )
    prompt = f"{question}\n\nState:\n{state_text[:6000]}\n\n{schema}"
    try:
        proc = subprocess.run(["claude", "-p", "--model", PROFESSOR_MODEL, prompt], capture_output=True, text=True, timeout=120)
        text = proc.stdout.strip()
        start, end = text.find("{"), text.rfind("}")
        if start == -1 or end == -1:
            return None
        return json.loads(text[start : end + 1])
    except (subprocess.TimeoutExpired, json.JSONDecodeError, OSError):
        return None


def backend_laya(state: Any, questions: dict[str, dict[str, Any]]) -> dict[str, Any]:
    """Laya zero-shot local; skipped documentado se pacote/pesos faltarem (R13)."""
    try:
        import laya  # type: ignore  # noqa: F401
    except ImportError:
        return {"skipped": "laya_not_installed"}
    return {"skipped": "laya_adapter_not_implemented"}


# ---------------------------------------------------------------- dispatch


def _in_professor_subsample(item_id: str, n_items: int) -> bool:
    """Subamostra determinística por id (o professor é lento demais p/ o teste inteiro)."""
    step = max(1, n_items // PROFESSOR_SUBSAMPLE + 1)
    return int(item_id.split(":")[1], 16) % step == 0


def predict_choice(
    backend: str,
    state: Any,
    questions: dict[str, dict[str, Any]],
    qid: str,
    labels: list[str],
    item_id: str,
    n_items: int,
    state_text: str,
    professor_question: str,
    weak_label: str | None = None,
) -> tuple[str | None, float | None]:
    """Uma decisão Choice por backend. (pred, prob) ou (None, None) = célula fora."""
    if backend == "current":
        if weak_label is not None:
            return weak_label, None  # memory_kind: a tag heurística registrada
        return backend_current_intent(state_text), None  # prompt_intent: CLI viva
    if backend == "jev":
        ans = jev_client.system_one(state, questions)
        if ans is None:
            return None, None
        got = ans.get(qid, {})
        pred = got.get("choice")
        return pred, (got.get("probabilities") or {}).get(pred or "", None)
    if backend == "professor":
        if not _in_professor_subsample(item_id, n_items):
            return None, None
        ans = backend_professor(state_text, professor_question, labels + ["other"])
        return (ans.get("answer"), None) if ans else (None, None)
    if backend == "laya":
        ans = backend_laya(state, questions)
        if "skipped" in ans:
            return None, None
        return ans.get(qid, {}).get("choice"), None
    return None, None


def predict_noul(
    backend: str,
    query: str,
    candidate: str,
    item_id: str,
    n_items: int,
) -> tuple[bool | None, float | None]:
    """Uma decisão Noul por backend (recall_relevance). (pred, P(sim)) ou (None, None)."""
    question = {
        "relevant": {
            "type": "noul",
            "instructions": {
                "candidate": candidate[:3000],
                "question": "Does `candidate` contain information that directly helps with the task in `query`?",
            },
        }
    }
    if backend == "current":
        return None, None  # baseline por item não existe no G1 — unmeasured, nunca inventado
    if backend == "jev":
        ans = jev_client.system_one({"query": query[:2000]}, question)
        p = (ans or {}).get("relevant", {}).get("noul")
        return (p >= 0.5, p) if p is not None else (None, None)
    if backend == "professor":
        if not _in_professor_subsample(item_id, n_items):
            return None, None
        ans = backend_professor(
            f"Query: {query[:2000]}\nCandidate: {candidate[:3000]}",
            "Does the candidate directly help with the query?",
            None,
        )
        if not ans or "probability_yes" not in ans:
            return None, None
        p = float(ans["probability_yes"])
        return p >= 0.5, p
    if backend == "laya":
        ans = backend_laya({"query": query[:2000]}, question)
        if "skipped" in ans:
            return None, None
        p = ans.get("relevant", {}).get("noul")
        return (p >= 0.5, p) if p is not None else (None, None)
    return None, None


# ---------------------------------------------------------------- métricas


def ece(per_item: list[tuple[float, bool]]) -> float | None:
    """Expected Calibration Error em 10 bins; None se não há probabilidade."""
    if not per_item:
        return None
    total = len(per_item)
    acc = 0.0
    for b in range(ECE_BINS):
        lo, hi = b / ECE_BINS, (b + 1) / ECE_BINS
        bucket = [(p, ok) for p, ok in per_item if lo <= p < hi or (b == ECE_BINS - 1 and p == 1.0)]
        if not bucket:
            continue
        conf = statistics.mean(p for p, _ in bucket)
        acc_bin = sum(1 for _, ok in bucket if ok) / len(bucket)
        acc += (len(bucket) / total) * abs(acc_bin - conf)
    return round(acc, 4)


def summarize(cells: list[dict[str, Any]]) -> dict[str, Any]:
    lat = sorted(c["latency_ms"] for c in cells)
    n = len(cells)
    return {
        "n": n,
        "skipped": n == 0,  # célula sem dados é skipped declarado, nunca zero disfarçado
        "accuracy": round(sum(1 for c in cells if c["correct"]) / n, 4) if n else None,
        "ece": ece([(c["prob"], c["correct"]) for c in cells if c.get("prob") is not None]),
        "p50_ms": round(statistics.median(lat), 1) if lat else None,
        "p99_ms": round(lat[max(0, int(0.99 * n) - 1)], 1) if lat else None,
        "input_tokens": sum(c.get("input_tokens") or 0 for c in cells) or None,
    }


# ---------------------------------------------------------------- runner genérico

CHOICE_TASKS: dict[str, dict[str, Any]] = {
    "prompt_intent": {
        "set": "prompts",
        "items_key": "prompts",
        "labels_key": "labels_intent",
        "qid": "intent",
        "question": "What kind of work is the user asking for in `prompt`? Answer with the dominant intent.",
        "professor_question": "Classify the user's intent.",
        "state": lambda it: {"prompt": it["item"]["text"]},
        "state_text": lambda it: it["item"]["text"],
        "gold": lambda it: it["label"]["label"],
        "weak": lambda it: None,
    },
    "memory_kind": {
        "set": "memories",
        "items_key": "memories",
        "labels_key": "labels_kind",
        "qid": "kind",
        "question": "What kind of knowledge is `memory`? Base the answer on what the text TEACHES, not its words.",
        "professor_question": "What kind of knowledge is this memory?",
        "state": lambda it: {"memory": (it["item"].get("text") or it["item"].get("value") or "")[:4000]},
        "state_text": lambda it: (it["item"].get("text") or it["item"].get("value") or "")[:4000],
        "gold": lambda it: it["label"]["label"],
        "weak": lambda it: it["label"].get("weak_label"),
    },
}


def run_choice_task(spec: dict[str, Any], items: list[dict[str, Any]], backends: list[str]) -> dict[str, dict[str, Any]]:
    labels = sorted({spec["gold"](it) for it in items})
    criteria = {lb: None for lb in labels} | {"other": None}
    questions = {spec["qid"]: {"type": "choice", "instructions": spec["question"], "criteria": criteria}}
    results: dict[str, list[dict[str, Any]]] = {b: [] for b in backends}
    for it in items:
        gold = spec["gold"](it)
        for b in backends:
            started = time.monotonic()
            try:
                pred, prob = predict_choice(
                    b,
                    spec["state"](it),
                    questions,
                    spec["qid"],
                    labels,
                    it["id"],
                    len(items),
                    spec["state_text"](it),
                    spec["professor_question"],
                    spec["weak"](it),
                )
            except (subprocess.TimeoutExpired, json.JSONDecodeError, OSError, KeyError, ValueError):
                continue
            if pred is None:
                continue
            results[b].append({"id": it["id"], "pred": pred, "correct": pred == gold, "prob": prob, "latency_ms": (time.monotonic() - started) * 1000})
    return {b: summarize(cells) for b, cells in results.items()}


def run_recall_task(items: list[dict[str, Any]], backends: list[str]) -> dict[str, dict[str, Any]]:
    results: dict[str, list[dict[str, Any]]] = {b: [] for b in backends}
    for it in items:
        gold_yes = it["label"]["label"] == "yes"
        for b in backends:
            started = time.monotonic()
            try:
                pred, prob = predict_noul(b, it["item"]["query"], it["item"]["text"], it["id"], len(items))
            except (subprocess.TimeoutExpired, json.JSONDecodeError, OSError, KeyError, ValueError):
                continue
            if pred is None:
                continue
            results[b].append({"id": it["id"], "pred": "yes" if pred else "no", "correct": pred == gold_yes, "prob": prob, "latency_ms": (time.monotonic() - started) * 1000})
    return {b: summarize(cells) for b, cells in results.items()}


# ---------------------------------------------------------------- main


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", default=str(Path("~/.claude/touring/g1-corpus").expanduser()))
    parser.add_argument("--tasks", default="prompt_intent,memory_kind,recall_relevance")
    parser.add_argument("--backends", default="current,jev,professor,laya")
    parser.add_argument("--out", default="docs/plans/2026-09-30-jev-harness-potencializacao/experiments/g2-baselines")
    args = parser.parse_args()

    data = load_corpus(Path(args.corpus).expanduser())
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    wanted = set(args.tasks.split(","))
    backends = args.backends.split(",")
    results: dict[str, Any] = {"split_hash": jev_client._cache_key(data["split"], {}, "split"), "tasks": {}}

    for task_name, spec in CHOICE_TASKS.items():
        if task_name not in wanted:
            continue
        items = test_items(data, spec["set"], spec["items_key"], spec["labels_key"])
        results["tasks"][task_name] = {"n_test": len(items), "backends": run_choice_task(spec, items, backends)}

    if "recall_relevance" in wanted:
        items = test_items(data, "pairs", "pairs", "labels_recall")
        results["tasks"]["recall_relevance"] = {"n_test": len(items), "backends": run_recall_task(items, backends)}

    (out_dir / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=1))
    print(json.dumps({"status": "ok", "out": str(out_dir / "results.json")}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
