#!/usr/bin/env python3
"""w46_teacher_cascade — corpus 800→1.200 com rótulos teacher em cascata (S-5, plan r2).

O G1 (800 prompts) ficou abaixo do piso documentado (~1.000, caso oficial 51,3%→79,4%).
A extensão usa prompts de PRODUÇÃO (transcripts CC — a deriva mora na distribuição
real), rotulados em cascata: Jev em lote (Choice 8 classes, confiança na gold) e
professor Claude SÓ nos casos fracos (conf < 0,6) — o custo vai onde a incerteza está.
O split congelado (seed 20260922) NUNCA é tocado: a extensão grava arquivos _r2 com
seed própria (20261001) e `extension: true`, e o trainer decide como incluí-los sem
contaminar a avaliação congelada.
"""

from __future__ import annotations

import hashlib
import json
import random
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).parent))
import jev_client  # noqa: E402
from g2_baselines import backend_professor  # noqa: E402

CORPUS = Path("~/.claude/touring/g1-corpus").expanduser()
TRANSCRIPTS = Path("~/.claude/projects").expanduser()
EXTENSION_SEED = 20261001
PROFESSOR_BELOW = 0.6  # τ medido do aluno — abaixo disso o teacher é fraco
INTENT_CLASSES = ["general", "code", "debug", "refactor", "test", "analysis", "plan", "creative"]
INTENT_QUESTION = "What kind of work is the user asking for in `prompt`? Answer with the dominant intent."

# Payloads de máquina — subconjunto produção do AUTOMATED_PAYLOAD_PREFIXES (prompt_enhance.rs)
AUTOMATED_PREFIXES = (
    "<task-notification>", "<cross-session-message", "<system-reminder",
    "<local-command", "<command-", "Caveat:", "[SYSTEM NOTIFICATION",
)

# LGPD (restrição constitucional da sessão): dados do projeto analise (ANTT) NUNCA
# vão ao Jev — professor Claude apenas lá. A extração exclui esses transcripts por
# construção, não por disciplina: o caminho nem é lido.
EXCLUDED_PROJECT_MARKERS = ("analise", "ANTT", "antt")


def _project_allowed(path: Path) -> bool:
    return not any(marker in str(path) for marker in EXCLUDED_PROJECT_MARKERS)


def extract_prompts(transcripts_dir: Path, *, max_files: int = 40) -> dict[str, str]:
    """prompt_hash → texto, de mensagens user-typed dos transcripts mais recentes.

    Dedup por blake2b-16 do texto normalizado; fora: payloads de máquina, vazios,
    < 12 ou > 4000 chars. Ordem determinística (arquivos por mtime, depois hash).
    """
    files = sorted(
        (p for p in transcripts_dir.glob("*/*.jsonl") if _project_allowed(p)),
        key=lambda p: p.stat().st_mtime,
        reverse=True,
    )[:max_files]
    out: dict[str, str] = {}
    for path in files:
        for line in path.read_text(errors="replace").splitlines():
            try:
                d = json.loads(line)
            except json.JSONDecodeError:
                continue
            if d.get("type") != "user":
                continue
            content = d.get("message", {}).get("content")
            if isinstance(content, str):
                text = content
            elif isinstance(content, list):
                text = " ".join(b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text")
            else:
                continue
            text = text.strip()
            if len(text) < 12 or len(text) > 4000 or text.startswith(AUTOMATED_PREFIXES):
                continue
            key = hashlib.blake2b(text.encode(), digest_size=16).hexdigest()
            out.setdefault(key, text)
    return dict(sorted(out.items()))


def sample_prompts(prompts: dict[str, str], n: int, seed: int = EXTENSION_SEED) -> list[tuple[str, str]]:
    """Amostra determinística (seed declarada) de n (hash, texto)."""
    keys = sorted(prompts)
    chosen = random.Random(seed).sample(keys, min(n, len(keys)))
    return [(k, prompts[k]) for k in sorted(chosen)]


def label_batch(texts: list[str]) -> dict[int, dict[str, Any]]:
    """Uma chamada Jev por lote: pergunta por prompt, texto embutido nas instructions."""
    questions = {
        f"p{i}": {
            "type": "choice",
            "instructions": {"prompt": text[:2500], "question": INTENT_QUESTION},
            "criteria": {c: None for c in INTENT_CLASSES},
        }
        for i, text in enumerate(texts)
    }
    answers = jev_client.system_one({"task": "intent classification (teacher cascade)"}, questions)
    if answers is None:
        return {}
    return {int(qid[1:]): a for qid, a in answers.items()}


def cascade_label(
    pairs: list[tuple[str, str]],
    *,
    batch_size: int = 25,
    professor=None,
) -> list[dict[str, Any]]:
    """Rótulo gold por prompt: Jev sempre; professor quando conf < PROFESSOR_BELOW.

    provenance: "jev" (forte) | "professor" (fraco — o professor decide) |
    "jev+professor_agree" (o professor confirma o fraco — confiança sobe para 0,7).
    `professor=None` resolve o backend NO CALL TIME (default-arg bindaria no def e
    tornaria o teste vendado impossível — pego quando a suíte rodou o professor REAL).
    """
    prof_fn = professor if professor is not None else backend_professor
    labeled: list[dict[str, Any]] = []
    for start in range(0, len(pairs), batch_size):
        chunk = pairs[start : start + batch_size]
        answers = label_batch([text for _, text in chunk])
        for i, (key, text) in enumerate(chunk):
            got = answers.get(i) or {}
            choice = got.get("choice")
            probs = got.get("probabilities") or {}
            conf = float(got.get("confidence") if got.get("confidence") is not None else (max(probs.values()) if probs else 0.0))
            if choice is None:
                continue  # fora do corpus, nunca um rótulo inventado
            entry = {
                "id": f"r2:{key}",
                "task": "prompt_intent",
                "project": "production",
                "text": text,
                "label": choice,
                "confidence": round(conf, 4),
                "weak_label": None,
                "provenance": "jev",
                "extension_seed": EXTENSION_SEED,
            }
            if conf < PROFESSOR_BELOW and prof_fn is not None:
                prof = prof_fn(text[:6000], INTENT_QUESTION, INTENT_CLASSES + ["other"])
                plabel = (prof or {}).get("answer")
                if plabel in INTENT_CLASSES:
                    if plabel == choice:
                        entry["provenance"] = "jev+professor_agree"
                        entry["confidence"] = 0.7
                    else:
                        entry["label"] = plabel
                        entry["provenance"] = "professor"
                        entry["confidence"] = 0.8
            labeled.append(entry)
    return labeled


def write_corpus(entries: list[dict[str, Any]], corpus: Path = CORPUS) -> dict[str, Any]:
    """prompts_r2.jsonl + labels_prompt_intent_r2.jsonl — arquivos NOVOS, split intacto."""
    corpus.mkdir(parents=True, exist_ok=True)
    prompts_path = corpus / "prompts_r2.jsonl"
    labels_path = corpus / "labels_prompt_intent_r2.jsonl"
    with prompts_path.open("w") as fh:
        for e in entries:
            fh.write(json.dumps({"id": e["id"], "task": e["task"], "project": e["project"], "text": e["text"]}, ensure_ascii=False) + "\n")
    with labels_path.open("w") as fh:
        for e in entries:
            fh.write(json.dumps({
                "id": e["id"], "label": e["label"], "confidence": e["confidence"],
                "weak_label": e["weak_label"], "provenance": e["provenance"],
                "extension_seed": e["extension_seed"], "extension": True,
            }, ensure_ascii=False) + "\n")
    prov: dict[str, int] = {}
    for e in entries:
        prov[e["provenance"]] = prov.get(e["provenance"], 0) + 1
    return {"written": len(entries), "prompts": str(prompts_path), "labels": str(labels_path), "provenance": prov}


def main() -> int:
    n = int(sys.argv[sys.argv.index("--n") + 1]) if "--n" in sys.argv else 400
    pairs = sample_prompts(extract_prompts(TRANSCRIPTS), n)
    entries = cascade_label(pairs)
    result = write_corpus(entries)
    print(json.dumps(result, ensure_ascii=False, indent=1))
    return 0 if entries else 1


if __name__ == "__main__":
    sys.exit(main())
