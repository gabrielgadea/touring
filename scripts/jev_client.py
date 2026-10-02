#!/usr/bin/env python3
"""jev_client — adaptador fino para a API TypeSafe System One (Jev).

Plano: docs/plans/2026-09-30-jev-harness-potencializacao/plan.md (S-0.2).
Contratos:
- A chave é lida de ~/.config/typesafe/env (fallback: TYPESAFE_API_KEY no ambiente).
  Nenhum rc é tocado; o daemon não precisa de restart; a chave NUNCA vai a log/journal.
- Cache por blake2b(state + perguntas + modelo) — o Jev é "extremely consistent" (docs).
- Journal append-only em ~/.claude/touring/logs/jev_journal.jsonl (uma linha por chamada).
- FAIL-OPEN estrutural: qualquer falha (rede, 429/529, sem chave, SDK) devolve None
  e registra `fallback_reason` no journal. Este módulo nunca derruba o chamador.
"""

# touring-quality:allow-secrets — JUSTIFICATIVA (F2.4, cross-audit 2026-10-01):
# o sentinela casa a palavra "key"/"TYPESAFE_API_KEY" no código, mas NENHUM literal
# de segredo existe aqui: a chave é lida em runtime de ~/.config/typesafe/env (ou do
# ambiente), nunca embutida, nunca logada (load_key devolve o valor sem registrá-lo;
# o journal grava apenas route_taken/fallback_reason/digests). UNVERIFIED → verificado
# por leitura humana do fluxo completo da chave.

from __future__ import annotations

import hashlib
import json
import os
import tempfile
import time
from pathlib import Path
from typing import Any

DEFAULT_MODEL = "jev-1.13.0"
DEFAULT_KEY_FILE = Path("~/.config/typesafe/env").expanduser()
DEFAULT_CACHE_FILE = Path("~/.claude/touring/jev_cache.json").expanduser()
DEFAULT_JOURNAL_FILE = Path("~/.claude/touring/logs/jev_journal.jsonl").expanduser()
DEFAULT_TIMEOUT_S = 10.0

# Exceções específicas que o SDK pode levantar (importadas com guarda para o
# módulo continuar importável mesmo sem o SDK instalado — fail-open desde o import).
try:
    from typesafe_sdk import (
        Choice,
        Noul,
        Score,
        TypeSafeClient,
        TypeSafeAPIConnectionError,
        TypeSafeAPIError,
    )

    _SDK_AVAILABLE = True
    _SDK_ERRORS: tuple[type[BaseException], ...] = (
        TypeSafeAPIError,
        TypeSafeAPIConnectionError,
        TimeoutError,
        OSError,
    )
except ImportError:  # SDK ausente: todo system_one devolve None com razão "sdk_missing"
    _SDK_AVAILABLE = False
    _SDK_ERRORS = (TimeoutError, OSError)


class JevSpecError(ValueError):
    """Spec de pergunta inválida (tipo desconhecido ou campo obrigatório ausente)."""


def load_key(key_file: Path | str | None = None) -> str | None:
    """Lê a chave do ambiente ou do arquivo de env dedicado. Nunca loga o valor."""
    env_val = os.environ.get("TYPESAFE_API_KEY")
    if env_val:
        return env_val
    # Aceita str OU Path (cross-audit 2026-10-01: str caía em AttributeError opaco)
    path = Path(key_file) if key_file else DEFAULT_KEY_FILE
    try:
        for line in path.read_text().splitlines():
            line = line.strip()
            if line.startswith("TYPESAFE_API_KEY="):
                value = line.split("=", 1)[1].strip().strip('"').strip("'")
                return value or None
    except OSError:
        return None
    return None


def _canonical(value: Any) -> str:
    """JSON canônico (chaves ordenadas) para chave de cache estável."""
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def _cache_key(state: Any, questions: dict[str, dict[str, Any]], model: str) -> str:
    digest = hashlib.blake2b(digest_size=32)
    digest.update(_canonical(state).encode("utf-8"))
    digest.update(b"\x00")
    digest.update(_canonical(questions).encode("utf-8"))
    digest.update(b"\x00")
    digest.update(model.encode("utf-8"))
    return digest.hexdigest()


def _read_cache(cache_file: Path) -> dict[str, Any]:
    try:
        return json.loads(cache_file.read_text())
    except (OSError, json.JSONDecodeError):
        return {}


def _write_cache(cache_file: Path, data: dict[str, Any]) -> None:
    """Escrita atômica (tmp + rename) para não corromper em concorrência leve."""
    cache_file.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=cache_file.parent, prefix=".jev_cache_", suffix=".tmp")
    try:
        with os.fdopen(fd, "w") as fh:
            json.dump(data, fh, ensure_ascii=False)
        os.replace(tmp, cache_file)
    except OSError:
        try:
            os.unlink(tmp)
        except OSError:
            pass


def _journal(journal_file: Path, record: dict[str, Any]) -> None:
    """Append-only; falha de escrita nunca derruba a chamada (mas não é silenciada
    para o chamador humano: vai a stderr)."""
    try:
        journal_file.parent.mkdir(parents=True, exist_ok=True)
        with journal_file.open("a") as fh:
            fh.write(json.dumps(record, ensure_ascii=False) + "\n")
    except OSError as exc:
        import sys

        print(f"jev_client: journal write failed: {exc}", file=sys.stderr)


def _build_questions(specs: dict[str, dict[str, Any]]) -> dict[str, Any]:
    """Traduz specs planas (dicts) para os objetos pydantic do SDK.

    Isolar a forma do SDK aqui é o ponto: 0.6.0 e 0.7.0 quebraram compatibilidade;
    quem chama o adapter nunca vê isso.
    """
    if not _SDK_AVAILABLE:
        raise JevSpecError("sdk_missing")
    built: dict[str, Any] = {}
    for qid, spec in specs.items():
        qtype = spec.get("type")
        instructions = spec.get("instructions")
        if instructions is None:
            raise JevSpecError(f"{qid}: instructions ausente")
        criteria = spec.get("criteria")
        if qtype == "noul":
            built[qid] = Noul(instructions=instructions, **({"criteria": criteria} if criteria else {}))
        elif qtype == "choice":
            if not isinstance(criteria, dict) or not criteria:
                raise JevSpecError(f"{qid}: choice exige criteria (mapa opção→descrição)")
            built[qid] = Choice(instructions=instructions, criteria=criteria)
        elif qtype == "score":
            if not isinstance(criteria, (list, tuple)) or len(criteria) < 2:
                raise JevSpecError(f"{qid}: score exige criteria (lista de 2–10 níveis)")
            built[qid] = Score(instructions=instructions, criteria=list(criteria))
        else:
            raise JevSpecError(f"{qid}: tipo desconhecido {qtype!r}")
    return built


def _normalize_answers(raw_answers: Any) -> dict[str, dict[str, Any]]:
    """Extrai as respostas para dicts planos (type + payload + confidence quando há)."""
    out: dict[str, dict[str, Any]] = {}
    for qid, ans in dict(raw_answers).items():
        entry: dict[str, Any] = {"type": getattr(ans, "type", None)}
        for field in ("choice", "noul", "score", "probabilities", "legend", "confidence"):
            value = getattr(ans, field, None)
            if value is not None:
                entry[field] = value
        out[qid] = entry
    return out


def _answers_digest(answers: dict[str, dict[str, Any]]) -> dict[str, dict[str, Any]]:
    """Digest compacto por pergunta para o journal: tipo, valor, confidence, max_prob.

    O KPI `jev_confidence_hist` (W3) e as curvas ECE leem ESTE campo — o journal sem
    ele é um instrumento que mede chamadas mas não decisões (defeito achado na
    revisão da W1, 30/09: 100 chamadas sem confidence visível).
    """
    digest: dict[str, dict[str, Any]] = {}
    for qid, ans in answers.items():
        entry: dict[str, Any] = {"type": ans.get("type")}
        for field in ("choice", "noul", "score", "confidence"):
            if ans.get(field) is not None:
                entry[field] = ans[field]
        probs = ans.get("probabilities")
        if probs:
            entry["max_prob"] = max(probs.values())
        digest[qid] = entry
    return digest


def _make_client(key: str | None, timeout: float, base_url: str | None) -> Any:
    """Constrói o TypeSafeClient; `base_url` redireciona o contrato /v1/systemone."""
    kwargs: dict[str, Any] = {"api_key": key or "local", "timeout": timeout}
    if base_url:
        kwargs["base_url"] = base_url
    return TypeSafeClient(**kwargs)


def _invoke(
    client: Any,
    state: Any,
    built: dict[str, Any],
    model: str,
    base: dict[str, Any],
    cache_key: str,
    cache: dict[str, Any],
    cf: Path,
    jf: Path,
    started: float,
) -> dict[str, dict[str, Any]] | None:
    """Chama a API, registra latência/tokens/digest e atualiza o cache."""
    response = client.system_one(state=state, questions=built, model=model)
    latency_ms = (time.monotonic() - started) * 1000
    answers = _normalize_answers(response.answers)
    input_tokens = getattr(getattr(response, "usage", None), "input_tokens", None)
    _journal(
        jf,
        {
            **base,
            "route_taken": "api",
            "cache_key": cache_key,
            "latency_ms": round(latency_ms, 1),
            "input_tokens": input_tokens,
            "model_reported": getattr(response, "model", None),
            "answers_digest": _answers_digest(answers),
        },
    )
    cache[cache_key] = {"answers": answers, "model": model, "ts": base["ts"]}
    _write_cache(cf, cache)
    return answers


def system_one(
    state: Any,
    questions: dict[str, dict[str, Any]],
    *,
    model: str = DEFAULT_MODEL,
    timeout: float = DEFAULT_TIMEOUT_S,
    key_file: Path | None = None,
    cache_file: Path | None = None,
    journal_file: Path | None = None,
    client: Any | None = None,
    base_url: str | None = None,
) -> dict[str, dict[str, Any]] | None:
    """Avalia `state` contra `questions` tipadas. Devolve respostas normalizadas
    ou None (fail-open) — toda falha vira linha de journal com `fallback_reason`.

    `base_url` aponta o contrato `/v1/systemone` para outro servidor (o aluno
    Laya local, OpenRouter, Vercel) — contrato, não fornecedor. Com `base_url`,
    a chave pode ser None (servidor local sem bearer).
    Parâmetros de infraestrutura (key/cache/journal/client) são injetáveis para
    teste vendado (replay sem rede, padrão dos cookbooks oficiais).
    """
    jf = journal_file or DEFAULT_JOURNAL_FILE
    cf = cache_file or DEFAULT_CACHE_FILE
    base: dict[str, Any] = {
        "ts": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "model": model,
        "question_ids": sorted(questions.keys()),
    }
    if base_url:
        base["base_url"] = base_url

    def fail(reason: str) -> None:
        _journal(jf, {**base, "route_taken": "fallback", "fallback_reason": reason})

    key = load_key(key_file)
    if not key and client is None and not base_url:
        fail("no_key")
        return None
    if not _SDK_AVAILABLE and client is None:
        fail("sdk_missing")
        return None
    try:
        built = _build_questions(questions) if client is None else questions
    except JevSpecError as exc:
        fail(f"spec_error:{exc}")
        return None

    cache_key = _cache_key(state, questions, model + (base_url or ""))
    cache = _read_cache(cf)
    if cache_key in cache:
        cached_answers = cache[cache_key]["answers"]
        _journal(
            jf,
            {**base, "route_taken": "cache", "cache_key": cache_key, "answers_digest": _answers_digest(cached_answers)},
        )
        return cached_answers

    started = time.monotonic()
    try:
        if client is None:
            client = _make_client(key, timeout, base_url)
        return _invoke(client, state, built, model, base, cache_key, cache, cf, jf, started)
    except _SDK_ERRORS as exc:
        fail(f"{type(exc).__name__}:{getattr(exc, 'status_code', '')}")
        return None
    except Exception as exc:  # fail-open de última linha: a razão VAI ao journal
        fail(f"unexpected:{type(exc).__name__}")
        return None
