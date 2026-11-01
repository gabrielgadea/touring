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


def load_key(key_file: Path | None = None) -> str | None:
    """Lê a chave do ambiente ou do arquivo de env dedicado. Nunca loga o valor."""
    env_val = os.environ.get("TYPESAFE_API_KEY")
    if env_val:
        return env_val
    path = key_file or DEFAULT_KEY_FILE
    try:
        for line in path.read_text().splitlines():
