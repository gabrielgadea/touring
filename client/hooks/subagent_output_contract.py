#!/usr/bin/env python3
"""subagent_output_contract.py — SubagentStart + SubagentStop: the long deliverable goes to a file, the reply stays short.

WHY THIS EXISTS
===============
Recurring fact (Gabriel, 13/11/2026): "o desenho chegou cortado no meio da gramática; pedi ao arquiteto que grave
o texto integral em arquivo". A subagent that returns its whole deliverable as the final message hands the
orchestrator a text that can arrive cut, and that — when it arrives whole — is resent in every later turn of the
main conversation. Measured on 1,036 subagent transcripts of 21 days (13/11/2026): no final turn ended in
`max_tokens`, so the model did not run out of output; the final text is p50 2,075 chars, p90 9,377, p99 25,981,
max 87,758. The cut happens on delivery, and the remedy does not depend on where: the full text lives on disk.

THE CONTRACT (one source for the announcement and for the executor — D8)
========================================================================
A final message above `MAX_CHARS` characters is not a reply, it is a document: the subagent writes it INTEGRAL to
`~/claude-artefatos/<project>/subagentes/<agent_type>-<agent_id[:8]>.md` and replies with `report_path` plus a
summary. `SubagentStart` announces this to every subagent; `SubagentStop` applies it: a longer final message is
blocked ONCE (`decision: "block"`, the reason is the subagent's next instruction — code.claude.com/docs/en/hooks,
"SubagentStop"), and `stop_hook_active` lets the second stop through, so no subagent can loop here.

OUT OF SCOPE
============
Internal agents (empty `agent_type`: prompt suggestions, `/btw`), the human kill switch
`SUBAGENT_OUTPUT_CONTRACT_DISABLED=1`, and `SUBAGENT_OUTPUT_CONTRACT_MODE=shadow` (measures, never blocks). The
announcement obeys the context-injection switch (`TOURING_HOOK_INJECTION`); the block is enforcement and does not.
A subagent that delivers through the `SubagentHandback` tool is not seen: its `last_assistant_message` is only the
closing text (known limit).

Fail-open: any error lets the subagent stop (exit 0, nothing on stdout) and says so on stderr. Every SubagentStop
is one line in `~/.claude/touring/subagent_output_contract.jsonl` (size and verdict, never the text).

Registration: `client/claude/hooks.settings.json` (SubagentStart and SubagentStop, no matcher) →
`scripts/install_claude_hooks.py --apply`. Tests: `client/hooks/tests/test_subagent_output_contract.py`.
"""

from __future__ import annotations

import json
import os
import sys
from collections.abc import Mapping
from datetime import datetime
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

#: Above this many characters the final message is a document (≈ p88 of 1,036 measured final messages).
MAX_CHARS = 8000
#: Built-in types with no Write/Edit/Bash: they cannot save a file, so they are told to shorten instead.
READ_ONLY_TYPES = frozenset({"Explore", "Plan", "claude-code-guide", "statusline-setup"})
INJECTION_FAMILY = "plugin-hook"
LOG_NAME = "subagent_output_contract.jsonl"


def max_chars(env: Mapping[str, str]) -> int:
    """The limit: `SUBAGENT_OUTPUT_MAX_CHARS` when it is a positive integer, else the default."""
    raw = env.get("SUBAGENT_OUTPUT_MAX_CHARS", "").strip()
    return int(raw) if raw.isdigit() and int(raw) > 0 else MAX_CHARS


def report_file(payload: Mapping[str, Any], env: Mapping[str, str], home: Path) -> Path:
    """`~/claude-artefatos/<project>/subagentes/<agent_type>-<agent_id[:8]>.md` — outside every repository."""
    project = Path(env.get("CLAUDE_PROJECT_DIR") or str(payload.get("cwd") or "") or os.getcwd()).name or "sem-projeto"
    agent = "".join(c if c.isalnum() or c in "-_." else "-" for c in str(payload.get("agent_type") or "agente"))
    ident = "".join(c for c in str(payload.get("agent_id") or "") if c.isalnum())[:8] or "sem-id"
    return home / "claude-artefatos" / project / "subagentes" / f"{agent}-{ident}.md"


def contract_text(target: Path, limit: int) -> str:
    """The announcement a subagent gets at start: the same limit and the same file the stop hook will name."""
    return (
        f"[CONTRATO DE SAÍDA DO SUBAGENTE] Sua mensagem final tem teto de {limit} caracteres: acima disso ela pode "
        "chegar cortada ao orquestrador e é reenviada inteira a cada turno dele.\n"
        f"MUST: entregável longo (desenho, gramática, spec, relatório, tabela, diff) → grave o texto INTEGRAL em "
        f"{target} (crie outro arquivo na mesma pasta se houver mais de um) e responda só com `report_path` + um "
        "resumo: conclusão, decisões, pendências e as evidências `arquivo:linha` que sustentam cada uma.\n"
        "MUST: nunca resuma no arquivo o que o pedido quer inteiro; nunca cole na resposta o que já está no arquivo.\n"
        "Sem ferramenta de escrita (Explore/Plan): fique abaixo do teto — conclusões e `arquivo:linha`, sem transcrever "
        "trechos. O SubagentStop devolve uma resposta acima do teto uma vez, com esta mesma instrução."
    )


def block_reason(chars: int, limit: int, target: Path, agent_type: str) -> str:
    """The stop deny: what was refused, by how much, and the exact way back for this agent."""
    head = (f"[subagent-output-contract] Sua mensagem final tem {chars} caracteres; o teto é {limit}. Acima dele o "
            "texto pode chegar cortado ao orquestrador, e o que chega é reenviado em todo turno seguinte.")
    if agent_type in READ_ONLY_TYPES:
        return (f"{head}\nVocê está sem ferramenta de escrita ({agent_type}): reescreva a resposta abaixo de {limit} "
                "caracteres — conclusões, decisões e as evidências `arquivo:linha`, sem transcrever trechos longos. "
                "Diga o que ficou de fora, para o orquestrador pedir a parte que faltar.")
    return (f"{head}\n1. Grave AGORA o texto INTEGRAL que você ia entregar em {target} (a pasta já existe; sem resumir, "
            "sem cortar).\n2. Responda de novo, abaixo do teto, com `report_path` (esse caminho) + resumo: conclusão, "
            "decisões, pendências e as evidências `arquivo:linha`. Se o seu formato de saída é JSON, `report_path` é um "
            "campo dele.\nSem como gravar arquivo: reescreva a resposta abaixo do teto e diga o que ficou de fora.")


def log(home: Path, record: dict[str, Any]) -> None:
    """Append one measurement. Never raises: the log is best-effort and never changes the verdict."""
    path = home / ".claude" / "touring" / LOG_NAME
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(record, ensure_ascii=True) + "\n")
    except OSError as exc:
        sys.stderr.write(f"subagent_output_contract: log indisponível ({type(exc).__name__})\n")


def on_start(payload: Mapping[str, Any], env: Mapping[str, str], home: Path) -> dict[str, Any] | None:
    """SubagentStart: announce the contract, unless the injection switch cuts this family."""
    import injection_policy  # beside this file: its directory is on sys.path since the top of the module
    if not injection_policy.allows(INJECTION_FAMILY):
        return None
    text = contract_text(report_file(payload, env, home), max_chars(env))
    return {"hookSpecificOutput": {"hookEventName": "SubagentStart", "additionalContext": text}}


def on_stop(payload: Mapping[str, Any], env: Mapping[str, str], home: Path) -> dict[str, Any] | None:
    """SubagentStop: measure the final message; block it once when it is a document."""
    message = payload.get("last_assistant_message")
    if not isinstance(message, str):
        return None
    limit, chars, agent_type = max_chars(env), len(message), str(payload.get("agent_type"))
    record: dict[str, Any] = {"ts": datetime.now().astimezone().isoformat(timespec="seconds"),
                              "session_id": payload.get("session_id"), "agent_id": payload.get("agent_id"),
                              "agent_type": agent_type, "chars": chars, "limit": limit}
    if chars <= limit:
        verdict = "ok"
    elif payload.get("stop_hook_active"):
        verdict = "long-after-block"  # already told once: a second block would be a loop
    elif env.get("SUBAGENT_OUTPUT_CONTRACT_MODE", "block").strip().lower() == "shadow":
        verdict = "shadow-would-block"
    else:
        verdict = "block"
    log(home, {**record, "verdict": verdict})
    if verdict != "block":
        return None
    target = report_file(payload, env, home)
    try:
        target.parent.mkdir(parents=True, exist_ok=True)
    except OSError as exc:  # the reason still names the path; the subagent's own mkdir may succeed
        sys.stderr.write(f"subagent_output_contract: pasta não criada ({type(exc).__name__})\n")
    return {"decision": "block", "reason": block_reason(chars, limit, target, agent_type)}


def main() -> int:
    try:
        payload = json.loads(sys.stdin.buffer.read().decode("utf-8", errors="replace"))
        if not isinstance(payload, dict) or os.environ.get("SUBAGENT_OUTPUT_CONTRACT_DISABLED") == "1":
            return 0
        if not str(payload.get("agent_type") or ""):
            return 0  # Claude Code's own internal agents: not a delegated subagent
        handler = {"SubagentStart": on_start, "SubagentStop": on_stop}.get(str(payload.get("hook_event_name")))
        output = handler(payload, os.environ, Path.home()) if handler else None
        if output is not None:
            sys.stdout.write(json.dumps(output, ensure_ascii=False))
            sys.stdout.flush()
    except Exception as exc:  # noqa: BLE001 — fail-open by contract: a bug here must never trap a subagent
        sys.stderr.write(f"subagent_output_contract: erro interno, subagente liberado ({type(exc).__name__}: {exc})\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
