#!/usr/bin/env python3
"""pre_review_triage — funil advisory de 7 Nouls sobre um diff (S-5.3).

Plano: docs/plans/2026-09-30-jev-harness-potencializacao/plan.md.
Padrão externo: "Pre-Code Review Gate" (7 perguntas binárias; todas falsas com
confiança → revisão rápida; qualquer verdadeira/incerta → revisão completa).
ADVISORY SEMPRE: este script nunca bloqueia (exit 0 mesmo sem Jev), nunca é
cláusula do juiz de convergência — ele TRIAGEIA onde o review profundo vale.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).parent))
import jev_client  # noqa: E402

# Piso de "sim" para o code-path: P < 0,5 em TODAS → fast path. Declarado,
# ilustrativo (a doc é explícita: calibrar com dados próprios — S-2.x).
FAST_PATH_BELOW = 0.5

# S-10 (plano r2, cookbook self-consistency): a banda 0,5±0,1 é INCERTEZA — um
# outcome roteável de primeira classe (humano/Sistema 2), nunca um binário forçado.
# fast path exige TODAS as respostas "no" com confiança; qualquer "yes" OU
# "uncertain" roteia para a revisão completa (a doutrina já estava no docstring).
UNCERTAIN_BAND = (0.4, 0.6)


def outcome_of(p: float | None) -> str:
    """yes | no | uncertain — a banda média é declarada, nunca arredondada."""
    if p is None:
        return "uncertain"
    lo, hi = UNCERTAIN_BAND
    if p >= hi:
        return "yes"
    if p <= lo:
        return "no"
    return "uncertain"

# As 7 perguntas do funil — uma decisão estreita cada, formato "sim alto" (docs).
TRIAGE_QUESTIONS: dict[str, str] = {
    "touches_auth_or_crypto": "Does the diff change authentication, authorization, tokens, or cryptographic code?",
    "modifies_security_boundaries": "Does the diff change a security boundary (sandbox, gates, validators, permission checks)?",
    "changes_public_api": "Does the diff change a public API surface (exported signatures, CLI contracts, wire formats)?",
    "deletes_code_paths": "Does the diff delete code paths, features, or behavior without an obvious replacement?",
    "lacks_tests_for_new_logic": "Does the diff add logic (branches, error handling, state) without accompanying tests?",
    "concurrency_or_shared_state": "Does the diff touch concurrency, shared mutable state, locks, or caches?",
    "ambiguous_intent": "Is the intent of the diff unclear from the diff itself (no tests, no docs, opaque names)?",
}

TRIAGE_REASONS: dict[str, str] = {
    "touches_auth_or_crypto": "mexe com autenticação/cripto",
    "modifies_security_boundaries": "mexe com fronteira de segurança",
    "changes_public_api": "muda API pública",
    "deletes_code_paths": "remove caminhos sem substituição clara",
    "lacks_tests_for_new_logic": "lógica nova sem testes",
    "concurrency_or_shared_state": "concorrência/estado compartilhado",
    "ambiguous_intent": "intenção opaca",
}

# S-6 (plano r2, composite scoring — "o modelo pontua, o código decide"): 3 Scores
# na MESMA chamada; os pesos vivem em .touring/touring.toml [triage.weights] e o
# composite é computado EM CÓDIGO — afinar o risco é mudar um número, nunca um prompt.
TRIAGE_SCORES: dict[str, dict[str, Any]] = {
    "security_risk": {
        "instructions": "Rate the security risk of this diff: 0 = no security-relevant change, 1 = touches input handling/auth-adjacent code, 2 = handles auth, secrets, sandbox boundaries, or user input directly.",
        "criteria": ["no security-relevant change", "touches input handling or auth-adjacent code", "handles auth, secrets, sandbox boundaries, or user input directly"],
    },
    "pattern_alignment": {
        "instructions": "Rate how well this diff follows the existing codebase conventions: 0 = breaks established patterns, 1 = mixed, 2 = follows existing patterns cleanly.",
        "criteria": ["breaks established patterns", "partially follows conventions", "follows existing patterns cleanly"],
    },
    "diff_quality": {
        "instructions": "Rate the quality of this diff as a reviewable unit: 0 = unclear, no tests, opaque intent, 1 = reasonable, 2 = clean diff with tests and clear intent.",
        "criteria": ["unclear, untested, opaque", "reasonable diff", "clean diff with tests and clear intent"],
    },
}

DEFAULT_WEIGHTS: dict[str, float] = {"security_risk": 0.5, "pattern_alignment": 0.3, "diff_quality": 0.2}


def load_weights(config_path: Path = Path(".touring/touring.toml")) -> dict[str, float]:
    """[triage.weights] do touring.toml, senão DEFAULT_WEIGHTS — sempre declarado."""
    try:
        import tomllib

        data = tomllib.loads(config_path.read_text())
        weights = data.get("triage", {}).get("weights", {})
        return {k: float(weights.get(k, v)) for k, v in DEFAULT_WEIGHTS.items()}
    except (OSError, ValueError, KeyError):
        return dict(DEFAULT_WEIGHTS)


def compose_composite(scores: dict[str, float | None], weights: dict[str, float]) -> float | None:
    """Σ wᵢ·sᵢ normalizado (0–2) — puro, replayável: trocar peso NÃO re-chama o modelo."""
    parts = [(weights[k], scores[k]) for k in DEFAULT_WEIGHTS if scores.get(k) is not None]
    if not parts:
        return None
    return round(sum(w * (s / 2.0) for w, s in parts) / sum(w for w, _ in parts), 4)


def build_questions() -> dict[str, dict[str, Any]]:
    """As 7 Nouls + 3 Scores sobre o mesmo state — uma chamada, paralelas (a doc:
    adicionar perguntas à mesma chamada é praticamente gratuito)."""
    nouls = {qid: {"type": "noul", "instructions": text} for qid, text in TRIAGE_QUESTIONS.items()}
    scores = {qid: {"type": "score", "instructions": spec["instructions"], "criteria": spec["criteria"]} for qid, spec in TRIAGE_SCORES.items()}
    return nouls | scores


def triage(diff_text: str, *, files: list[str] | None = None, weights: dict[str, float] | None = None) -> dict[str, Any]:
    """Avalia o diff. Retorna o veredito advisory; Jev ausente → skipped (exit 0)."""
    state = {"files": files or [], "diff": diff_text[:12000]}
    answers = jev_client.system_one(state, build_questions())
    if answers is None:
        return {"status": "skipped", "reason": "jev_unavailable", "fast_path": None}
    probs = {qid: answers[qid].get("noul") for qid in TRIAGE_QUESTIONS if qid in answers}
    outcomes = {qid: outcome_of(p) for qid, p in probs.items()}
    flagged = [qid for qid, o in outcomes.items() if o == "yes"]
    uncertain = [qid for qid, o in outcomes.items() if o == "uncertain"]
    scores = {qid: answers[qid].get("score") for qid in TRIAGE_SCORES if qid in answers}
    used_weights = weights if weights is not None else load_weights()
    return {
        "status": "ok",
        "fast_path": not flagged and not uncertain,
        "flagged": flagged,
        "uncertain": uncertain,
        "reasons": [TRIAGE_REASONS[qid] for qid in flagged + uncertain],
        "probabilities": probs,
        "outcomes": outcomes,
        "scores": scores,
        "composite": compose_composite(scores, used_weights),
        "weights": used_weights,
        "policy": {"fast_path_below": FAST_PATH_BELOW, "uncertain_band": UNCERTAIN_BAND, "advisory": True},
    }


def read_diff(args: argparse.Namespace) -> str:
    """Diff do arquivo, do git show, ou do stdin — nessa ordem de precedência."""
    if args.diff_file:
        return Path(args.diff_file).read_text()
    if args.git_ref:
        proc = subprocess.run(
            ["git", "show", args.git_ref] + (["--"] + args.paths if args.paths else []),
            capture_output=True,
            text=True,
            timeout=30,
        )
        return proc.stdout
    if not sys.stdin.isatty():
        return sys.stdin.read()
    raise SystemExit("sem diff: use --diff-file, --git-ref ou stdin")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--diff-file")
    parser.add_argument("--git-ref")
    parser.add_argument("--paths", nargs="*", default=[])
    args = parser.parse_args()
    result = triage(read_diff(args), files=args.paths)
    print(json.dumps(result, ensure_ascii=False, indent=1))
    return 0  # advisory: exit 0 sempre — quem bloqueia é o juiz, não o funil


if __name__ == "__main__":
    sys.exit(main())
