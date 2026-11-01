"""test_fable_consent_guard.py — o consentimento do Fable fica só com o humano.

O guard é shell puro (custo ~ms em todo Bash) e lê o JSON do PreToolUse da stdin. Cada teste roda o
script REAL por subprocesso com payload sintético no formato do Claude Code (chaves na ordem real:
sessão, transcript, cwd, evento, ferramenta, entrada). O guard só NEGA ou fica em silêncio: um
caminho de permissão sem saída é o veredito correto, então todo teste de permissão confere também
rc 0 e stdout vazio — um script ausente ou quebrado não passa por "silêncio" (rc != 0).
"""

from __future__ import annotations

import json
import os
import re
import subprocess
from typing import Any

import pytest
from helpers import CLIENT_HOOKS, REPO_ROOT, HookResult, run_hook_sh, schema_violations

SCRIPT = CLIENT_HOOKS / "fable_consent_guard.sh"
HOOKS_SETTINGS = REPO_ROOT / "client" / "claude" / "hooks.settings.json"
REGISTERED = "$HOME/.claude/hooks/fable_consent_guard.sh"
CONSENT = "~/.claude/touring/fable_consent.json"
CLI = "python3 ~/.claude/hooks/agent_model_gate.py"


def _payload(tool_name: str, tool_input: Any, **top: Any) -> dict[str, Any]:
    return {
        "session_id": "hooks-contract-fable-guard",
        "transcript_path": top.pop("transcript_path", "/home/u/.claude/projects/p/s.jsonl"),
        "cwd": top.pop("cwd", "/home/u/projects/p"),
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": tool_name,
        "tool_input": tool_input,
        "tool_use_id": "toolu_01",
        **top,
    }


def _bash(command: str, **top: Any) -> dict[str, Any]:
    return _payload("Bash", {"command": command, "description": "x"}, **top)


def _run(payload: dict[str, Any], **json_kwargs: Any) -> HookResult:
    if not json_kwargs:
        return run_hook_sh(SCRIPT, payload)
    proc = subprocess.run(["bash", str(SCRIPT)], input=json.dumps(payload, **json_kwargs).encode(),
                          capture_output=True, timeout=15, check=False)
    return HookResult(SCRIPT, payload, proc)


def _assert_denied(result: HookResult) -> str:
    assert result.rc == 0, result.stderr_raw
    assert result.is_deny(), (result.stdout_raw, result.stderr_raw)
    assert schema_violations("PreToolUse", result) == []
    hso = (result.json or {})["hookSpecificOutput"]
    assert hso["hookEventName"] == "PreToolUse"
    return str(hso["permissionDecisionReason"])


def _assert_silent(result: HookResult) -> None:
    assert result.rc == 0, result.stderr_raw
    assert result.stdout_raw == "", result.stdout_raw
    assert result.stderr_raw == "", result.stderr_raw


# --- Bash: the CLI that grants and the file it writes ---------------------------------------------

@pytest.mark.parametrize("command", [
    f"{CLI} --consentir-fable 2h",
    f'{CLI} --consentir-fable 30m --motivo "causa-raiz do deadlock"',
    "python3 /home/gabrielgadea/.claude/hooks/agent_model_gate.py --consentir-fable 1h",
    "cd client/hooks && python3 agent_model_gate.py --consentir-fable 90m",
    f"sh -c '{CLI} --consentir-fable 2h'",
    f'bash -lc "{CLI} --consentir-fable 2h"',
    "python3 - <<'EOF'\nimport subprocess\nsubprocess.run(['python3', 'agent_model_gate.py', '--consentir-fable', '2h'])\nEOF",
    f"echo ok && {CLI} --consentir-fable 2h && echo done",
    f"{CLI} --revogar-fable ; {CLI} --consentir-fable 2h",
], ids=["plain", "motivo", "absolute", "cd-relative", "sh-c", "bash-lc", "heredoc-python", "chained", "revogar-then-consentir"])
def test_the_consent_cli_is_denied_to_the_model(command: str) -> None:
    reason = _assert_denied(_run(_bash(command)))
    for needle in ("Gabriel", "! python3 ~/.claude/hooks/agent_model_gate.py --consentir-fable 2h", "`!`"):
        assert needle in reason, needle


@pytest.mark.parametrize("command", [
    f"echo '{{\"valido_ate\": \"2099-01-01T00:00:00+00:00\"}}' > {CONSENT}",
    "echo x > /home/gabrielgadea/.claude/touring/fable_consent.json",
    f"cat > {CONSENT} <<'EOF'\n{{\"valido_ate\": \"2099-01-01T00:00:00+00:00\"}}\nEOF",
    f"printf '{{}}' | tee {CONSENT}",
    f"cp /tmp/x.json {CONSENT}",
    "python3 -c \"open('/home/g/.claude/touring/fable_consent.json','w').write('{}')\"",
    "touch $HOME/.claude/touring/fable_consent.json",
    f"sh -c 'echo x > {CONSENT}'",
], ids=["echo-redirect", "absolute", "heredoc", "tee", "cp", "python-c", "touch-home", "sh-c-redirect"])
def test_writing_the_consent_file_by_bash_is_denied(command: str) -> None:
    _assert_denied(_run(_bash(command)))


# --- Write / Edit / MultiEdit / NotebookEdit: the path ---------------------------------------------

@pytest.mark.parametrize("tool_name", ["Write", "Edit", "MultiEdit"])
@pytest.mark.parametrize("path", ["/home/gabrielgadea/.claude/touring/fable_consent.json",
                                  "~/.claude/touring/fable_consent.json",
                                  "/tmp/qualquer/fable_consent.json"])
def test_file_tools_aimed_at_the_consent_file_are_denied(tool_name: str, path: str) -> None:
    body: dict[str, Any] = {"file_path": path, "content": "{}"}
    if tool_name != "Write":
        body = {"file_path": path, "old_string": "a", "new_string": "b"} if tool_name == "Edit" else {
            "file_path": path, "edits": [{"old_string": "a", "new_string": "b"}]}
    _assert_denied(_run(_payload(tool_name, body)))


def test_notebook_edit_aimed_at_the_consent_file_is_denied() -> None:
    _assert_denied(_run(_payload("NotebookEdit", {"notebook_path": "/x/fable_consent.json", "new_source": "x"})))


def test_the_path_is_found_even_when_the_content_comes_first() -> None:
    body = {"content": "x" * 5000, "file_path": "/h/.claude/touring/fable_consent.json"}
    _assert_denied(_run(_payload("Write", body)))


@pytest.mark.parametrize("kwargs", [{}, {"separators": (",", ":")}, {"indent": 2}], ids=["default", "compact", "pretty"])
def test_the_json_formatting_does_not_matter(kwargs: dict[str, Any]) -> None:
    _assert_denied(_run(_bash(f"{CLI} --consentir-fable 2h"), **kwargs))
    _assert_denied(_run(_payload("Write", {"file_path": "/x/fable_consent.json", "content": ""}), **kwargs))
    _assert_silent(_run(_bash("ls -la"), **kwargs))


# --- what must stay quiet: revoking is safe, and unrelated work must not be taxed -------------------

def test_revoking_is_allowed_to_the_model() -> None:
    _assert_silent(_run(_bash(f"{CLI} --revogar-fable")))


@pytest.mark.parametrize("command", [
    "ls", "git status", "ruff check client/hooks/agent_model_gate.py",
    "python3 -m pytest client/hooks/tests/test_fable_consent_guard.py -q",
    "python3 -m pytest client/hooks/tests/test_agent_model_gate.py -q",
    "chmod +x client/hooks/fable_consent_guard.sh",
    "bash client/hooks/fable_consent_guard.sh < /dev/null",
    f"{CLI} --help",
    "cat ~/.claude/touring/agent_model_gate.jsonl",
    "grep -n consentir docs/economia-de-creditos-e-contexto.md",
    "grep -n fable_consent client/hooks/agent_model_gate.py",
    "echo --consentir",
], ids=["ls", "git", "ruff-gate", "pytest-guard", "pytest-gate", "chmod-guard", "run-guard", "gate-help", "cat-log",
        "grep-doc", "grep-name-without-ext", "partial-flag"])
def test_unrelated_bash_is_allowed_in_silence(command: str) -> None:
    _assert_silent(_run(_bash(command)))


def test_names_that_merely_contain_fable_consent_are_not_the_consent_file() -> None:
    """The guard's own script, its test and the hook directory contain `fable_consent`: they must stay editable."""
    for path in ("/p/client/hooks/fable_consent_guard.sh", "/p/client/hooks/tests/test_fable_consent_guard.py",
                 "/p/docs/fable_consent_notes.md"):
        for tool in ("Write", "Edit", "MultiEdit"):
            _assert_silent(_run(_payload(tool, {"file_path": path, "content": "x", "old_string": "a", "new_string": "b"})))


def test_content_that_mentions_the_file_or_the_flag_is_not_judged_only_the_path_is() -> None:
    text = f'CONSENT = "fable_consent.json"; usage: {CLI} --consentir-fable 2h'
    _assert_silent(_run(_payload("Write", {"file_path": "/p/docs/a.md", "content": text})))
    _assert_silent(_run(_payload("Edit", {"file_path": "/p/client/hooks/x.py", "old_string": text, "new_string": text})))
    escaped = _payload("Write", {"file_path": "/p/docs/a.md", "content": '{"file_path": "/x/fable_consent.json"}'})
    _assert_silent(_run(escaped))
    multi = _payload("MultiEdit", {"file_path": "/p/a.py", "edits": [{"old_string": '"file_path": "/x/fable_consent.json"',
                                                                     "new_string": "y"}]})
    _assert_silent(_run(multi))


def test_reading_the_consent_file_is_allowed() -> None:
    _assert_silent(_run(_payload("Read", {"file_path": "/h/.claude/touring/fable_consent.json"})))
    _assert_silent(_run(_payload("Grep", {"pattern": "fable_consent.json", "path": "/p"})))
    _assert_silent(_run(_payload("Agent", {"description": "--consentir-fable", "prompt": "fable_consent.json"})))


def test_a_path_in_the_envelope_is_not_the_tool_input() -> None:
    """cwd/transcript_path that happen to contain the file name must not tax an unrelated call."""
    envelope = {"cwd": "/tmp/fable_consent.json.d", "transcript_path": "/tmp/fable_consent.json/t.jsonl"}
    _assert_silent(_run(_bash("ls", **envelope)))
    _assert_silent(_run(_payload("Write", {"file_path": "/p/a.md", "content": "x"}, **envelope)))


# --- robustness: the guard never blocks on its own failure ------------------------------------------

@pytest.mark.parametrize("raw", [b"", b"isto nao e json", b"{}", b'{"tool_name": 7}', b'{"tool_name": "Bash"}',
                                 b'{"tool_name": "Bash", "tool_input": {"description": "sem command"}}',
                                 b'{"tool_name": "Write", "tool_input": {}}', b"\xff\xfe{lixo"])
def test_garbage_input_exits_zero_in_silence(raw: bytes) -> None:
    proc = subprocess.run(["bash", str(SCRIPT)], input=raw, capture_output=True, timeout=15, check=False)
    _assert_silent(HookResult(SCRIPT, {}, proc))


def test_the_script_is_executable_and_runs_as_registered_with_no_python() -> None:
    assert os.access(SCRIPT, os.X_OK)
    first = SCRIPT.read_text(encoding="utf-8").splitlines()[0]
    assert first == "#!/usr/bin/env bash"
    body = "\n".join(line for line in SCRIPT.read_text(encoding="utf-8").splitlines() if not line.lstrip().startswith("#"))
    invoked = re.findall(r"(?m)(?:^|[;&|(]|\$\()\s*(python3?|jq|sed|awk|grep|perl|cat)\b", body)
    assert invoked == [] or invoked == ["cat"], invoked  # the only fork allowed is one `cat` to read stdin
    proc = subprocess.run([str(SCRIPT)], input=json.dumps(_bash(f"{CLI} --consentir-fable 2h")).encode(),
                          capture_output=True, timeout=15, check=False)
    assert HookResult(SCRIPT, {}, proc).is_deny()


def test_a_large_payload_is_still_judged_quickly_and_correctly() -> None:
    big = _payload("Write", {"file_path": "/p/big.txt", "content": "x" * 2_000_000})
    _assert_silent(_run(big))
    big_bash = _bash("echo " + "y" * 1_000_000 + f" && {CLI} --consentir-fable 2h")
    _assert_denied(_run(big_bash))


# --- registration: the template the installer applies must cover exactly the tools the script handles ---

def _script_tools() -> set[str]:
    """Tool names in the script's `case $tool in` arms (`Write | Edit | ...)` and `Bash)`)."""
    arms = re.findall(r"(?m)^  ([A-Za-z]+(?: \| [A-Za-z]+)*)\)$", SCRIPT.read_text(encoding="utf-8"))
    return {name for arm in arms for name in arm.split(" | ")}


def _registered_matchers() -> list[tuple[str, int | None]]:
    hooks = json.loads(HOOKS_SETTINGS.read_text(encoding="utf-8"))["PreToolUse"]
    return [(group["matcher"], hook.get("timeout")) for group in hooks for hook in group["hooks"]
            if hook["command"] == REGISTERED]


def test_the_template_registers_the_guard_for_exactly_the_tools_the_script_handles() -> None:
    registered = _registered_matchers()
    assert sorted(m for m, _ in registered) == ["Bash", "Write|Edit|MultiEdit|NotebookEdit"]
    covered = {tool for matcher, _ in registered for tool in matcher.split("|")}
    assert covered == _script_tools() == {"Bash", "Write", "Edit", "MultiEdit", "NotebookEdit"}


def test_the_guard_registration_has_the_tight_timeout_it_was_measured_for() -> None:
    assert all(timeout == 2 for _, timeout in _registered_matchers())


def test_the_canonical_doc_declares_what_the_script_matches() -> None:
    """D8: the patterns the doc tells Gabriel about are the ones in the script's `case` arms."""
    doc = (REPO_ROOT / "docs" / "economia-de-creditos-e-contexto.md").read_text(encoding="utf-8")
    patterns = {p for arm in re.findall(r"(?m)^\s+(\*[^)]*\*(?: \| \*[^)]*\*)*)\) ", SCRIPT.read_text(encoding="utf-8"))
                for p in arm.split(" | ")}
    assert {p.strip("*") for p in patterns} == {"--consentir-fable", "fable_consent.json"}, patterns
    assert all(p.strip("*") in doc for p in patterns)
    assert "fable_consent_guard.sh" in doc

