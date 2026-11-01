#!/usr/bin/env python3
"""hooks_runnable.py — Verifica que todo command registrado em hooks.*.hooks[]
é executável (ou legível, se for argumento de intérprete).

Replica a semântica de test_registered_hook_commands_are_runnable
em ~/.claude/skills/loop-engineering/.

Uso: hooks_runnable.py <settings.json> [--home <dir>] [--json]
Exit 1 se qualquer comando falhar na verificação.
"""

import argparse
import json
import os
import re
import shlex
import shutil
import sys
from pathlib import Path

INTERPRETERS = {"python3", "python", "node", "bash", "sh"}


def expand_path(raw: str, home_dir: Path) -> str:
    """Expande $HOME, ~, $CLAUDE_* no início do token."""
    # Substitui $HOME e ~ pelo home_dir explícito
    result = raw.replace("$HOME", str(home_dir))
    result = result.replace("${HOME}", str(home_dir))
    if result.startswith("~/"):
        result = str(home_dir) + result[1:]
    elif result == "~":
        result = str(home_dir)
    # Remove variáveis $CLAUDE_* que não conhecemos (substituir por vazio)
    result = re.sub(r"\$\{?CLAUDE_\w+\}?", "", result)
    return result


def resolve_executable(token: str, home_dir: Path) -> tuple[bool, str]:
    """Retorna (ok, evidence)."""
    expanded = expand_path(token, home_dir)
    if not expanded:
        return False, "empty after expansion"

    # Se é caminho absoluto ou relativo
    if "/" in expanded:
        p = Path(expanded)
        if p.exists() and os.access(str(p), os.X_OK):
            return True, str(p)
        if p.exists():
            return False, f"not executable: {p}"
        return False, f"not found: {p}"

    # Busca no PATH
    found = shutil.which(expanded)
    if found:
        return True, found
    return False, f"not in PATH: {expanded}"


def check_command(raw_cmd: str, home_dir: Path) -> tuple[str, str, str]:
    """Retorna (status, name, evidence). status = ok | warn | fail."""
    raw_cmd = raw_cmd.strip()
    if not raw_cmd:
        return "warn", "(empty)", "empty command string"

    try:
        tokens = shlex.split(raw_cmd)
    except ValueError as exc:
        return "fail", raw_cmd[:60], f"shlex parse error: {exc}"

    if not tokens:
        return "warn", "(empty)", "tokenizes to empty"

    first = tokens[0]
    first_name = Path(first).name if "/" in first else first

    if first_name in INTERPRETERS and len(tokens) >= 2:
        # O alvo é o 2º token (o script); precisa ser legível
        target = expand_path(tokens[1], home_dir)
        p = Path(target)
        if p.exists() and p.is_file():
            return "ok", raw_cmd[:60], f"readable: {p}"
        if p.exists():
            return "ok", raw_cmd[:60], f"exists: {p}"
        # Pode ser que o script não exista na máquina de origem (e.g. no CI)
        return "fail", raw_cmd[:60], f"script not found: {target}"
    else:
        # O 1º token deve ser executável
        ok_flag, evidence = resolve_executable(first, home_dir)
        if ok_flag:
            return "ok", raw_cmd[:60], evidence
        return "fail", raw_cmd[:60], evidence


def collect_commands(settings: dict) -> list[str]:
    """Extrai todos os command strings de hooks.*[].hooks[]."""
    cmds: list[str] = []
    for _event, entries in settings.get("hooks", {}).items():
        for entry in entries:
            for h in entry.get("hooks", []):
                cmd = h.get("command")
                if cmd is not None:
                    cmds.append(cmd)
    return cmds


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verifica que todos os commands de hooks são executáveis."
    )
    parser.add_argument("settings", help="Caminho para settings.json")
    parser.add_argument("--home", default=str(Path.home()),
                        help="Diretório home para expansão de $HOME (padrão: ~)")
    parser.add_argument("--json", dest="json_out", action="store_true",
                        help="Saída em JSON")
    args = parser.parse_args()

    settings_path = Path(args.settings)
    if not settings_path.is_file():
        msg = f"FAIL settings-not-found {args.settings}"
        if args.json_out:
            print(json.dumps({"ok": False, "checks": [
                {"name": "settings-file", "status": "fail", "evidence": args.settings}
            ]}))
        else:
            print(msg)
        return 1

    try:
        settings = json.loads(settings_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        msg = f"FAIL settings-invalid-json {exc}"
        if args.json_out:
            print(json.dumps({"ok": False, "checks": [
                {"name": "settings-parse", "status": "fail", "evidence": str(exc)}
            ]}))
        else:
            print(msg)
        return 1

    home_dir = Path(args.home)
    commands = collect_commands(settings)

    checks: list[dict] = []
    fail_count = 0

    for cmd in commands:
        status, name, evidence = check_command(cmd, home_dir)
        checks.append({"name": name, "status": status, "evidence": evidence})
        if status == "fail":
            fail_count += 1
        if not args.json_out:
            print(f"{status} {name} {evidence}")

    if args.json_out:
        print(json.dumps({
            "ok": fail_count == 0,
            "total": len(commands),
            "failed": fail_count,
            "checks": checks,
        }, indent=2))

    return 1 if fail_count > 0 else 0


if __name__ == "__main__":
    sys.exit(main())
