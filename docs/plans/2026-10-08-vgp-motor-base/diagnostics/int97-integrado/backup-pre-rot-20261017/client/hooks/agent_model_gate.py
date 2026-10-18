#!/usr/bin/env python3
"""agent_model_gate.py — PreToolUse(Agent): subagente sem `model` numa sessão Anthropic é recusado.

POR QUE ISTO EXISTE
===================
A regra "cada parte da tarefa desce ao executor mais barato que ainda passa no juiz" estava
escrita no CLAUDE.md dos dois projetos (08/10/2026, diretriz de Gabriel) e a medição de 14 dias
de transcripts mostrou o que a prosa produz: em sessões Opus, **102 de 171 chamadas `Agent`
(60%) saíram sem `model`** e herdaram o Opus — o gargalo não era o custo de decidir, era a
decisão não tomada. Enforcement mora no executor, não no anúncio (D8): este hook transforma
"escolha o modelo" em afordância — a chamada sem decisão volta com a matriz, e reemiti-la com
`model` custa uma linha.

O QUE PASSA SEM `model` (a decisão já foi tomada em outro lugar)
=================================================================
`EXEMPTIONS`, na ordem em que o predicado as testa — o corpo canônico
(`docs/economia-de-creditos-e-contexto.md` §1) as lista na MESMA ordem, e um teste exige:
kill switch humano · sessão irmã Kimi/MiniMax (pelo `ANTHROPIC_BASE_URL` que o hook herda;
gateway desconhecido NÃO isenta) · fork · `model:` declarado na definição do agente, inclusive
`inherit` escrito (o autor decidiu herdar) · built-in com default próprio ·
`CLAUDE_CODE_SUBAGENT_MODEL`. `model: "opus"` explícito também passa.

Onde a definição é procurada (code.claude.com/docs/en/sub-agents, lida em 08/10/2026):
projeto — subindo do cwd até a raiz do repositório, a mais próxima vence — > usuário
(`~/.claude/agents`) > plugin (`installPath` do `installed_plugins.json` e `plugins/synced`;
subpasta de `agents/` entra no identificador: `plug:review:security`). Definições de managed
settings, `--agents` e `--add-dir` não são visíveis daqui: caem no deny, cujo remédio funciona.

FAIL-OPEN, NUNCA MUDO
=====================
Erro do próprio gate deixa a chamada passar (um bug aqui não pode travar o trabalho), mas
cada chamada `Agent` — permitida, recusada ou com erro — vira uma linha em
`~/.claude/touring/agent_model_gate.jsonl` com o motivo, sem prompt nem description. Entrada e
saída são bytes/ASCII: nenhuma configuração de encoding derruba o veredito.

Registro: `client/claude/hooks.settings.json` (PreToolUse, matcher `Agent`) →
`scripts/install_claude_hooks.py --apply`. Testes: `client/hooks/tests/test_agent_model_gate.py`.
Kill switch (decisão humana): `AGENT_MODEL_GATE_DISABLED=1` no env da sessão.
"""

from __future__ import annotations

import json
import os
import re
import sys
from collections.abc import Iterator, Mapping, Sequence
from datetime import datetime
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from backend_provider import SISTER_PROVIDERS, provider_of  # noqa: E402 — one predicate for every hook (D8)

#: The Agent tool's `model` enum, each with the work it is the cheapest fit for.
MATRIX = (
    ("haiku", "buscar, extrair, classificar, recuperar documentação, varredura mecânica"),
    ("sonnet", "implementar com spec clara, testes, revisão, pesquisa web com julgamento"),
    ("opus", 'decidir, sintetizar, julgar a entrega de um delegado, arquitetura (herdar de propósito = `model: "opus"`)'),
    ("fable", "só ambiguidade longa em que o Opus em effort alto já falhou; em headless, só com o sim do Gabriel"),
)
#: (keyword the canonical doc's bullet must contain, phrase shown to the model), in predicate order.
EXEMPTIONS = (
    ("AGENT_MODEL_GATE_DISABLED", "AGENT_MODEL_GATE_DISABLED=1 (só humano)"),
    ("sessão irmã", "sessão irmã Kimi/MiniMax"),
    ("fork", "fork"),
    ("frontmatter", "agente com `model:` no frontmatter (inclusive `inherit` escrito)"),
    ("claude-code-guide", "built-in com default próprio (claude-code-guide, statusline-setup)"),
    ("CLAUDE_CODE_SUBAGENT_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL definido"),
)
#: Built-ins whose default is not the session model (code.claude.com/docs/en/sub-agents, 08/10/2026).
#: general-purpose, Explore and Plan inherit it, so they are judged like any undecided call.
BUILTIN_DEFAULTS = {"claude-code-guide": "haiku", "statusline-setup": "sonnet"}
FRONTMATTER = re.compile(r"\A﻿?---\r?\n(.*?)\r?\n---", re.S)
INLINE_COMMENT = re.compile(r"\s+#.*$")
SESSION_MODEL = re.compile(rb'"model":"(claude-[^"]+)"')
TRANSCRIPT_TAIL_BYTES = 262_144


def frontmatter(path: Path) -> dict[str, str]:
    """Top-level `key: value` pairs of a Markdown frontmatter (BOM, CRLF and `# comments` tolerated)."""
    try:
        match = FRONTMATTER.match(path.read_text(encoding="utf-8", errors="replace"))
    except OSError:
        return {}
    fields: dict[str, str] = {}
    for line in (match.group(1) if match else "").splitlines():
        key, sep, value = line.partition(":")
        if sep and not line.startswith((" ", "\t", "#")):
            fields[key.strip()] = INLINE_COMMENT.sub("", value).strip().strip("\"'")
    return fields


def project_agent_dirs(cwd: Path | None, project_dir: Path | None, home: Path) -> list[Path]:
    """Every `.claude/agents` from cwd up to the repository root, closest first (docs: closest wins)."""
    dirs: list[Path] = []
    start = cwd or project_dir
    for directory in (start, *start.parents) if start else ():
        if directory == home:  # ~/.claude/agents is the user scope, never a project one
            break
        dirs.append(directory / ".claude" / "agents")
        if (directory / ".git").exists():
            break
    if project_dir is not None and project_dir / ".claude" / "agents" not in dirs:
        dirs.append(project_dir / ".claude" / "agents")
    return dirs


def plugin_agent_dirs(plugin: str, home: Path) -> list[Path]:
    """`agents/` of the INSTALLED copy (installed_plugins.json installPath) and of synced plugins.

    Never the cache by mtime: it keeps stale versions side by side with the installed one.
    """
    root = home / ".claude" / "plugins"
    try:
        installed = json.loads((root / "installed_plugins.json").read_text(encoding="utf-8")).get("plugins", {})
    except (OSError, ValueError, AttributeError):
        installed = {}
    dirs = [Path(entry["installPath"]) / "agents"
            for key, entries in installed.items() if key.split("@", 1)[0] == plugin
            for entry in (entries if isinstance(entries, list) else [entries])
            if isinstance(entry, dict) and entry.get("installPath")]
    return dirs + sorted(root.glob(f"synced/*/{plugin}/agents"))


def _definition_dirs(subagent_type: str, project_dirs: Sequence[Path], home: Path) -> Iterator[tuple[Path, str]]:
    """(directory, name to match) in Claude Code's precedence: project > user > plugin."""
    plugin, *subpath, name = subagent_type.split(":") if ":" in subagent_type else ("", subagent_type)
    if plugin:
        for agents in plugin_agent_dirs(plugin, home):
            yield agents.joinpath(*subpath), name
        return
    for directory in project_dirs:
        yield directory, name
    yield home / ".claude" / "agents", name


def definition_model(subagent_type: str, project_dirs: Sequence[Path], home: Path) -> str | None:
    """`model:` of the winning definition: None = no definition found, "" = found without one."""
    is_plugin = ":" in subagent_type
    for directory, name in _definition_dirs(subagent_type, project_dirs, home):
        if not directory.is_dir():
            continue
        for path in sorted(directory.rglob("*.md")):
            fields = frontmatter(path)
            # A plugin file registers by its path (agents/review/security.md → plug:review:security).
            if fields.get("name") == name or (is_plugin and path.stem == name):
                return fields.get("model", "")
    return None


def _session_decision(env: Mapping[str, str]) -> str | None:
    """The whole session is out of scope: the human switch, or a named sister backend."""
    if env.get("AGENT_MODEL_GATE_DISABLED") == "1":
        return "disabled"
    provider = provider_of(env.get("ANTHROPIC_BASE_URL"))
    return f"irmã:{provider}" if provider in SISTER_PROVIDERS else None


def _definition_decision(subagent_type: str, project_dirs: Sequence[Path], home: Path) -> str | None:
    """The agent itself fixes the model: a fork, a declared `model:` (even `inherit`), a built-in default."""
    if subagent_type == "fork":
        return "fork"
    pinned = definition_model(subagent_type, project_dirs, home)
    if pinned:
        return f"frontmatter:{pinned.lower() if pinned.lower() == 'inherit' else pinned}"
    if pinned is None and subagent_type in BUILTIN_DEFAULTS:
        return f"builtin:{BUILTIN_DEFAULTS[subagent_type]}"
    return None


def decide(tool_input: Mapping[str, Any], env: Mapping[str, str], project_dirs: Sequence[Path],
           home: Path) -> tuple[bool, str]:
    """(allowed, why). Pure: every input is an argument; `why` names the decision that was found.

    Stages run lazily in the order Claude Code resolves a subagent's model (call > definition >
    CLAUDE_CODE_SUBAGENT_MODEL), after the session-wide exemptions.
    """
    model = tool_input.get("model")
    explicit = f"explicit:{model.strip()}" if isinstance(model, str) and model.strip() else None
    env_model = env.get("CLAUDE_CODE_SUBAGENT_MODEL", "").strip()
    subagent_type = str(tool_input.get("subagent_type") or "general-purpose")
    why = (_session_decision(env) or explicit or _definition_decision(subagent_type, project_dirs, home)
           or (f"env:{env_model}" if env_model else None))
    return (True, why) if why else (False, "deny")


def session_model(transcript_path: str | None) -> str | None:
    """Model of the last assistant turn, from the transcript TAIL only (the hook input carries none)."""
    if not transcript_path:
        return None
    try:
        with open(transcript_path, "rb") as fh:
            fh.seek(0, os.SEEK_END)
            fh.seek(max(0, fh.tell() - TRANSCRIPT_TAIL_BYTES))
            found = SESSION_MODEL.findall(fh.read())
    except OSError:
        return None
    return found[-1].decode("ascii", errors="replace") if found else None


def deny_reason(tool_input: Mapping[str, Any], model: str | None, quota: str | None = None) -> str:
    """The matrix plus the real call — dense, specific, and the way back named.

    `quota` is the recorded franchise line (rate_limits_snapshot.quota_line); absent, nothing is claimed.
    """
    subagent_type = tool_input.get("subagent_type") or "general-purpose"
    description = str(tool_input.get("description") or "")[:80]
    call = f'subagent_type={subagent_type}, "{description}"' if description else f"subagent_type={subagent_type}"
    lines = [
        f"[agent-model-gate] Agent sem `model` ({call}) herdaria {model or 'o modelo desta sessão'}: "
        "a decisão de modelo não foi tomada. Reemita a MESMA chamada com `model`:",
        *(f"  {alias:<6} — {use}" for alias, use in MATRIX),
        *([f"{quota}."] if quota else []),
        "Passam sem `model`: " + " · ".join(phrase for _, phrase in EXEMPTIONS) + ".",
        "Matriz e fatos datados: ~/projects/touring/docs/economia-de-creditos-e-contexto.md §1.",
    ]
    return "\n".join(lines)


def recorded_quota(home: Path) -> str | None:
    """The Anthropic franchise line the status line recorded; None (never a guess) when unavailable."""
    try:
        from rate_limits_snapshot import quota_line  # noqa: PLC0415 — only a deny pays for this import
        return quota_line(home)
    except Exception:  # noqa: BLE001 — the quota is context for the deny, never a reason to lose it
        return None


def log(home: Path | None, record: dict[str, Any]) -> None:
    """Append one verdict. Never raises: a log failure goes to stderr and never changes the verdict."""
    if home is None:
        return
    path = home / ".claude" / "touring" / "agent_model_gate.jsonl"
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(record, ensure_ascii=True) + "\n")
    except Exception as exc:  # noqa: BLE001 — the log is best-effort by contract
        sys.stderr.write(f"agent_model_gate: log indisponível ({type(exc).__name__})\n")


def main() -> int:
    home: Path | None = None
    record: dict[str, Any] = {"ts": datetime.now().astimezone().isoformat(timespec="seconds")}
    try:
        home = Path.home()
        payload = json.loads(sys.stdin.buffer.read().decode("utf-8", errors="replace"))
        if not isinstance(payload, dict) or payload.get("tool_name") != "Agent":
            return 0
        record.update(session_id=payload.get("session_id"), agent_id=payload.get("agent_id"))
        tool_input = payload.get("tool_input")
        if not isinstance(tool_input, dict):
            log(home, {**record, "allowed": True, "why": "malformed"})
            return 0
        env_project = os.environ.get("CLAUDE_PROJECT_DIR")
        cwd = payload.get("cwd") or env_project
        dirs = project_agent_dirs(Path(cwd) if cwd else None, Path(env_project) if env_project else None, home)
        allowed, why = decide(tool_input, os.environ, dirs, home)
        record.update(allowed=allowed, why=why, subagent_type=tool_input.get("subagent_type"),
                      model=tool_input.get("model"))
        if not allowed:
            reason = deny_reason(tool_input, session_model(payload.get("transcript_path")), recorded_quota(home))
            sys.stdout.write(json.dumps({"hookSpecificOutput": {
                "hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": reason,
            }}, ensure_ascii=True))
            sys.stdout.flush()
    except Exception as exc:  # noqa: BLE001 — fail-open by contract; the log line keeps it loud
        sys.stderr.write(f"agent_model_gate: erro interno, chamada liberada ({type(exc).__name__}: {exc})\n")
        log(home, {**record, "allowed": True, "why": f"error:{type(exc).__name__}"})
        return 0
    log(home, record)
    return 0


if __name__ == "__main__":
    sys.exit(main())
