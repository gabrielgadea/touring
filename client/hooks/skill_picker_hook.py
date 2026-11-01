#!/usr/bin/env python3
"""skill_picker_hook — ponte assíncrona UserPromptSubmit → scripts/skill_picker.py.

Decisão de Gabriel (01/10, S-5.1 registration). O juiz (`--judge`) custa ~370 ms,
incompatível com UserPromptSubmit síncrono (medição da touring-02). Por isso:
a sugestão é computada em BACKGROUND no prompt N e entregue no prompt N+1.

Contrato (experiments/s5-skill-picker/CONTRACT.md):
- `said: false` → silêncio ABSOLUTO no contexto (nenhuma mensagem);
- `silent_reason` SEMPRE registrado no journal (skill_suggestion_journal.jsonl)
  — um hook que só loga quando fala produz journal vazio indistinguível de
  "gate quebrado" (unmeasured ≠ 0);
- fail-open estrutural: qualquer falha vira linha de journal, nunca exceção.
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

PICKER = Path("/home/gabrielgadea/projects/touring/scripts/skill_picker.py")
SUGGESTION_FILE = Path("~/.claude/touring/skill_suggestion.json").expanduser()
JOURNAL_FILE = Path("~/.claude/touring/logs/skill_suggestion_journal.jsonl").expanduser()
PICKER_TIMEOUT_S = 60


def _journal(record: dict) -> None:
    """Append-only, um write por linha (a lição da intercalação). Fail-open."""
    try:
        JOURNAL_FILE.parent.mkdir(parents=True, exist_ok=True)
        with JOURNAL_FILE.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(record, ensure_ascii=False) + "\n")
    except OSError:
        pass


def _deliver_pending_suggestion() -> dict | None:
    """Lê a sugestão do prompt ANTERIOR (se houver) e a consome."""
    try:
        if not SUGGESTION_FILE.exists():
            return None
        data = json.loads(SUGGESTION_FILE.read_text())
        SUGGESTION_FILE.unlink()
    except (OSError, json.JSONDecodeError):
        return None
    if not data.get("said") or not data.get("picked"):
        return None
    return {
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": (
                f"[SUGESTÃO DE SKILL: `{data['picked']}` (confiança {data.get('confidence', '?')}) "
                "— avaliada por decisão tipada sobre o prompt anterior. Invoque com "
                f"`/{data['picked']}` se fizer sentido, ou ignore: o gate de silêncio "
                "só deixa passar sugestões acima do τ medido (0,55).]"
            ),
        }
    }


def _spawn_background_judge(prompt: str) -> None:
    """Dispara o juiz para o PRÓXIMO prompt. Nunca espera; nunca levanta."""
    try:
        proc = subprocess.Popen(
            [sys.executable, str(PICKER), "--query", prompt, "--judge", "--json"],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        out, _ = proc.communicate(timeout=PICKER_TIMEOUT_S)
        data = json.loads(out)
        record = {
            "ts": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "said": bool(data.get("said")),
            "silent_reason": data.get("silent_reason"),
            "picked": data.get("picked"),
            "confidence": data.get("confidence"),
        }
        _journal(record)
        if record["said"]:
            SUGGESTION_FILE.write_text(json.dumps(data, ensure_ascii=False))
    except (subprocess.TimeoutExpired, json.JSONDecodeError, OSError) as exc:
        _journal({
            "ts": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "said": False,
            "silent_reason": f"hook_fail_open:{type(exc).__name__}",
        })


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        payload = {}
    prompt = str(payload.get("prompt", ""))
    suggestion = _deliver_pending_suggestion()
    if prompt and not prompt.startswith("<"):
        _spawn_background_judge(prompt)
    if suggestion:
        # Interruptor de injeção de contexto (26/10/2026) — client/hooks/injection_policy.py
        import sys as _sys; _sys.path.insert(0, __import__("os").path.expanduser("~/.claude/hooks"))
        import injection_policy
        if not injection_policy.allows("plugin-hook"):
            return 0
        # S-4p (26/10/2026): dedupe por época via ponte Rust — fail-open.
        import injection_dedup
        print(injection_dedup.dedup_output(
            json.dumps(suggestion),
            session_id=str(payload.get("session_id") or ""),
            cwd=str(payload.get("cwd") or ""),
        ))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001 — fail-open de última linha: hooks nunca quebram a sessão
        sys.stderr.write(f"skill_picker_hook: sugestão não processada ({type(exc).__name__}: {exc})\n")
        sys.exit(0)
