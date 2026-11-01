"""test_agent_model_gate.py — o gate que recusa `Agent` sem `model` numa sessão Anthropic.

Cada teste roda o hook REAL (subprocesso, payload JSON no stdin) com HOME, projeto e env
sintéticos: nada aqui lê ~/.claude — salvo o guarda D8 cruzado com `sessoes_irmas.py`, que
pula quando a ferramenta não existe (runner de CI). O env do teste remove as variáveis que
uma sessão irmã exporta, para a suíte dar o mesmo veredito rodando dentro de uma.

Todo teste de PERMISSÃO exige o motivo no log (`_assert_allowed`): `stdout` vazio também
acontece com o hook ausente ou quebrado — no RED, 10 deles passaram assim, no vazio.
"""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from types import ModuleType
from typing import Any

import pytest
from helpers import CLIENT_HOOKS, REPO_ROOT, HookResult, schema_violations

HOOK = CLIENT_HOOKS / "agent_model_gate.py"
CANONICAL_DOC = REPO_ROOT / "docs" / "economia-de-creditos-e-contexto.md"
SESSOES_IRMAS = Path.home() / ".claude" / "tools" / "sessoes_irmas.py"
SESSION_ENV = ("ANTHROPIC_BASE_URL", "ANTHROPIC_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL", "AGENT_MODEL_GATE_DISABLED",
               "PYTHONIOENCODING")
CALL = {"description": "Achar callers de foo", "prompt": "SEGREDO-DO-PROMPT", "subagent_type": "general-purpose"}
Run = tuple[subprocess.CompletedProcess[str], dict[str, Any]]


def _load(path: Path, name: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None, path
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _agent_md(directory: Path, name: str, model: str | None, *, prefix: str = "") -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    model_line = f"model: {model}\n" if model is not None else ""
    path = directory / f"{name}.md"
    path.write_text(f"{prefix}---\nname: {name}\ndescription: x\n{model_line}---\ncorpo\n", encoding="utf-8")
    return path


def _install_plugin(home: Path, plugin: str, install_path: Path) -> None:
    registry = home / ".claude" / "plugins" / "installed_plugins.json"
    registry.parent.mkdir(parents=True, exist_ok=True)
    registry.write_text(json.dumps({"version": 2, "plugins": {
        f"{plugin}@mkt": [{"scope": "user", "installPath": str(install_path), "version": install_path.name}],
    }}))


def _run(tmp_path: Path, tool_input: Any, *, tool_name: str = "Agent", env_extra: dict[str, str] | None = None,
         transcript: Path | None = None, raw: str | bytes | None = None, cwd: Path | None = None) -> Run:
    home, project = tmp_path / "home", tmp_path / "proj"
    home.mkdir(exist_ok=True)
    project.mkdir(exist_ok=True)
    env = {k: v for k, v in os.environ.items() if k not in SESSION_ENV}
    env.update(HOME=str(home), CLAUDE_PROJECT_DIR=str(project), **(env_extra or {}))
    payload = raw if raw is not None else json.dumps({
        "session_id": "hooks-contract-agent-model-gate",
        "transcript_path": str(transcript or tmp_path / "ausente.jsonl"),
        "cwd": str(cwd or project),
        "hook_event_name": "PreToolUse",
        "tool_name": tool_name,
        "tool_input": tool_input,
    })
    data = payload.encode("utf-8", errors="surrogatepass") if isinstance(payload, str) else payload
    proc = subprocess.run([sys.executable, str(HOOK)], input=data, capture_output=True, env=env, timeout=15, check=False)
    text = subprocess.CompletedProcess(proc.args, proc.returncode, proc.stdout.decode("utf-8", "replace"),
                                       proc.stderr.decode("utf-8", "replace"))
    return text, (json.loads(text.stdout) if text.stdout.strip() else {})


def _denied(data: dict[str, Any]) -> bool:
    return data.get("hookSpecificOutput", {}).get("permissionDecision") == "deny"


def _reason(data: dict[str, Any]) -> str:
    return data["hookSpecificOutput"]["permissionDecisionReason"]


def _log(tmp_path: Path) -> list[dict[str, Any]]:
    path = tmp_path / "home" / ".claude" / "touring" / "agent_model_gate.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def _assert_allowed(tmp_path: Path, run: Run, why: str) -> None:
    """Positive predicate: the hook RAN, stayed silent AND logged why it let the call through."""
    proc, data = run
    assert proc.returncode == 0, proc.stderr
    assert data == {}
    last = _log(tmp_path)[-1]
    assert last["allowed"] is True
    assert last["why"] == why, last


def _assert_denied(tmp_path: Path, run: Run) -> str:
    proc, data = run
    assert proc.returncode == 0, proc.stderr
    assert _denied(data), (data, proc.stderr)
    assert _log(tmp_path)[-1]["why"] == "deny"
    return _reason(data)


# --- the verdict ---------------------------------------------------------------------------------

def test_general_purpose_without_model_is_denied_with_the_matrix_and_the_real_call(tmp_path: Path) -> None:
    proc, _ = run = _run(tmp_path, CALL)
    reason = _assert_denied(tmp_path, run)
    as_bytes = subprocess.CompletedProcess(proc.args, proc.returncode, proc.stdout.encode(), proc.stderr.encode())
    assert schema_violations("PreToolUse", HookResult(HOOK, CALL, as_bytes)) == []
    for needle in ("general-purpose", "Achar callers de foo", "`model`", "haiku", "sonnet", "opus", "fable"):
        assert needle in reason, needle
    assert "SEGREDO-DO-PROMPT" not in reason


def test_omitted_subagent_type_is_judged_as_general_purpose(tmp_path: Path) -> None:
    assert "general-purpose" in _assert_denied(tmp_path, _run(tmp_path, {"description": "d", "prompt": "p"}))


@pytest.mark.parametrize("alias", ["haiku", "sonnet", "opus", "fable"])
def test_explicit_model_passes_silently(tmp_path: Path, alias: str) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "model": alias}), f"explicit:{alias}")


def test_fork_passes_without_model(tmp_path: Path) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "fork"}), "fork")


# --- session-wide exemptions ---------------------------------------------------------------------

@pytest.mark.parametrize(("base_url", "provider"), [
    ("https://api.kimi.com/coding/", "kimi"),
    ("https://api.moonshot.ai/anthropic", "kimi"),
    ("https://api.minimax.io/anthropic", "minimax"),
])
def test_sister_sessions_are_exempt(tmp_path: Path, base_url: str, provider: str) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, CALL, env_extra={"ANTHROPIC_BASE_URL": base_url}), f"irmã:{provider}")


@pytest.mark.parametrize("base_url", ["https://api.anthropic.com", "https://litellm.example.org/v1"])
def test_anthropic_or_unknown_gateway_is_still_gated(tmp_path: Path, base_url: str) -> None:
    """Only the NAMED sister backends are exempt: an unknown gateway may front Opus."""
    _assert_denied(tmp_path, _run(tmp_path, CALL, env_extra={"ANTHROPIC_BASE_URL": base_url}))


def test_global_subagent_model_env_is_a_decision(tmp_path: Path) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, CALL, env_extra={"CLAUDE_CODE_SUBAGENT_MODEL": "haiku"}), "env:haiku")


def test_human_kill_switch(tmp_path: Path) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, CALL, env_extra={"AGENT_MODEL_GATE_DISABLED": "1"}), "disabled")


# --- where the definition lives ------------------------------------------------------------------

@pytest.mark.parametrize(("model", "why"), [
    ("sonnet", "frontmatter:sonnet"), ("claude-haiku-5-5", "frontmatter:claude-haiku-5-5"),
    ("inherit", "frontmatter:inherit"), ("Inherit  # o autor decidiu", "frontmatter:inherit"),
])
def test_declared_model_is_a_decision_even_inherit(tmp_path: Path, model: str, why: str) -> None:
    _agent_md(tmp_path / "proj" / ".claude" / "agents" / "sub", "revisor", model)
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}), why)


def test_definition_without_model_is_not_a_decision(tmp_path: Path) -> None:
    _agent_md(tmp_path / "proj" / ".claude" / "agents", "revisor", None)
    _assert_denied(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}))


def test_bom_prefixed_definition_is_read(tmp_path: Path) -> None:
    _agent_md(tmp_path / "proj" / ".claude" / "agents", "revisor", "sonnet", prefix="﻿")
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}), "frontmatter:sonnet")


def test_user_definition_decides_and_project_definition_wins(tmp_path: Path) -> None:
    _agent_md(tmp_path / "home" / ".claude" / "agents", "revisor", "haiku")
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}), "frontmatter:haiku")
    _agent_md(tmp_path / "proj" / ".claude" / "agents", "revisor", None)
    _assert_denied(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}))


def test_project_definitions_are_found_walking_up_and_the_closest_wins(tmp_path: Path) -> None:
    sub = tmp_path / "proj" / "pacote" / "modulo"
    sub.mkdir(parents=True)
    (tmp_path / "proj" / ".git").mkdir()
    _agent_md(tmp_path / "proj" / ".claude" / "agents", "revisor", "sonnet")
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}, cwd=sub), "frontmatter:sonnet")
    _agent_md(tmp_path / "proj" / "pacote" / ".claude" / "agents", "revisor", None)
    _assert_denied(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}, cwd=sub))


def test_walk_up_stops_at_the_repository_root(tmp_path: Path) -> None:
    _agent_md(tmp_path / ".claude" / "agents", "revisor", "sonnet")  # above the repo: never scanned
    (tmp_path / "proj" / ".git").mkdir(parents=True)
    _assert_denied(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}))


def test_plugin_agent_resolves_through_the_installed_path_never_a_stale_cache(tmp_path: Path) -> None:
    cache = tmp_path / "home" / ".claude" / "plugins" / "cache" / "mkt" / "plug"
    _agent_md(cache / "2.0.0" / "agents", "scan", None)
    stale = _agent_md(cache / "1.0.0" / "agents", "scan", "haiku")
    os.utime(stale.parent.parent, (2_000_000_000, 2_000_000_000))  # the stale copy is the NEWEST directory
    _install_plugin(tmp_path / "home", "plug", cache / "2.0.0")
    _assert_denied(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "plug:scan"}))
    _agent_md(cache / "2.0.0" / "agents", "scan", "sonnet")
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "plug:scan"}), "frontmatter:sonnet")


def test_plugin_subfolder_is_part_of_the_identifier(tmp_path: Path) -> None:
    install = tmp_path / "home" / ".claude" / "plugins" / "cache" / "mkt" / "plug" / "1.0.0"
    _agent_md(install / "agents" / "review", "security", "opus")
    _install_plugin(tmp_path / "home", "plug", install)
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "plug:review:security"}), "frontmatter:opus")


def test_synced_plugin_agent_is_found(tmp_path: Path) -> None:
    synced = tmp_path / "home" / ".claude" / "plugins" / "synced" / "abc_def" / "brand-voice" / "agents"
    _agent_md(synced, "quality-assurance", "haiku")
    run = _run(tmp_path, {**CALL, "subagent_type": "brand-voice:quality-assurance"})
    _assert_allowed(tmp_path, run, "frontmatter:haiku")


def test_unknown_plugin_agent_is_denied(tmp_path: Path) -> None:
    _assert_denied(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "plug:outro"}))


@pytest.mark.parametrize(("subagent_type", "default"), [("claude-code-guide", "haiku"), ("statusline-setup", "sonnet")])
def test_builtins_with_their_own_default_pass(tmp_path: Path, subagent_type: str, default: str) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": subagent_type}), f"builtin:{default}")


def test_a_definition_that_shadows_a_builtin_decides_instead(tmp_path: Path) -> None:
    _agent_md(tmp_path / "proj" / ".claude" / "agents", "claude-code-guide", None)
    _assert_denied(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "claude-code-guide"}))


@pytest.mark.parametrize("subagent_type", ["Explore", "Plan"])
def test_builtins_that_inherit_the_session_are_denied(tmp_path: Path, subagent_type: str) -> None:
    _assert_denied(tmp_path, _run(tmp_path, {**CALL, "subagent_type": subagent_type}))


# --- robustness: fail-open, never mute, never lose the verdict -----------------------------------

def test_other_tools_pass_without_a_trace(tmp_path: Path) -> None:
    proc, data = _run(tmp_path, {"command": "ls"}, tool_name="Bash")
    assert (proc.returncode, data, _log(tmp_path)) == (0, {}, [])


@pytest.mark.parametrize(("tool_input", "raw", "why"), [
    ("não é objeto", None, "malformed"),
    (None, "isto não é JSON", "error:JSONDecodeError"),
    (None, b"\xff\xfe{lixo", "error:JSONDecodeError"),
])
def test_garbage_fails_open_but_loud(tmp_path: Path, tool_input: Any, raw: str | bytes | None, why: str) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, tool_input, raw=raw), why)


def test_a_hostile_output_encoding_does_not_lose_the_deny(tmp_path: Path) -> None:
    _assert_denied(tmp_path, _run(tmp_path, CALL, env_extra={"PYTHONIOENCODING": "ascii"}))


def test_a_lone_surrogate_in_the_input_does_not_lose_the_deny(tmp_path: Path) -> None:
    raw = json.dumps({"session_id": "s", "cwd": str(tmp_path / "proj"), "tool_name": "Agent",
                      "tool_input": {"subagent_type": "gp\udc80", "description": "d"}})
    _assert_denied(tmp_path, _run(tmp_path, None, raw=raw))


def test_session_model_is_named_and_an_escaped_lookalike_is_ignored(tmp_path: Path) -> None:
    transcript = tmp_path / "t.jsonl"
    lines = [
        {"type": "assistant", "message": {"role": "assistant", "model": "claude-sonnet-5-5", "content": []}},
        {"type": "assistant", "message": {"role": "assistant", "model": "claude-opus-5-5", "content": []}},
        {"type": "user", "message": {"role": "user", "content": [{"type": "tool_result", "content": '"model":"claude-x"'}]}},
    ]
    transcript.write_text("".join(json.dumps(line, separators=(",", ":")) + "\n" for line in lines))
    assert "herdaria claude-opus-5-5:" in _assert_denied(tmp_path, _run(tmp_path, CALL, transcript=transcript))


def test_only_the_transcript_tail_is_read(tmp_path: Path) -> None:
    """A model line older than the tail window is never seen: the read cost is bounded."""
    transcript = tmp_path / "t.jsonl"
    old = json.dumps({"type": "assistant", "message": {"model": "claude-opus-5-5"}}, separators=(",", ":"))
    transcript.write_text(old + "\n" + ("x" * 1000 + "\n") * 300)
    assert "herdaria o modelo desta sessão:" in _assert_denied(tmp_path, _run(tmp_path, CALL, transcript=transcript))


def _record_quota(tmp_path: Path, five_hour_pct: float) -> None:
    path = tmp_path / "home" / ".claude" / "touring" / "rate_limits.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    now = int(time.time())
    path.write_text(json.dumps({"schema": 1, "providers": {"anthropic": {
        "captured_epoch": now - 120, "captured_at": "x", "session_id": "s", "model_id": "m", "cc_version": "v",
        "windows": {"five_hour": {"used_percentage": five_hour_pct, "resets_at": now + 3600},
                    "seven_day": {"used_percentage": 71, "resets_at": now + 86400 * 3}},
    }}}))


def test_deny_cites_the_recorded_quota_at_the_moment_of_choosing(tmp_path: Path) -> None:
    _record_quota(tmp_path, 42)
    reason = _assert_denied(tmp_path, _run(tmp_path, CALL))
    assert "Franquia Anthropic" in reason and "5 h 42%" in reason and "7 d 71%" in reason


def test_deny_without_a_recorded_quota_claims_nothing_about_it(tmp_path: Path) -> None:
    assert "Franquia" not in _assert_denied(tmp_path, _run(tmp_path, CALL))


def test_every_agent_call_is_logged_without_prompt_or_description(tmp_path: Path) -> None:
    _run(tmp_path, CALL)
    _run(tmp_path, {**CALL, "model": "haiku"})
    _run(tmp_path, {"command": "ls"}, tool_name="Bash")
    records = _log(tmp_path)
    assert [(r["allowed"], r["why"]) for r in records] == [(False, "deny"), (True, "explicit:haiku")]
    assert "SEGREDO-DO-PROMPT" not in json.dumps(records)
    assert "Achar callers" not in json.dumps(records)


# --- D8: one source for the predicate and for what the text declares ------------------------------

@pytest.mark.skipif(not SESSOES_IRMAS.exists(), reason="sessoes_irmas.py só existe na máquina do operador")
def test_provider_predicate_matches_sessoes_irmas() -> None:
    hook, tool = _load(HOOK, "agent_model_gate"), _load(SESSOES_IRMAS, "sessoes_irmas")
    urls = [None, "", "https://api.anthropic.com", "https://api.kimi.com/coding/", "https://api.moonshot.ai/anthropic",
            "https://api.minimax.io/anthropic", "https://proxy.example.org/v1"]
    assert [hook.provider_of(u) for u in urls] == [tool.provider_of(u) for u in urls]


def _gate_section_bullets() -> list[str]:
    doc = CANONICAL_DOC.read_text(encoding="utf-8")
    start = doc.index("**Aplicada por código: o gate `agent_model_gate`")
    section = doc[start:doc.index("\n\n**", start + 1)]
    bullets: list[str] = []
    for line in section.splitlines():
        if line.startswith("- "):
            bullets.append(line)
        elif bullets and line.startswith("  "):
            bullets[-1] += " " + line.strip()
    return bullets


def test_canonical_doc_lists_every_exemption_in_predicate_order() -> None:
    """The doc's bullet list under the gate paragraph mirrors EXEMPTIONS one-to-one, in order."""
    hook = _load(HOOK, "agent_model_gate")
    bullets = _gate_section_bullets()
    assert len(bullets) == len(hook.EXEMPTIONS), bullets
    misplaced = [(kw, b[:60]) for (kw, _), b in zip(hook.EXEMPTIONS, bullets, strict=True) if kw not in b]
    assert misplaced == []
