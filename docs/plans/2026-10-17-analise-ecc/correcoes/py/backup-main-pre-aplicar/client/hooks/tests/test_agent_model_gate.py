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
import io
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timedelta, timezone
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


def test_the_matrix_in_the_omitted_model_deny_does_not_send_the_model_to_a_refused_opus(tmp_path: Path) -> None:
    """The way back must not lead to a second deny: opus is refused for general-purpose, so the text says so."""
    reason = _assert_denied(tmp_path, _run(tmp_path, CALL))
    opus_line = next(line for line in reason.splitlines() if line.strip().startswith("opus"))
    assert "recusado" in opus_line and "general-purpose" in opus_line and "Explore" in opus_line
    assert "herdar de propósito" not in reason


def test_omitted_subagent_type_is_judged_as_general_purpose(tmp_path: Path) -> None:
    assert "general-purpose" in _assert_denied(tmp_path, _run(tmp_path, {"description": "d", "prompt": "p"}))


@pytest.mark.parametrize("alias", ["haiku", "sonnet"])
def test_explicit_model_passes_silently(tmp_path: Path, alias: str) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "model": alias}), f"explicit:{alias}")


def test_explicit_opus_passes_for_a_type_outside_the_two_classes(tmp_path: Path) -> None:
    """Rule 4: only the two classes are judged on fit — a custom type that chose opus decided it."""
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor", "model": "opus"}), "explicit:opus")


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


# --- fit: model x task, judged on the model the call NAMES ----------------------------------------

SEARCH_TYPES = ["Explore", "touring-scouter", "claude-code-guide", "statusline-setup"]
IMPLEMENT_TYPES = ["touring-engineer", "general-purpose", "touring-architect", "touring-auditor", "touring-scriber", "Plan"]


def _assert_denied_why(tmp_path: Path, run: Run, why: str) -> str:
    proc, data = run
    assert proc.returncode == 0, proc.stderr
    assert _denied(data), (data, proc.stderr)
    assert _log(tmp_path)[-1]["why"] == why, _log(tmp_path)[-1]
    return _reason(data)


@pytest.mark.parametrize("subagent_type", SEARCH_TYPES)
@pytest.mark.parametrize("model", ["sonnet", "opus", "fable", "Sonnet", " OPUS "])
def test_search_class_with_a_costlier_model_is_denied_and_taught_the_way_back(
        tmp_path: Path, subagent_type: str, model: str) -> None:
    run = _run(tmp_path, {**CALL, "subagent_type": subagent_type, "model": model})
    reason = _assert_denied_why(tmp_path, run, "fit:busca-exige-haiku")
    for needle in ('model: "haiku"', "matriz §1", "sessão principal", "touring-engineer", "general-purpose",
                   '"sonnet"', subagent_type, "Achar callers de foo"):
        assert needle in reason, needle
    assert "SEGREDO-DO-PROMPT" not in reason


@pytest.mark.parametrize("subagent_type", SEARCH_TYPES)
def test_search_class_with_haiku_passes(tmp_path: Path, subagent_type: str) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": subagent_type, "model": "haiku"}),
                    "explicit:haiku")


@pytest.mark.parametrize("subagent_type", IMPLEMENT_TYPES)
@pytest.mark.parametrize("model", ["opus", "Opus", " OPUS ", "claude-opus-5-5"])
def test_implement_class_with_opus_is_denied_and_taught_the_way_back(
        tmp_path: Path, subagent_type: str, model: str) -> None:
    run = _run(tmp_path, {**CALL, "subagent_type": subagent_type, "model": model})
    reason = _assert_denied_why(tmp_path, run, "fit:opus-fica-na-sessao-principal")
    for needle in ("sessão principal", '"sonnet"', '"haiku"', subagent_type, "Achar callers de foo"):
        assert needle in reason, needle
    assert "SEGREDO-DO-PROMPT" not in reason


@pytest.mark.parametrize("subagent_type", IMPLEMENT_TYPES)
@pytest.mark.parametrize("model", ["haiku", "sonnet", "Sonnet"])
def test_implement_class_with_haiku_or_sonnet_passes(tmp_path: Path, subagent_type: str, model: str) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": subagent_type, "model": model}),
                    f"explicit:{model}")


def test_the_deny_log_line_carries_the_model_the_call_named(tmp_path: Path) -> None:
    _run(tmp_path, {**CALL, "subagent_type": "Explore", "model": "opus"})
    last = _log(tmp_path)[-1]
    assert (last["allowed"], last["why"], last["model"], last["subagent_type"]) == (
        False, "fit:busca-exige-haiku", "opus", "Explore")


def _project_def(tmp_path: Path, name: str, model: str | None) -> Path:
    return _agent_md(tmp_path / "proj" / ".claude" / "agents", name, model)


@pytest.mark.parametrize("subagent_type", SEARCH_TYPES[:2])  # the built-in ones have no file to pin
@pytest.mark.parametrize("pinned", ["sonnet", "opus", "fable", "Sonnet", "claude-sonnet-4-6"])
def test_a_search_definition_pinning_a_costlier_model_is_denied_when_the_call_omits_model(
        tmp_path: Path, subagent_type: str, pinned: str) -> None:
    """The model of the DEFINITION is judged like the model of the call (it is what would run)."""
    _project_def(tmp_path, subagent_type, pinned)
    reason = _assert_denied_why(tmp_path, _run(tmp_path, {**CALL, "subagent_type": subagent_type}),
                                "fit:busca-exige-haiku")
    for needle in (f"A definição de {subagent_type} fixa `model: {pinned}`", 'model: "haiku"', "o parâmetro `model` da chamada vence",
                   "sessão principal", "Achar callers de foo"):
        assert needle in reason, needle
    last = _log(tmp_path)[-1]
    assert (last["origin"], last["judged"]) == ("definição", pinned)


@pytest.mark.parametrize("subagent_type", SEARCH_TYPES[:2])
def test_a_search_definition_pinning_haiku_or_inherit_or_nothing_has_no_fit_objection(
        tmp_path: Path, subagent_type: str) -> None:
    _project_def(tmp_path, subagent_type, "haiku")
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": subagent_type}), "frontmatter:haiku")
    _project_def(tmp_path, subagent_type, "inherit")
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": subagent_type}), "frontmatter:inherit")
    _project_def(tmp_path, subagent_type, None)
    _assert_denied(tmp_path, _run(tmp_path, {**CALL, "subagent_type": subagent_type}))  # omitted, undecided: the old deny


def test_an_implement_definition_pinning_opus_is_denied_and_sonnet_haiku_pass(tmp_path: Path) -> None:
    _project_def(tmp_path, "touring-architect", "opus")
    reason = _assert_denied_why(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "touring-architect"}),
                                "fit:opus-fica-na-sessao-principal")
    assert "A definição de touring-architect fixa `model: opus`" in reason and '"sonnet"' in reason
    for pinned in ("sonnet", "haiku"):
        _project_def(tmp_path, "touring-architect", pinned)
        _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "touring-architect"}), f"frontmatter:{pinned}")


def test_a_user_level_definition_is_judged_too(tmp_path: Path) -> None:
    _agent_md(tmp_path / "home" / ".claude" / "agents", "touring-scouter", "sonnet")
    _assert_denied_why(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "touring-scouter"}), "fit:busca-exige-haiku")


def test_fable_in_a_definition_needs_the_consent_like_fable_in_a_call(tmp_path: Path) -> None:
    _project_def(tmp_path, "revisor", "fable")
    reason = _assert_denied_why(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}), WHY_NO_CONSENT)
    assert "A definição de revisor fixa `model: fable`" in reason and "--consentir-fable 2h" in reason
    _consent(tmp_path, _in(timedelta(hours=1)))
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}), "fable:consentido")


def test_an_explicit_model_wins_over_the_definition_which_is_then_not_judged(tmp_path: Path) -> None:
    """`model: haiku` is the way back: it overrides a sonnet definition (the call wins), so it must pass."""
    _project_def(tmp_path, "touring-scouter", "sonnet")
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "touring-scouter", "model": "haiku"}),
                    "explicit:haiku")
    _assert_denied_why(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "touring-scouter", "model": "opus"}),
                       "fit:busca-exige-haiku")


def test_a_definition_outside_the_two_classes_is_never_fit_judged(tmp_path: Path) -> None:
    for pinned in ("opus", "sonnet"):
        _project_def(tmp_path, "revisor", pinned)
        _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "revisor"}), f"frontmatter:{pinned}")


def test_the_definition_fit_stops_at_the_session_wide_exemptions(tmp_path: Path) -> None:
    _project_def(tmp_path, "touring-scouter", "sonnet")
    call = {**CALL, "subagent_type": "touring-scouter"}
    _assert_allowed(tmp_path, _run(tmp_path, call, env_extra={"AGENT_MODEL_GATE_DISABLED": "1"}), "disabled")
    _assert_allowed(tmp_path, _run(tmp_path, call, env_extra={"ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/"}),
                    "irmã:kimi")


def test_a_search_definition_beats_the_global_subagent_env_in_precedence(tmp_path: Path) -> None:
    """call > definition > env: the sonnet definition is what would run, so it is judged, env or not."""
    _project_def(tmp_path, "touring-scouter", "sonnet")
    _assert_denied_why(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "touring-scouter"},
                                      env_extra={"CLAUDE_CODE_SUBAGENT_MODEL": "haiku"}), "fit:busca-exige-haiku")


def test_a_builtin_default_is_not_a_frontmatter_and_keeps_passing(tmp_path: Path) -> None:
    """statusline-setup's built-in default is sonnet (docs, 08/10): no file pins it, so no fit rule sees it."""
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "statusline-setup"}), "builtin:sonnet")


@pytest.mark.parametrize(("env_extra", "subagent_type", "model", "why"), [
    ({"AGENT_MODEL_GATE_DISABLED": "1"}, "Explore", "opus", "disabled"),
    ({"AGENT_MODEL_GATE_DISABLED": "1"}, "general-purpose", "fable", "disabled"),
    ({"ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/"}, "general-purpose", "opus", "irmã:kimi"),
    ({"ANTHROPIC_BASE_URL": "https://api.minimax.io/anthropic"}, "Explore", "sonnet", "irmã:minimax"),
    ({"ANTHROPIC_BASE_URL": "https://api.minimax.io/anthropic"}, "general-purpose", "fable", "irmã:minimax"),
])
def test_session_wide_exemptions_come_before_the_fit_rules(
        tmp_path: Path, env_extra: dict[str, str], subagent_type: str, model: str, why: str) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": subagent_type, "model": model},
                                   env_extra=env_extra), why)


def test_fork_runs_in_the_session_model_so_no_rule_applies_to_it(tmp_path: Path) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "fork", "model": "fable"}), "fork")


def test_the_global_subagent_env_still_passes_an_omitted_model_but_never_shields_an_explicit_one(
        tmp_path: Path) -> None:
    """Claude Code resolves call > definition > env: an explicit `model` overrides the env, so it is judged."""
    env = {"CLAUDE_CODE_SUBAGENT_MODEL": "haiku"}
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "Explore"}, env_extra=env), "env:haiku")
    _assert_denied_why(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "Explore", "model": "opus"}, env_extra=env),
                       "fit:busca-exige-haiku")


def test_unknown_or_non_alias_models_have_no_fit_rule(tmp_path: Path) -> None:
    _assert_allowed(tmp_path, _run(tmp_path, {**CALL, "subagent_type": "Explore", "model": "inherit"}),
                    "explicit:inherit")


# --- Fable: consent is a human, time-boxed decision ----------------------------------------------

CONSENT_NAME = "fable_consent.json"


def _consent_path(tmp_path: Path) -> Path:
    return tmp_path / "home" / ".claude" / "touring" / CONSENT_NAME


def _consent(tmp_path: Path, valido_ate: Any, **extra: Any) -> Path:
    path = _consent_path(tmp_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({"valido_ate": valido_ate, "criado_em": "2026-10-17T10:00:00-03:00",
                                "motivo": "teste", **extra}))
    return path


def _in(delta: timedelta) -> str:
    return (datetime.now().astimezone() + delta).isoformat(timespec="seconds")


FABLE_CALL = {**CALL, "model": "fable"}
WHY_NO_CONSENT = "fable:sem-consentimento"


@pytest.mark.parametrize("subagent_type", ["general-purpose", "touring-engineer", "revisor", "Plan"])
def test_fable_without_consent_is_denied_and_the_human_is_named_as_the_way_back(
        tmp_path: Path, subagent_type: str) -> None:
    reason = _assert_denied_why(tmp_path, _run(tmp_path, {**FABLE_CALL, "subagent_type": subagent_type}),
                                "fable:sem-consentimento")
    for needle in ("Fable", "Gabriel", "! python3 ~/.claude/hooks/agent_model_gate.py --consentir-fable 2h",
                   subagent_type, "Achar callers de foo"):
        assert needle in reason, needle
    assert "SEGREDO-DO-PROMPT" not in reason


def test_fable_with_a_live_consent_is_allowed_and_logged(tmp_path: Path) -> None:
    _consent(tmp_path, _in(timedelta(hours=1)))
    _assert_allowed(tmp_path, _run(tmp_path, FABLE_CALL), "fable:consentido")
    _assert_allowed(tmp_path, _run(tmp_path, {**FABLE_CALL, "model": "Fable", "subagent_type": "revisor"}),
                    "fable:consentido")


def test_a_consent_with_a_utc_z_suffix_is_read(tmp_path: Path) -> None:
    _consent(tmp_path, (datetime.now(timezone.utc) + timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ"))
    _assert_allowed(tmp_path, _run(tmp_path, FABLE_CALL), "fable:consentido")


@pytest.mark.parametrize(("valido_ate", "state"), [
    pytest.param(lambda: _in(timedelta(seconds=-30)), "expirado", id="expired"),
    pytest.param(lambda: (datetime.now(timezone.utc).replace(tzinfo=None) + timedelta(hours=1)).isoformat(timespec="seconds"), "ilegível",
                 id="naive-timestamp-has-no-zone"),
    pytest.param(lambda: "amanhã", "ilegível", id="not-iso"),
    pytest.param(lambda: 12345, "ilegível", id="not-a-string"),
    pytest.param(lambda: None, "ilegível", id="null"),
])
def test_a_consent_that_is_not_live_does_not_count_and_the_deny_says_why(
        tmp_path: Path, valido_ate: Any, state: str) -> None:
    _consent(tmp_path, valido_ate())
    reason = _assert_denied_why(tmp_path, _run(tmp_path, FABLE_CALL), "fable:sem-consentimento")
    assert state in reason
    assert _log(tmp_path)[-1]["consent"] == state


def test_a_consent_beyond_the_ceiling_does_not_count_even_if_hand_written(tmp_path: Path) -> None:
    """The 24 h ceiling lives in the READER: a year-9999 file is not an eternal consent."""
    _consent(tmp_path, _in(timedelta(hours=25)))
    reason = _assert_denied_why(tmp_path, _run(tmp_path, FABLE_CALL), WHY_NO_CONSENT)
    assert "acima-do-teto" in reason and _log(tmp_path)[-1]["consent"] == "acima-do-teto"
    _consent(tmp_path, "9999-12-31T00:00:00+00:00")
    _assert_denied_why(tmp_path, _run(tmp_path, FABLE_CALL), WHY_NO_CONSENT)
    _consent(tmp_path, _in(timedelta(hours=23, minutes=50)))
    _assert_allowed(tmp_path, _run(tmp_path, FABLE_CALL), "fable:consentido")


@pytest.mark.parametrize("content", ["isto não é JSON", "[1, 2]", "", '{"criado_em": "x"}'])
def test_a_corrupt_or_empty_consent_file_is_no_consent(tmp_path: Path, content: str) -> None:
    path = _consent_path(tmp_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)
    _assert_denied_why(tmp_path, _run(tmp_path, FABLE_CALL), "fable:sem-consentimento")


def test_a_missing_consent_is_logged_as_absent(tmp_path: Path) -> None:
    _run(tmp_path, FABLE_CALL)
    assert _log(tmp_path)[-1]["consent"] == "ausente"


@pytest.mark.parametrize("subagent_type", SEARCH_TYPES)
def test_a_live_consent_does_not_make_fable_a_fit_for_search(tmp_path: Path, subagent_type: str) -> None:
    _consent(tmp_path, _in(timedelta(hours=1)))
    _assert_denied_why(tmp_path, _run(tmp_path, {**FABLE_CALL, "subagent_type": subagent_type}),
                       "fit:busca-exige-haiku")


def test_a_live_consent_does_not_license_opus_for_the_implement_class(tmp_path: Path) -> None:
    _consent(tmp_path, _in(timedelta(hours=1)))
    _assert_denied_why(tmp_path, _run(tmp_path, {**CALL, "model": "opus"}), "fit:opus-fica-na-sessao-principal")


def test_a_consent_in_another_home_is_invisible(tmp_path: Path) -> None:
    """The consent is read from the HOME the hook runs in, never from a path the call can name."""
    elsewhere = tmp_path / "outro" / CONSENT_NAME
    elsewhere.parent.mkdir(parents=True)
    elsewhere.write_text(json.dumps({"valido_ate": _in(timedelta(hours=1))}))
    _assert_denied_why(tmp_path, _run(tmp_path, {**FABLE_CALL, "consent_path": str(elsewhere)}),
                       "fable:sem-consentimento")


# --- consent CLI: the human's command --------------------------------------------------------------

def _cli(tmp_path: Path, *args: str) -> subprocess.CompletedProcess[str]:
    home = tmp_path / "home"
    home.mkdir(exist_ok=True)
    env = {k: v for k, v in os.environ.items() if k not in SESSION_ENV}
    env.update(HOME=str(home))
    return subprocess.run([sys.executable, str(HOOK), *args], capture_output=True, text=True, env=env,
                          timeout=15, check=False, stdin=subprocess.DEVNULL)


def _events(tmp_path: Path) -> list[dict[str, Any]]:
    return [r for r in _log(tmp_path) if "evento" in r]


def test_consent_cli_writes_the_file_the_gate_reads_and_logs_the_event(tmp_path: Path) -> None:
    before = datetime.now().astimezone()
    proc = _cli(tmp_path, "--consentir-fable", "2h", "--motivo", "causa-raiz do deadlock X")
    assert proc.returncode == 0, proc.stderr
    data = json.loads(_consent_path(tmp_path).read_text())
    valid_until = datetime.fromisoformat(data["valido_ate"])
    assert valid_until.tzinfo is not None
    assert timedelta(hours=2) - timedelta(seconds=90) <= valid_until - before <= timedelta(hours=2, seconds=90)
    assert datetime.fromisoformat(data["criado_em"]).tzinfo is not None
    assert data["motivo"] == "causa-raiz do deadlock X"
    assert data["valido_ate"] in proc.stdout
    (event,) = _events(tmp_path)
    assert (event["evento"], event["valido_ate"], event["motivo"]) == (
        "consentir-fable", data["valido_ate"], "causa-raiz do deadlock X")
    _assert_allowed(tmp_path, _run(tmp_path, FABLE_CALL), "fable:consentido")


@pytest.mark.parametrize(("duration", "seconds"), [("30m", 1800), ("1h30m", 5400), ("90m", 5400), ("24h", 86400),
                                                    ("1d", 86400), ("45s", 45)])
def test_consent_cli_parses_durations(tmp_path: Path, duration: str, seconds: int) -> None:
    before = datetime.now().astimezone()
    assert _cli(tmp_path, "--consentir-fable", duration).returncode == 0
    delta = datetime.fromisoformat(json.loads(_consent_path(tmp_path).read_text())["valido_ate"]) - before
    assert abs(delta.total_seconds() - seconds) <= 90


@pytest.mark.parametrize("duration", ["abc", "0m", "25h", "2", "-1h", "2 h", "1h 30m", "1w", "9999999999999d", ""])
def test_consent_cli_rejects_a_bad_duration_and_writes_nothing(tmp_path: Path, duration: str) -> None:
    proc = _cli(tmp_path, "--consentir-fable", duration)
    assert proc.returncode == 2, (proc.returncode, proc.stderr)
    assert not _consent_path(tmp_path).exists()


@pytest.mark.parametrize(("duration", "needle"), [("25h", "24h"), ("abc", "duração inválida"), ("0m", "24h")])
def test_consent_cli_says_what_is_wrong_with_the_duration(tmp_path: Path, duration: str, needle: str) -> None:
    assert needle in _cli(tmp_path, "--consentir-fable", duration).stderr


def test_consent_cli_without_a_duration_fails_without_writing(tmp_path: Path) -> None:
    proc = _cli(tmp_path, "--consentir-fable")
    assert proc.returncode == 2
    assert not _consent_path(tmp_path).exists()


def test_consent_cli_renews_by_overwriting(tmp_path: Path) -> None:
    _cli(tmp_path, "--consentir-fable", "30m")
    first = json.loads(_consent_path(tmp_path).read_text())["valido_ate"]
    _cli(tmp_path, "--consentir-fable", "2h")
    assert json.loads(_consent_path(tmp_path).read_text())["valido_ate"] > first
    assert len(_events(tmp_path)) == 2


def test_revoke_cli_removes_the_consent_and_the_gate_denies_again(tmp_path: Path) -> None:
    _cli(tmp_path, "--consentir-fable", "2h")
    proc = _cli(tmp_path, "--revogar-fable")
    assert proc.returncode == 0, proc.stderr
    assert not _consent_path(tmp_path).exists()
    assert [e["evento"] for e in _events(tmp_path)] == ["consentir-fable", "revogar-fable"]
    _assert_denied_why(tmp_path, _run(tmp_path, FABLE_CALL), "fable:sem-consentimento")


def test_revoke_cli_without_a_consent_is_a_quiet_success(tmp_path: Path) -> None:
    proc = _cli(tmp_path, "--revogar-fable")
    assert proc.returncode == 0, proc.stderr
    assert "nenhum" in proc.stdout.lower()


def test_cli_modes_do_not_read_stdin_and_hook_mode_without_args_still_works(tmp_path: Path) -> None:
    assert _cli(tmp_path, "--revogar-fable").returncode == 0
    _assert_denied_why(tmp_path, _run(tmp_path, FABLE_CALL), "fable:sem-consentimento")


# --- fail-open stays loud and distinguishable from every decision ----------------------------------

def test_an_internal_error_in_the_new_rules_fails_open_with_an_error_why(
        tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]) -> None:
    hook = _load(HOOK, "agent_model_gate_failopen")
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    for name in ("ANTHROPIC_BASE_URL", "AGENT_MODEL_GATE_DISABLED", "CLAUDE_CODE_SUBAGENT_MODEL"):
        monkeypatch.delenv(name, raising=False)

    def boom(*_args: Any, **_kwargs: Any) -> Any:
        raise RuntimeError("consent store exploded")

    monkeypatch.setattr(hook, "fable_consent", boom)
    payload = json.dumps({"session_id": "s", "cwd": str(tmp_path), "tool_name": "Agent", "tool_input": FABLE_CALL})
    monkeypatch.setattr(sys, "stdin", io.TextIOWrapper(io.BytesIO(payload.encode())))
    assert hook.main() == 0
    captured = capsys.readouterr()
    assert captured.out == ""
    assert "RuntimeError" in captured.err
    last = json.loads((home / ".claude" / "touring" / "agent_model_gate.jsonl").read_text().splitlines()[-1])
    assert (last["allowed"], last["why"]) == (True, "error:RuntimeError")


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


def _fit_section() -> str:
    doc = CANONICAL_DOC.read_text(encoding="utf-8")
    start = doc.index("**Ajuste modelo × tarefa e consentimento do Fable")
    return doc[start:doc.index("\n\n**", start + 1)]


def test_canonical_doc_declares_the_fit_classes_whys_and_consent_from_the_same_source() -> None:
    """D8: what the doc tells Gabriel and what the predicate enforces derive from one set of constants."""
    hook, section = _load(HOOK, "agent_model_gate"), _fit_section()
    missing = [n for n in (*hook.SEARCH_TYPES, *hook.IMPLEMENT_TYPES) if f"`{n}`" not in section]
    assert missing == []
    for why in (hook.WHY_SEARCH, hook.WHY_OPUS, hook.WHY_NO_CONSENT, hook.WHY_CONSENT):
        assert why in section, why
    for needle in (hook.CONSENT_NAME, "--consentir-fable", "--revogar-fable", "24 h", "Limitação conhecida",
                   "**definição**", "inherit", "statusline-setup", "fable_consent_guard.sh"):
        assert needle in section, needle

