#!/usr/bin/env python3
"""model_pool_roster.py — SessionStart: announce the model pool and the live Kimi/MiniMax sessions of the project.

WHAT IT DOES
============
On an Anthropic session it injects, once per SessionStart, the roster of `model_pool.POOL` (Fable orchestrator,
1 Opus, 3 Sonnet, 5 Haiku, each with its default effort) and the sister sessions (Kimi/MiniMax backends) alive in
this project, with model and status. On `source == "startup"` it also orders the open subagent seats (nine under a Fable session; eight otherwise —
`opus-1` is closed unless the session is PROVEN Fable, the gate's own rule, resolved with the gate's own
`main_session_model`) to be invoked in the first turn — a SessionStart hook cannot call a tool itself (it only adds context:
code.claude.com/docs/en/hooks, "SessionStart decision control"), so the invocation is the orchestrator's first
action, not the hook's.

WHO GETS NOTHING
================
A sister session (the matrix is for Anthropic sessions only; inside Kimi/MiniMax every subagent stays on the
session's own model), a subagent (`agent_id` in the input), any session with the human kill switch
`MODEL_POOL_ROSTER_DISABLED=1`, and any session whose injection policy (`TOURING_HOOK_INJECTION`,
client/hooks/injection_policy.py) cuts the `plugin-hook` family. `MODEL_POOL_PREWARM=0` (human) keeps the roster
and drops the startup order: the seats are then filled on demand.

ABSENCE IS NOT ZERO
===================
A missing session registry is reported as "indisponível", never as "nenhuma irmã". Fail-open: any error here
leaves the session untouched (exit 0, nothing on stdout) and says so on stderr.

Registration: `client/claude/hooks.settings.json` (SessionStart, no matcher) → `scripts/install_claude_hooks.py
--apply`. Tests: `client/hooks/tests/test_model_pool_roster.py`.
"""

from __future__ import annotations

import json
import os
import sys
from collections.abc import Mapping
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
# The gate's own resolver (D8): the same session model, so the same verdict on the seat it orders.
from agent_model_gate import main_session_model
from backend_provider import SISTER_PROVIDERS, model_family, provider_of
from model_pool import ORCHESTRATOR, POOL, PoolSlot, slot_names, subagent_slots

DOC_REF = "~/projects/touring/docs/economia-de-creditos-e-contexto.md §1"
#: The env keys read from a sibling's /proc/<pid>/environ — a whitelist, so no secret leaves the process.
ENV_KEYS = ("ANTHROPIC_MODEL", "ANTHROPIC_BASE_URL")
READY_PROMPT = "Slot <nome> do pool de modelos: responda PRONTO em uma linha e aguarde a tarefa por SendMessage."
INJECTION_FAMILY = "plugin-hook"


def _starttime(base: Path) -> str | None:
    """Field 22 of /proc/<pid>/stat (process start, clock ticks since boot); None when unreadable."""
    try:
        return (base / "stat").read_text().rsplit(")", 1)[1].split()[19]
    except (OSError, IndexError):
        return None


def _resolved(path: object) -> str:
    """`path` with symlinks resolved; the raw text when it cannot be (a registry cwd may be gone)."""
    try:
        return str(Path(str(path)).resolve())
    except (OSError, RuntimeError, ValueError):
        return str(path)


def _sibling_env(proc_root: Path, pid: int, proc_start: object = None) -> dict[str, str] | None:
    """Whitelisted env of a live `claude` process; None if it is dead, is not claude, or is a recycled PID.

    `proc_start` is the registry's `procStart` — the process starttime in ticks (field 22 of /proc/<pid>/stat,
    checked against live sessions). A PID the kernel reused has a different starttime, so a mismatch means the
    registered session is gone. Without `procStart` or a readable stat there is no identity to compare: the
    process is taken as before.
    """
    base = proc_root / str(pid)
    try:
        if (base / "comm").read_text().strip() != "claude":
            return None
        if proc_start is not None and (live := _starttime(base)) is not None and live != str(proc_start):
            return None
        raw = (base / "environ").read_bytes().decode(errors="replace")
    except OSError:
        return None
    pairs = (kv.split("=", 1) for kv in raw.split("\0") if "=" in kv)
    return {key: value for key, value in pairs if key in ENV_KEYS}


def live_sisters(cwd: str, sessions_dir: Path, proc_root: Path) -> tuple[list[dict[str, str]], int]:
    """(Kimi/MiniMax sessions alive in `cwd`, dead registry entries). Same join as ~/.claude/tools/sessoes_irmas.py."""
    rows: list[dict[str, str]] = []
    stale = 0
    wanted = _resolved(cwd)
    for path in sorted(sessions_dir.glob("*.json")):
        try:
            meta = json.loads(path.read_text(encoding="utf-8"))
            pid = int(meta["pid"])
        except (OSError, ValueError, KeyError, TypeError):
            stale += 1
            continue
        if _resolved(meta.get("cwd")) != wanted:
            continue
        env = _sibling_env(proc_root, pid, meta.get("procStart"))
        if env is None:
            stale += 1
            continue
        provider = provider_of(env.get("ANTHROPIC_BASE_URL"))
        if provider in SISTER_PROVIDERS:
            rows.append({"name": str(meta.get("name") or f"pid {pid}"), "provider": provider,
                         "model": env.get("ANTHROPIC_MODEL") or "modelo default", "status": str(meta.get("status") or "-")})
    return rows, stale


def sisters_line(cwd: str, sessions_dir: Path, proc_root: Path) -> str:
    """One line naming each live sister with provider, model and status; absence of the registry is declared."""
    head = "Sessões irmãs (Kimi/MiniMax) neste projeto"
    if not sessions_dir.is_dir():
        return f"{head}: indisponível — registro {sessions_dir} ausente (não é zero irmãs)."
    rows, stale = live_sisters(cwd, sessions_dir, proc_root)
    dead = f"{stale} registro(s) morto(s) ignorado(s)"
    if not rows:
        return f"{head}: nenhuma ativa ({dead}). Lançar: python3 ~/.claude/tools/sessoes_irmas.py --lancar kimi1m|minimax."
    named = " · ".join(f"{r['name']} = {r['provider']} {r['model']} [{r['status']}]" for r in rows)
    return (f"{head}: {named} ({dead}). Corpus longo, lote extenso ou 2º leitor → brief único por SendMessage "
            "(protocolo: §4 do mesmo documento).")


def _seat(slot: PoolSlot) -> str:
    return f"{slot.family}-1" if slot.count == 1 else f"{slot.family}-1..{slot.count}"


def _boss() -> PoolSlot:
    return next(slot for slot in POOL if slot.role == ORCHESTRATOR)


def opus_seat_open(session_model: object) -> bool:
    """True only under a PROVEN Fable orchestrator — the gate's rule (`fit:opus-fica-na-sessao-principal`)."""
    return model_family(session_model) == _boss().family


def ordered_seats(session_model: object) -> list[str]:
    """The seats the startup order names: every pool seat the gate would let through in this session."""
    return [name for name in slot_names() if name != "opus-1" or opus_seat_open(session_model)]


def roster_lines(session_model: object = None) -> list[str]:
    """The roster: orchestrator first, then one line per subagent family with the exact `Agent` arguments."""
    boss = _boss()
    lines = [
        f"[POOL DE MODELOS · sessão Anthropic · fonte única client/hooks/model_pool.py · matriz: {DOC_REF}]",
        f"Orquestrador: {boss.family}, effort {boss.effort} (esta sessão) — {boss.use}.",
    ]
    family = model_family(session_model)
    if family and family != boss.family:
        lines.append(f"Esta sessão roda {session_model}, não {boss.family}: o pool de subagentes vale, menos o assento opus-1.")
    elif not family:
        lines.append(f"O modelo desta sessão não foi identificado (nunca presumido {boss.family}): o assento opus-1 fica fechado.")
    lines.append("Subagentes previstos (alias = a versão mais nova da família na API da Anthropic):")
    for slot in subagent_slots():
        if slot.family == "opus" and not opus_seat_open(session_model):
            lines.append(f'  {_seat(slot):<12} FECHADO nesta sessão (abre só sob orquestrador {boss.family}; o gate recusa '
                         f'model="{slot.family}" em tipo de implementação aqui) — {slot.use}')
        else:
            lines.append(f'  {_seat(slot):<12} Agent(model="{slot.family}", effort="{slot.effort}") — {slot.use}')
    lines.append("Toda chamada Agent nomeia o `model` e o `effort` da linha que serve à subtarefa; "
                 "o agent_model_gate recusa `model` ausente ou inadequado ao tipo.")
    return lines


def spawn_lines(source: str, prewarm: bool, session_model: object = None) -> list[str]:
    """The startup order for the open seats, or the on-demand note when it does not apply."""
    if source == "startup" and prewarm:
        names = ordered_seats(session_model)
        seats = " · ".join(names)
        return [
            (f"MUST (1º turno, uma única mensagem, junto do trabalho pedido): invoque os {len(names)} assentos — "
             f'{seats} — cada um com `name` = o assento, `subagent_type="general-purpose"`, o `model` e o `effort` '
             f'da linha acima e o prompt "{READY_PROMPT}"'),
            ("Depois entregue cada subtarefa ao assento da classe por SendMessage(to=<assento>) em vez de criar "
             "agente novo. Desligar o pré-aquecimento: MODEL_POOL_PREWARM=0 (decisão humana)."),
        ]
    return [("Assentos já invocados nesta conversa seguem endereçáveis por SendMessage(to=<assento>); os demais "
             "nascem sob demanda, com `name` = o assento.")]


def build_context(payload: Mapping[str, Any], env: Mapping[str, str], home: Path, proc_root: Path) -> str | None:
    """The text to inject, or None when this session is out of scope."""
    if env.get("MODEL_POOL_ROSTER_DISABLED") == "1" or payload.get("agent_id"):
        return None
    if provider_of(env.get("ANTHROPIC_BASE_URL")) in SISTER_PROVIDERS:
        return None
    cwd = str(Path(env.get("CLAUDE_PROJECT_DIR") or payload.get("cwd") or os.getcwd()).resolve())
    session = payload.get("model") or main_session_model(payload.get("transcript_path"), env, home)
    lines = [*roster_lines(session),
             *spawn_lines(str(payload.get("source") or ""), env.get("MODEL_POOL_PREWARM", "1") != "0", session),
             sisters_line(cwd, home / ".claude" / "sessions", proc_root)]
    return "\n".join(lines)


def main() -> int:
    try:
        raw = sys.stdin.buffer.read().decode("utf-8", errors="replace")
        payload = json.loads(raw) if raw.strip() else {}
        context = build_context(payload if isinstance(payload, dict) else {}, os.environ, Path.home(), Path("/proc"))
        if context:
            # Context-injection switch (26/10/2026) — client/hooks/injection_policy.py, resolved beside this file.
            import injection_policy
            if not injection_policy.allows(INJECTION_FAMILY):
                return 0
            sys.stdout.write(json.dumps({"hookSpecificOutput": {"hookEventName": "SessionStart",
                                                                "additionalContext": context}}, ensure_ascii=False))
            sys.stdout.flush()
    except Exception as exc:  # noqa: BLE001 — fail-open by contract: a roster bug must never cost a session
        sys.stderr.write(f"model_pool_roster: erro interno, sessão segue sem o pool ({type(exc).__name__}: {exc})\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
