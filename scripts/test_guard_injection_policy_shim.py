"""guard_injection_policy.classify: a command that reaches `touring-hook` through quotes or a shell branch is the shim.

13/11/2026: the TeammateIdle registration is `if …; then exec "$HOME/.claude/hooks/touring-hook" teammate-idle; else
exec "$HOME/.claude/hooks/touring-hook" teammate-idle-gate; fi`. The detector looked for the literal `touring-hook `
(binary name + space), the closing quote sat between the two, and the guard reported the hook as an opaque gap
(exit 1) although every branch runs the Rust shim.
"""
from __future__ import annotations

import importlib.util
from pathlib import Path

_spec = importlib.util.spec_from_file_location("guard_injection_policy", Path(__file__).with_name("guard_injection_policy.py"))
guard = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(guard)

BRANCHED = ('if [ "${TOURING_TEAMMATE_IDLE_GATE_DISABLED:-}" = 1 ]; then exec "$HOME/.claude/hooks/touring-hook" '
            'teammate-idle; else exec "$HOME/.claude/hooks/touring-hook" teammate-idle-gate; fi')


def test_a_quoted_shim_in_both_branches_of_a_shell_if_is_the_shim() -> None:
    assert guard.classify(BRANCHED)[0] == "shim"
    assert guard.row_for("TeammateIdle", None, BRANCHED)[4] is True


def test_the_plain_form_is_still_the_shim_and_names_its_subcommand() -> None:
    assert guard.classify("$HOME/.claude/hooks/touring-hook session-start")[0] in ("shim", "shim-puro")
    assert guard.shim_subcommands("$HOME/.claude/hooks/touring-hook session-start") == ["session-start"]
    assert guard.shim_subcommands(BRANCHED) == ["teammate-idle", "teammate-idle-gate"]


def test_a_command_is_pure_only_when_every_branch_is_pure() -> None:
    pure = min(guard.PURE_SUBCOMMANDS)
    assert guard.classify(f"touring-hook {pure}")[0] == "shim-puro"
    assert guard.classify(f'if x; then "touring-hook" {pure}; else touring-hook session-start; fi')[0] == "shim"


def test_a_command_that_only_mentions_the_shim_name_is_not_the_shim() -> None:
    assert guard.classify("echo touring-hook")[0] == "opaco"
    assert guard.classify("touring pre-task-scout")[0] == "opaco"
