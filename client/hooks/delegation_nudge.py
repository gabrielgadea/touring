#!/usr/bin/env python3
"""delegation_nudge.py — PreToolUse(Read|Grep|Glob) + UserPromptSubmit: induz delegar; só aconselha, nunca bloqueia.

POR QUE ISTO EXISTE
===================
O `agent_model_gate` fiscaliza a escolha do modelo QUANDO se delega. Nada induz delegar: desde
08/10/2026 o haiku foi 0,2% dos tokens Anthropic (régua deduplicada por message.id do
`scripts/model_routing_adherence.py`), porque o Opus da sessão principal lê e
busca sozinho — e cada KB lido entra no contexto reenviado em todo turno seguinte
(docs/economia-de-creditos-e-contexto.md §0 orçamento 4, §5). Prosa já falhou como persuasão
(nudges MUST ignorados), então este hook entrega o PROGRAMA pronto: o `Agent(...)` derivado dos
alvos reais da rajada. Enforcement vive no gate; aqui é afordância.

DOIS AVISOS (ambos aditivos, `additionalContext`, jamais `permissionDecision`)
================================================================================
1. `PreToolUse` em `Read|Grep|Glob`: janela deslizante de 10 min por sessão com as chamadas e o
   volume estimado (Read = tamanho do arquivo, limitado por `limit` linhas — ou pelo teto padrão de
   2000 linhas do Read — a 80 B/linha; Grep/Glob = 1 chamada, volume 0). Com ≥ 6 chamadas OU ≥ 150 KB
   na janela devolve 1 aviso, no máximo 1 a cada 10 min por sessão: quantas chamadas/KB, os 3
   alvos mais recentes e o `Agent(subagent_type="Explore", model="haiku", ...)` / `touring run`.
2. `UserPromptSubmit`: lê só a cauda (~200 linhas) do transcript; se os turnos assistentes da sessão
   principal mostram mais de uma família Anthropic (opus/sonnet/haiku/fable), avisa que a troca de
   modelo no meio invalida o cache (cada modelo tem o próprio) e que o effort sobe antes (regra 17).
   1 aviso por troca; troca com mais de 30 min não é notícia.

QUEM É AVISADO: só a sessão PRINCIPAL de backend Anthropic. Fora: kill switch humano
`DELEGATION_NUDGE_DISABLED=1`; sessão irmã Kimi/MiniMax (o `provider_of` do `backend_provider`, o
mesmo predicado do gate — a matriz não vale nelas); subagente. Subagente = o JSON de entrada traz
`agent_id`/`agent_type` ("Unique identifier for the subagent or in-process teammate the hook fires
in" — code.claude.com/docs/en/hooks#common-input-fields, lida em 17/10/2026); de reserva,
`/subagents/` no `transcript_path` (layout observado: `<projeto>/<sessão>/subagents/agent-*.jsonl`).

ESTADO E LOG: `~/.claude/touring/delegation_nudge/<session_id>.json` (lido-modificado-escrito sob
`flock`: Read/Grep/Glob disparam em paralelo) e uma linha por AVISO em
`~/.claude/touring/delegation_nudge.jsonl` (`ts, session, evento, n, kb, alvos`; erro interno vira
`evento: "erro"`). Erro interno nunca tem saída: exit 0 sem stdout. Orçamento < 50 ms por chamada.

Registro (fonte do deploy, feito pela coordenadora): PreToolUse matcher `Read|Grep|Glob` e
UserPromptSubmit, ambos `python3 $HOME/.claude/hooks/delegation_nudge.py`, timeout 5.
Testes: `client/hooks/tests/test_delegation_nudge.py`.
"""

from __future__ import annotations

import contextlib
import fcntl
import json
import os
import sys
import time
from collections.abc import Callable, Mapping
from datetime import datetime, timezone
from itertools import pairwise
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
# One predicate and one family classifier for every hook (D8).
from backend_provider import SISTER_PROVIDERS, model_family, provider_of

READ_TOOLS = frozenset({"Read", "Grep", "Glob"})
WINDOW_S = 600  # sliding window of reads
COOLDOWN_S = 600  # at most one read advice per session per window
MAX_CALLS = 6
MAX_KB = 150
BYTES_PER_LINE = 80  # estimate: a source line plus the cat -n prefix
DEFAULT_READ_LINES = 2000  # the Read tool's own cap when `limit` is absent
TAIL_LINES = 200
TAIL_BYTES = 1_048_576
MAX_LINE_BYTES = 1_048_576
SWITCH_RECENCY_S = 1800
KEEP_SWITCHES = 20
STATE_TTL_S = 7 * 86_400
TARGET_CHARS = 60
MATRIX_REF = "docs/economia-de-creditos-e-contexto.md"


def safe_id(session: object) -> str | None:
    """A file-name-safe session id, or None (no state is kept without one)."""
    if not isinstance(session, str) or not session:
        return None
    return "".join(c if c.isalnum() or c in "-_" else "_" for c in session)[:80] or None


def skip_reason(payload: Mapping[str, Any], env: Mapping[str, str]) -> str | None:
    """Why this session is out of scope: the human switch, a sister backend, or a subagent."""
    if env.get("DELEGATION_NUDGE_DISABLED") == "1":
        return "disabled"
    provider = provider_of(env.get("ANTHROPIC_BASE_URL"))
    if provider in SISTER_PROVIDERS:
        return f"irmã:{provider}"
    if payload.get("agent_id") or "/subagents/" in str(payload.get("transcript_path") or ""):
        return "subagente"
    return None


def _number(value: object) -> float | None:
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def short_path(path: str, cwd: str | None) -> str:
    """`cwd`-relative when under it, `~`-relative when under home, tail-truncated to TARGET_CHARS."""
    for base, prefix in ((cwd, ""), (str(Path.home()), "~/")):
        if base and path.startswith(base.rstrip("/") + "/"):
            path = prefix + path[len(base.rstrip("/")) + 1:]
            break
    return path if len(path) <= TARGET_CHARS else "…" + path[-(TARGET_CHARS - 1):]


def _search_target(tool_name: str, args: Mapping[str, Any], cwd: str | None) -> str:
    """How the advice names a Grep/Glob call: the pattern (and the scope, for Grep)."""
    pattern = str(args.get("pattern") or "")[:40]
    if tool_name == "Glob":
        return f"Glob {pattern}"
    scope = f" em {short_path(args['path'], cwd)}" if isinstance(args.get("path"), str) else ""
    return f'Grep "{pattern}"{scope}'


def read_cost(tool_name: str, tool_input: object, cwd: str | None) -> tuple[float, str]:
    """(estimated KB entering the context, how the advice names this call)."""
    args = tool_input if isinstance(tool_input, dict) else {}
    if tool_name != "Read":
        return 0.0, _search_target(tool_name, args, cwd)
    path = args.get("file_path")
    if not isinstance(path, str):
        return 0.0, "Read"
    limit = _number(args.get("limit"))
    cap = (limit if limit and limit > 0 else DEFAULT_READ_LINES) * BYTES_PER_LINE
    try:
        size = os.stat(path).st_size
    except OSError:
        size = 0
    return min(size, cap) / 1024, short_path(path, cwd)


def _clean_calls(raw: object) -> list[list[Any]]:
    """[epoch, KB, target] entries that are well-formed; anything else is dropped."""
    entries = raw if isinstance(raw, list) else []
    return [[float(c[0]), float(c[1]), str(c[2])] for c in entries
            if isinstance(c, list) and len(c) == 3 and _number(c[0]) is not None and _number(c[1]) is not None]


def clean_state(raw: object) -> dict[str, Any]:
    """The state with only well-formed fields: a corrupt file degrades to an empty window, never a crash."""
    data = raw if isinstance(raw, dict) else {}
    switches = data.get("switches")
    return {"calls": _clean_calls(data.get("calls")),
            "last_nudge": _number(data.get("last_nudge")) or 0.0,
            "switches": [s for s in switches if isinstance(s, str)] if isinstance(switches, list) else []}


def with_state[T](home: Path, session: str, mutate: Callable[[dict[str, Any]], T]) -> T:
    """Read-modify-write the session state under an exclusive flock (the file itself is the lock)."""
    directory = home / ".claude" / "touring" / "delegation_nudge"
    directory.mkdir(parents=True, exist_ok=True)
    fd = os.open(directory / f"{session}.json", os.O_CREAT | os.O_RDWR, 0o600)
    first_call = os.fstat(fd).st_size == 0
    with os.fdopen(fd, "r+", encoding="utf-8") as fh:
        fcntl.flock(fh, fcntl.LOCK_EX)
        try:
            state = clean_state(json.loads(fh.read() or "null"))
        except ValueError:
            state = clean_state(None)
        result = mutate(state)
        fh.seek(0)
        fh.truncate()
        fh.write(json.dumps(state, ensure_ascii=True))
    if first_call:
        prune_stale_states(directory, time.time())
    return result


def prune_stale_states(directory: Path, now: float) -> None:
    """Sessions end without a goodbye: a new session drops state files untouched for a week."""
    with contextlib.suppress(OSError):
        for entry in os.scandir(directory):
            if entry.name.endswith(".json") and now - entry.stat().st_mtime > STATE_TTL_S:
                os.unlink(entry.path)


def recent_targets(calls: list[list[Any]], limit: int = 3) -> list[str]:
    """The `limit` most recent distinct targets, newest first."""
    seen: list[str] = []
    for _, _, target in reversed(calls):
        if target not in seen:
            seen.append(target)
        if len(seen) == limit:
            break
    return seen


def read_advice(n: int, kb: float, minutes: int, targets: list[str]) -> str:
    named = ", ".join(targets)
    return (
        f"[delegation-nudge] {n} leituras (Read/Grep/Glob) em {minutes} min, ~{kb:.0f} KB direto no contexto desta "
        f"sessão — cada KB é reenviado em todo turno seguinte. Mais recentes: {' · '.join(targets)}.\n"
        f'Delegue: Agent(subagent_type="Explore", model="haiku", prompt="<a pergunta sobre {named}>") e fique só '
        "com a síntese; varredura determinística de ≥3 arquivos: touring run --lang python --file <script> "
        "(só o agregado entra no contexto).\n"
        f"Matriz: {MATRIX_REF} §1 (busca/extração → haiku)."
    )


def read_burst(payload: Mapping[str, Any], home: Path, session: str, now: float) -> dict[str, Any] | None:
    """Count this call in the window; return the advice once the window is hot and the cooldown is over."""
    tool_name = str(payload.get("tool_name"))
    kb, target = read_cost(tool_name, payload.get("tool_input"), payload.get("cwd"))

    def mutate(state: dict[str, Any]) -> tuple[int, float, int, list[str]] | None:
        calls = [c for c in state["calls"] if now - c[0] < WINDOW_S] + [[now, kb, target]]
        state["calls"] = calls
        total = sum(c[1] for c in calls)
        if not (len(calls) >= MAX_CALLS or total >= MAX_KB) or now - state["last_nudge"] < COOLDOWN_S:
            return None
        state["last_nudge"] = now
        return len(calls), total, max(1, round((now - calls[0][0]) / 60)), recent_targets(calls)

    hot = with_state(home, session, mutate)
    if hot is None:
        return None
    n, total, minutes, targets = hot
    log(home, {"session": session, "evento": "leitura", "n": n, "kb": round(total, 1), "alvos": targets})
    return additional_context("PreToolUse", read_advice(n, total, minutes, targets))


def tail_lines(path: str) -> list[bytes]:
    """The last TAIL_LINES lines of a file, reading at most its last TAIL_BYTES."""
    with open(path, "rb") as fh:
        size = fh.seek(0, os.SEEK_END)
        fh.seek(max(0, size - TAIL_BYTES))
        lines = fh.read().splitlines()
    return (lines[1:] if size > TAIL_BYTES else lines)[-TAIL_LINES:]


def _epoch(stamp: object) -> float | None:
    try:
        return datetime.fromisoformat(stamp).astimezone(timezone.utc).timestamp() if isinstance(stamp, str) else None
    except ValueError:
        return None


def _main_assistant_message(raw: bytes) -> tuple[dict[str, Any], dict[str, Any]] | None:
    """(line, message) when `raw` is a main-session assistant turn; cheap checks first, sidechains refused."""
    if b'"model"' not in raw or len(raw) > MAX_LINE_BYTES:
        return None
    try:
        obj = json.loads(raw)
    except ValueError:
        return None
    message = obj.get("message") if isinstance(obj, dict) else None
    if not isinstance(message, dict) or obj.get("isSidechain") or message.get("role") != "assistant":
        return None
    return obj, message


def _assistant_turn(raw: bytes) -> tuple[str, str, float | None] | None:
    """(family, key, epoch) when `raw` is a main-session assistant turn of an Anthropic family, else None."""
    parsed = _main_assistant_message(raw)
    if parsed is None:
        return None
    obj, message = parsed
    family = model_family(message.get("model"))
    if family is None:
        return None
    return family, str(obj.get("uuid") or message.get("id") or obj.get("timestamp")), _epoch(obj.get("timestamp"))


def assistant_turns(lines: list[bytes]) -> list[tuple[str, str, float | None]]:
    """The main-session assistant turns of the tail, oldest first; sidechains and synthetics are skipped."""
    return [turn for raw in lines if (turn := _assistant_turn(raw)) is not None]


def recent_switches(turns: list[tuple[str, str, float | None]], now: float) -> list[tuple[str, str, str, float | None]]:
    """(from, to, key, epoch) of each family change, oldest first; one older than SWITCH_RECENCY_S is not news."""
    return [(a[0], b[0], b[1], b[2]) for a, b in pairwise(turns)
            if a[0] != b[0] and (b[2] is None or now - b[2] <= SWITCH_RECENCY_S)]


def switch_advice(origin: str, target: str, age_min: int | None) -> str:
    when = f" (há {age_min} min)" if age_min is not None else ""
    return (
        f"[delegation-nudge] Troca de modelo no meio da sessão: {origin} → {target}{when}. Cada modelo tem o "
        "próprio cache — a troca relê o contexto inteiro sem cache. Suba o effort antes de trocar de modelo; "
        "se precisa de outro modelo, delegue a um subagente com `model:` "
        f"(regra 17 do CLAUDE.md; {MATRIX_REF} §2)."
    )


def tail_switches(path: object, now: float) -> list[tuple[str, str, str, float | None]]:
    """The recent family changes in the tail of the transcript at `path`; unreadable or absent = none."""
    if not isinstance(path, str) or not path:
        return []
    try:
        return recent_switches(assistant_turns(tail_lines(path)), now)
    except OSError:
        return []


def model_switch(payload: Mapping[str, Any], home: Path, session: str, now: float) -> dict[str, Any] | None:
    """Announce the newest recent switch seen in the transcript tail, once."""
    switches = tail_switches(payload.get("transcript_path"), now)
    if not switches:
        return None

    def mutate(state: dict[str, Any]) -> tuple[str, str, float | None] | None:
        fresh = [s for s in switches if s[2] not in state["switches"]]
        if not fresh:
            return None
        state["switches"] = (state["switches"] + [s[2] for s in fresh])[-KEEP_SWITCHES:]
        return fresh[-1][0], fresh[-1][1], fresh[-1][3]

    news = with_state(home, session, mutate)
    if news is None:
        return None
    origin, target, epoch = news
    log(home, {"session": session, "evento": "troca-de-modelo", "n": 1, "kb": 0.0, "alvos": [f"{origin}→{target}"]})
    age = None if epoch is None else round((now - epoch) / 60)
    return additional_context("UserPromptSubmit", switch_advice(origin, target, age))


def additional_context(event: str, text: str) -> dict[str, Any]:
    return {"hookSpecificOutput": {"hookEventName": event, "additionalContext": text}}


def log(home: Path | None, record: dict[str, Any]) -> None:
    """Append one line per advice. Never raises: the log is best-effort by contract."""
    if home is None:
        return
    path = home / ".claude" / "touring" / "delegation_nudge.jsonl"
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        stamped = {"ts": datetime.now().astimezone().isoformat(timespec="seconds"), **record}
        with path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(stamped, ensure_ascii=True) + "\n")
    except OSError:
        pass


def handle(payload: object, env: Mapping[str, str], home: Path, now: float) -> dict[str, Any] | None:
    """The advice for this hook call, or None. Pure but for the state/log files under `home`."""
    if not isinstance(payload, dict):
        return None
    session = safe_id(payload.get("session_id"))
    if session is None or skip_reason(payload, env):
        return None
    event = payload.get("hook_event_name")
    if event == "PreToolUse" and payload.get("tool_name") in READ_TOOLS:
        return read_burst(payload, home, session, now)
    if event == "UserPromptSubmit":
        return model_switch(payload, home, session, now)
    return None


def main() -> int:
    home: Path | None = None
    session: object = None
    try:
        home = Path.home()
        payload = json.loads(sys.stdin.buffer.read().decode("utf-8", errors="replace"))
        session = payload.get("session_id") if isinstance(payload, dict) else None
        output = handle(payload, os.environ, home, time.time())
        if output is not None:
            sys.stdout.write(json.dumps(output, ensure_ascii=True))
            sys.stdout.flush()
    except Exception as exc:  # noqa: BLE001 — an advisory hook must never fail the tool call; the log keeps it loud
        log(home, {"session": session if isinstance(session, str) else None, "evento": "erro", "n": 0, "kb": 0.0,
                   "alvos": [type(exc).__name__]})
    return 0


if __name__ == "__main__":
    sys.exit(main())
