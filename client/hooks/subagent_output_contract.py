#!/usr/bin/env python3
"""subagent_output_contract.py — every subagent delivery is kept whole: on disk, in memory, and where the parent can find it.

WHY THIS EXISTS
===============
Recurring fact (Gabriel, 13/11/2026): "o desenho chegou cortado no meio da gramática; pedi ao arquiteto que grave
o texto integral em arquivo". The first answer here was a character ceiling on the final message. Gabriel's
correction, same day: "a solução não é a especificação e teto de caracteres, mas sim a fidelidade e riqueza do
conteúdo que deve ser transmitido e a persistência e memória desse conteúdo". A ceiling makes the reply shorter;
it does not make the content survive. What survives is what the EXECUTOR writes down.

Measured on 1,036 subagent transcripts of 21 days: no final turn ended in `max_tokens` — the subagent wrote its
whole text and the cut happened on delivery. So the whole text exists at `SubagentStop`, in
`last_assistant_message`, and this hook is where it is kept.

WHAT IT DOES (three events, one module)
=======================================
* `SubagentStop` — FIDELITY AND PERSISTENCE, by the executor, for every delivery of every size: the final message
  is written INTEGRAL, byte for byte, to `~/claude-artefatos/<project>/subagentes/<agent_type>-<agent_id>.md`
  under a provenance header (session, agent, time, characters, sha256 of the text). A second delivery of the same
  agent is appended, never overwritten. MEMORY: a delivery of `MEMORY_MIN_CHARS` or more gets a Touring memory
  entry (`entrega-subagente:<agent_type>-<agent_id>`, tags `#kind:deliverable #artifact:report`) holding the path,
  the hash and the opening of the text, so a later session finds it by `touring memory recall`.
* `SubagentStart` — RICHNESS: the subagent is told that its delivery must be complete and faithful (nothing the
  request wants whole is summarized away), that its final message is archived whole at that path, and that
  further documents go beside it.
* `PostToolUse` on `Agent` — the PARENT is told where the whole text of the agent it just launched will be, so a
  reply that arrives cut is read from disk instead of asked for again.

The ceiling survives only as an opt-in (`SUBAGENT_OUTPUT_CONTRACT_MODE=block`, `SUBAGENT_OUTPUT_MAX_CHARS`): then
a final message above it is sent back once to be moved into a file. It is off by default.

OUT OF SCOPE
============
Internal agents (empty `agent_type`), the human kill switch `SUBAGENT_OUTPUT_CONTRACT_DISABLED=1`, and a delivery
made through the `SubagentHandback` tool, which `last_assistant_message` does not carry (known limit). The two
announcements obey the context-injection switch (`TOURING_HOOK_INJECTION`); archiving does not — it is not context.

Fail-open: any error lets the subagent stop and says so on stderr. Every SubagentStop is one line in
`~/.claude/touring/subagent_output_contract.jsonl` (size, hash, path, verdict — never the text).

Registration: `client/claude/hooks.settings.json` (SubagentStart, SubagentStop, PostToolUse `Agent`) →
`scripts/install_claude_hooks.py --apply`. Tests: `client/hooks/tests/test_subagent_output_contract.py`.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
from collections.abc import Mapping
from datetime import datetime
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

#: Opt-in ceiling of the final message (`SUBAGENT_OUTPUT_CONTRACT_MODE=block`); ≈ p88 of 1,036 measured messages.
MAX_CHARS = 8000
#: A delivery this long gets a memory entry; shorter ones ("PRONTO", a verdict line) are archived but not indexed.
MEMORY_MIN_CHARS = 1000
MEMORY_HEAD_CHARS = 600
MEMORY_TIMEOUT_S = 3
#: Built-in types with no Write/Edit/Bash: they cannot save a file of their own.
READ_ONLY_TYPES = frozenset({"Explore", "Plan", "claude-code-guide", "statusline-setup"})
INJECTION_FAMILY = "plugin-hook"
LOG_NAME = "subagent_output_contract.jsonl"
AGENT_ID = re.compile(r'"?agent_?[iI]d"?\s*[:=]\s*"?([A-Za-z0-9][A-Za-z0-9_-]{5,80})')


def _slug(text: object, fallback: str) -> str:
    """A file-name-safe form of `text`: letters, digits, `-`, `_`, `.`; `fallback` when nothing is left."""
    return "".join(c if c.isalnum() or c in "-_." else "-" for c in str(text or "")).strip("-.")[:60] or fallback


def deliveries_dir(payload: Mapping[str, Any], env: Mapping[str, str], home: Path) -> Path:
    """`~/claude-artefatos/<project>/subagentes` — outside every repository, durable across sessions."""
    project = Path(env.get("CLAUDE_PROJECT_DIR") or str(payload.get("cwd") or "") or os.getcwd()).name
    return home / "claude-artefatos" / _slug(project, "sem-projeto") / "subagentes"


def report_file(payload: Mapping[str, Any], env: Mapping[str, str], home: Path) -> Path:
    """Where this agent's whole delivery is kept: `<agent_type>-<agent_id>.md`, unique per agent."""
    name = f"{_slug(payload.get('agent_type'), 'agente')}-{_slug(payload.get('agent_id'), 'sem-id')}.md"
    return deliveries_dir(payload, env, home) / name


def max_chars(env: Mapping[str, str]) -> int:
    """The opt-in ceiling: `SUBAGENT_OUTPUT_MAX_CHARS` when it is a positive integer, else the default."""
    raw = env.get("SUBAGENT_OUTPUT_MAX_CHARS", "").strip()
    return int(raw) if raw.isdigit() and int(raw) > 0 else MAX_CHARS


def blocking(env: Mapping[str, str]) -> bool:
    return env.get("SUBAGENT_OUTPUT_CONTRACT_MODE", "").strip().lower() == "block"


def archive(target: Path, message: str, payload: Mapping[str, Any], now: datetime) -> str:
    """Append the delivery, whole, under a provenance header; returns the sha256 of the text. Never overwrites."""
    digest = hashlib.sha256(message.encode("utf-8", errors="surrogatepass")).hexdigest()
    header = (f"<!-- entrega-subagente · agent_type={payload.get('agent_type')} · agent_id={payload.get('agent_id')} · "
              f"session_id={payload.get('session_id')} · {now.isoformat(timespec='seconds')} · chars={len(message)} · "
              f"sha256={digest} -->\n")
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open("a", encoding="utf-8", errors="surrogatepass") as fh:
        if fh.tell():
            fh.write("\n\n")
        fh.write(header + message + "\n")
    return digest


def remember(target: Path, message: str, digest: str, payload: Mapping[str, Any], env: Mapping[str, str]) -> str:
    """Index the delivery in Touring memory: path, hash and opening. Returns `stored`, `skipped` or `failed:<why>`."""
    if len(message) < MEMORY_MIN_CHARS or env.get("SUBAGENT_OUTPUT_MEMORY_DISABLED") == "1":
        return "skipped"
    head = " ".join(message[:MEMORY_HEAD_CHARS].split())
    value = (f"Entrega integral do subagente {payload.get('agent_type')} ({len(message)} caracteres, sha256 {digest[:16]}) "
             f"em {target} — sessão {payload.get('session_id')}. Abre com: {head}")
    command = [env.get("TOURING_BIN") or "touring", "memory", "store", f"entrega-subagente:{target.stem}", value,
               "--tier", "episodic", "--type", "deliverable", "--tag", "#kind:deliverable", "--tag", "#artifact:report"]
    try:
        done = subprocess.run(command, capture_output=True, timeout=MEMORY_TIMEOUT_S, check=False)
    except (OSError, subprocess.SubprocessError) as exc:
        return f"failed:{type(exc).__name__}"
    return "stored" if done.returncode == 0 else f"failed:rc{done.returncode}"


def contract_text(target: Path, read_only: bool) -> str:
    """What a subagent is told at start: deliver whole and faithful; the harness keeps it; further files go beside."""
    own_files = ("Você está sem ferramenta de escrita: ponha TUDO na mensagem final — ela é arquivada inteira."
                 if read_only else
                 f"Documento que mereça arquivo próprio (desenho, gramática, spec, tabela, diff): grave-o INTEGRAL em "
                 f"{target.parent}/ com nome `{target.stem}-<assunto>.md` e cite o caminho na resposta.")
    return (
        "[CONTRATO DE ENTREGA DO SUBAGENTE] O que vale é a FIDELIDADE e a RIQUEZA do conteúdo, não o tamanho da resposta.\n"
        "MUST: entregue o conteúdo COMPLETO que o pedido quer — o texto integral do desenho, da gramática, do relatório; "
        "cada decisão com a razão; cada afirmação com a evidência `arquivo:linha` ou a saída do comando. Nunca troque o "
        "conteúdo por um resumo dele, nem escreva \"o restante segue o mesmo padrão\".\n"
        f"Persistência: sua mensagem final é arquivada INTEIRA pelo harness em {target} (com sha256) e indexada na "
        "memória do Touring; o orquestrador lê de lá o que chegar cortado, e uma sessão futura a recupera por "
        "`touring memory recall`. " + own_files + "\n"
        "MUST: abra a resposta pelo que foi concluído e decidido, e liste os arquivos que você gravou — é o mapa do "
        "conteúdo, não um substituto dele."
    )


def parent_text(directory: Path, agent_id: str) -> str:
    """What the parent is told when it launches an agent: where the whole delivery will be, and how to recall it."""
    return (f"[entrega-subagente] O texto INTEGRAL da resposta deste agente fica em {directory}/*-{_slug(agent_id, 'sem-id')}.md "
            "quando ele terminar (cabeçalho com sha256), e entra na memória do Touring como `entrega-subagente:…`. "
            "Resposta que chegar cortada: leia o arquivo, não peça de novo.")


def block_reason(chars: int, limit: int, target: Path, agent_type: str) -> str:
    """The opt-in ceiling's deny: what was refused, by how much, and the exact way back for this agent."""
    head = (f"[subagent-output-contract] Sua mensagem final tem {chars} caracteres; o teto ligado nesta sessão é {limit}. "
            "O texto integral dela já está arquivado; falta a resposta ser o mapa dele.")
    if agent_type in READ_ONLY_TYPES:
        return (f"{head}\nVocê está sem ferramenta de escrita ({agent_type}): responda de novo abaixo de {limit} "
                f"caracteres com as conclusões e as evidências `arquivo:linha`, e cite {target} como o texto integral.")
    return (f"{head}\nResponda de novo abaixo do teto com `report_path` = {target} (o texto INTEGRAL, já gravado pelo "
            "harness) + conclusão, decisões, pendências e as evidências `arquivo:linha`. Nada do conteúdo se perde: "
            "ele está no arquivo.")


def log(home: Path, record: dict[str, Any]) -> None:
    """Append one measurement. Never raises: the log is best-effort and never changes the verdict."""
    path = home / ".claude" / "touring" / LOG_NAME
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(record, ensure_ascii=True) + "\n")
    except OSError as exc:
        sys.stderr.write(f"subagent_output_contract: log indisponível ({type(exc).__name__})\n")


def _allowed_to_inject() -> bool:
    import injection_policy  # beside this file: its directory is on sys.path since the top of the module
    return bool(injection_policy.allows(INJECTION_FAMILY))


def on_start(payload: Mapping[str, Any], env: Mapping[str, str], home: Path) -> dict[str, Any] | None:
    """SubagentStart: tell the subagent what a faithful delivery is and where it is kept."""
    if not str(payload.get("agent_type") or "") or not _allowed_to_inject():
        return None  # an empty type is one of Claude Code's own internal agents
    text = contract_text(report_file(payload, env, home), str(payload.get("agent_type")) in READ_ONLY_TYPES)
    return {"hookSpecificOutput": {"hookEventName": "SubagentStart", "additionalContext": text}}


def on_stop(payload: Mapping[str, Any], env: Mapping[str, str], home: Path) -> dict[str, Any] | None:
    """SubagentStop: keep the delivery whole (disk + memory); only the opt-in ceiling can send it back, once."""
    message = payload.get("last_assistant_message")
    agent_type = str(payload.get("agent_type") or "")
    if not agent_type or not isinstance(message, str) or not message.strip():
        return None
    now, target, chars = datetime.now().astimezone(), report_file(payload, env, home), len(message)
    record: dict[str, Any] = {"ts": now.isoformat(timespec="seconds"), "session_id": payload.get("session_id"),
                              "agent_id": payload.get("agent_id"), "agent_type": agent_type, "chars": chars}
    try:
        digest = archive(target, message, payload, now)
        record.update(path=str(target), sha256=digest, memory=remember(target, message, digest, payload, env))
    except OSError as exc:  # the delivery still reaches the parent the usual way; the loss is named, never silent
        record.update(archive_error=type(exc).__name__)
        sys.stderr.write(f"subagent_output_contract: entrega NÃO arquivada em {target} ({type(exc).__name__}: {exc})\n")
    limit = max_chars(env)
    over = blocking(env) and chars > limit
    verdict = "ok" if not over else ("long-after-block" if payload.get("stop_hook_active") else "block")
    log(home, {**record, "verdict": verdict})
    if verdict != "block":
        return None
    return {"decision": "block", "reason": block_reason(chars, limit, target, agent_type)}


def on_agent_launched(payload: Mapping[str, Any], env: Mapping[str, str], home: Path) -> dict[str, Any] | None:
    """PostToolUse(Agent): tell the parent where the whole delivery of the agent it launched will be."""
    if payload.get("tool_name") != "Agent" or not _allowed_to_inject():
        return None
    found = AGENT_ID.search(json.dumps(payload.get("tool_response"), ensure_ascii=False, default=str))
    if not found:
        return None  # no id in the response: nothing true to say about a path
    text = parent_text(deliveries_dir(payload, env, home), found.group(1))
    return {"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": text}}


HANDLERS = {"SubagentStart": on_start, "SubagentStop": on_stop, "PostToolUse": on_agent_launched}


def main() -> int:
    try:
        payload = json.loads(sys.stdin.buffer.read().decode("utf-8", errors="replace"))
        if not isinstance(payload, dict) or os.environ.get("SUBAGENT_OUTPUT_CONTRACT_DISABLED") == "1":
            return 0
        handler = HANDLERS.get(str(payload.get("hook_event_name")))
        output = handler(payload, os.environ, Path.home()) if handler else None
        if output is not None:
            sys.stdout.write(json.dumps(output, ensure_ascii=False))
            sys.stdout.flush()
    except Exception as exc:  # noqa: BLE001 — fail-open by contract: a bug here must never trap a subagent
        sys.stderr.write(f"subagent_output_contract: erro interno, subagente liberado ({type(exc).__name__}: {exc})\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
