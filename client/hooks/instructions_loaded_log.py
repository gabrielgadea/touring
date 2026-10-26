#!/usr/bin/env python3
"""Hook InstructionsLoaded — logger mínimo de contexto por sessão (I7, plano F5).

Proveniência: docs/plans/2026-10-22-afordancia-pipeline/plano_f5.md §S-5 (aprovado em D2,
22/10/2026). O handler canônico (`touring-hook instructions-loaded`) pretendia gravar em
`activity.jsonl`, mas o gate lê `matcher` e o harness envia `load_reason` — medido em
22/10: 0 eventos `instructions_loaded` contra 137 de `post_edit` no mesmo arquivo (o canal
funciona; o gate está morto). Este logger cobre a lacuna SEM tocar a toolchain Touring.

Contrato: lê o payload JSON do stdin (protocolo de hook do Claude Code:
`hook_event_name`, `session_id`, `file_path`, `memory_type`, `load_reason`), mede os bytes
do arquivo carregado (stat no momento do evento) e acrescenta UMA linha JSON em
`~/.claude/touring/instructions_loaded.jsonl` (append + fsync). Agregação:
`scripts/diagnostics/kpi_contexto_sessoes.py` (repo analise).

Fail-open observável: nunca bloqueia a sessão (sempre exit 0 e `{}` no stdout), mas uma
falha de escrita vira linha `{"erro": ...}` no próprio log — silêncio aqui foi exatamente
o defeito do handler Rust que este logger substitui.

Reversão: remover a entrada `instructions_loaded_log.py` do bloco `InstructionsLoaded` em
`~/.claude/settings.json` e apagar este arquivo.
"""

import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

LOG = Path.home() / ".claude" / "touring" / "instructions_loaded.jsonl"


def _registrar(linha: dict) -> None:
    LOG.parent.mkdir(parents=True, exist_ok=True)
    with open(LOG, "a", encoding="utf-8") as fh:
        fh.write(json.dumps(linha, ensure_ascii=False) + "\n")
        fh.flush()
        os.fsync(fh.fileno())


def _bytes_da_fonte(file_path: str) -> int | None:
    """Bytes do arquivo no momento do evento. Não legível → None (null, nunca zero)."""
    if not file_path:
        return None
    try:
        return os.path.getsize(file_path)
    except OSError:
        return None


def _linha_do_evento(payload: dict) -> dict:
    file_path = str(payload.get("file_path") or "")
    return {
        "ts": datetime.now(timezone.utc).isoformat(),
        "session_id": payload.get("session_id"),
        "file_path": file_path or None,
        "memory_type": payload.get("memory_type"),
        "load_reason": payload.get("load_reason"),
        "bytes": _bytes_da_fonte(file_path),
        "cwd": payload.get("cwd"),
    }


def _registrar_erro(exc: Exception) -> None:
    try:
        _registrar(
            {
                "ts": datetime.now(timezone.utc).isoformat(),
                "erro": f"{type(exc).__name__}: {exc}",
            }
        )
    except Exception:
        pass


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        print("{}")
        return 0
    try:
        _registrar(_linha_do_evento(payload))
    except Exception as exc:  # fail-open observável: a falha vira dado, nunca silêncio
        _registrar_erro(exc)
    print("{}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
