"""Mutation driver for C4/C10: `mut.py apply <id>` / `restore <id>` / `cmd <id>` / `ids`.

Every mutant must make at least one test FAIL (rc != 0). Backups live next to this script, so a
crashed run is repaired by `restore`. The worktree is dedicated to this task: nothing else edits it.
"""
import os
import shutil
import sys

WT = "/home/gabrielgadea/projects/touring-wt-ecc-hk"
BAK = os.path.join(os.path.dirname(os.path.abspath(__file__)), "mutbak")
RT = "crates/touring-hook-runtime/src/hook_runtime_ext.rs"
MAIN = "crates/touring-hooks/src/main.rs"
REG = "crates/touring-dispatch/src/hook_registry.rs"
CB = "crates/touring-cli/src/cli/context_budget.rs"

T_RT = "cargo test -p touring-hook-runtime --lib stdin_read_tests --no-fail-fast"
T_DISP = "cargo test -p touring-dispatch --features pre-hooks,post-hooks --lib hook_registry --no-fail-fast"
T_E2E = "cargo test -p touring-hooks --features pre-hooks,post-hooks --test stdin_guard_fail_closed_e2e --no-fail-fast"
T_CB = "cargo test -p touring-cli --lib context_budget --no-fail-fast"

MUTANTS = {
    "M1-branch-removed": (MAIN, "    if read.state.is_unreadable()\n        && touring_hooks", "    if false && read.state.is_unreadable()\n        && touring_hooks", T_E2E),
    "M2-timeout-not-unreadable": (RT, "            StdinReadState::Timeout\n                | StdinReadState::ReadError", "            StdinReadState::ReadError", T_RT),
    "M3-cap-ignored": (RT, "                    if n > room {", "                    if false && n > room {", T_RT),
    "M4-no-recovery": (RT, "                Ok(v) if v.is_object() => {", "                Ok(v) if false && v.is_object() => {", T_RT),
    "M5-guard-missing": (REG, '    "pre-edit-prevention",\n    "pre-read",', '    "pre-read",', T_DISP),
    "M6-advisory-is-guard": (REG, "    GUARD_HOOK_NAMES.contains(&name)\n        || (name", "    true\n        || GUARD_HOOK_NAMES.contains(&name)\n        || (name", T_DISP),
    "M7-original-text-only": (CB, "                &text,\n                emitter.as_deref(),", "                &text,\n                None,", T_CB),
    "M8-owns-defers": (CB, '("analyzing blast radius", "pre-edit", Claim::Owns)', '("analyzing blast radius", "pre-edit", Claim::Fallback)', T_CB),
    "M9-needle-removed": (CB, '("syntax pre-check", "pre-edit-prevention", Claim::Owns)', '("syntax pre-checkX", "pre-edit-prevention", Claim::Owns)', T_CB),
}


def main():
    op = sys.argv[1]
    if op == "ids":
        print("\n".join(MUTANTS))
        return
    mid = sys.argv[2]
    path, old, new, cmd = MUTANTS[mid]
    full = os.path.join(WT, path)
    bak = os.path.join(BAK, mid + ".orig")
    if op == "cmd":
        print(cmd)
    elif op == "apply":
        os.makedirs(BAK, exist_ok=True)
        text = open(full).read()
        if text.count(old) != 1:
            sys.exit(f"{mid}: pattern occurs {text.count(old)}x in {path}")
        shutil.copy2(full, bak)
        open(full, "w").write(text.replace(old, new, 1))
        print(f"{mid}: applied")
    elif op == "restore":
        if os.path.exists(bak):
            shutil.copy2(bak, full)
            os.remove(bak)
            print(f"{mid}: restored")
    else:
        sys.exit("usage: mut.py ids|cmd|apply|restore [id]")


main()
