#!/usr/bin/env python3
"""config_protection.py — PreToolUse(Edit|Write|MultiEdit|NotebookEdit): a régua não muda para passar no check.

POR QUE ISTO EXISTE
===================
Um agente cuja tarefa é "passar no clippy / ruff / pytest" tem um atalho que não corrige nada: afrouxar a
régua (`ignore = [...]`, `fail_under = 0`, `#![allow(dead_code)]`, um `ruff.toml` novo numa subpasta). A
REGRA #21 proíbe o atalho; a prosa não o impediu (D8: a regra dita no anúncio não se aplica sozinha), então o
executor pergunta. O precedente do motivo é o `config-protection.js` do ECC (análise 17/10/2026).

VEREDITO
========
`permissionDecision: "ask"` — não bloqueia, **pergunta**: mexer na régua às vezes é intencional (subir uma
versão do ruff, tirar uma regra obsoleta). A razão ensina as duas saídas: corrigir o código, ou confirmar.

O QUE PERGUNTA
==============
1. **Pelo nome** (`PROTECTED_NAMES`, `PROTECTED_PREFIXES`, `.cargo/mutants.toml`): `ruff.toml`, `clippy.toml`,
   `rustfmt.toml`, `deny.toml`, `.eslintrc*`, `eslint.config.*`, `.prettierrc*`, `biome.json[c]`,
   `pyrightconfig.json`, `mypy.ini`, `pytest.ini`, `.coveragerc`, `.flake8`, `.pylintrc`, `.shellcheckrc`,
   `.pre-commit-config.yaml`. Editar, sobrescrever **ou criar** — ao contrário do ECC, criar também pergunta: o
   `ruff.toml` mais próximo vence o do raiz, então um novo, numa pasta, silencia a árvore inteira. Comparação
   sem distinção de caixa (APFS/NTFS gravam no mesmo inode).
2. **Pelo conteúdo**, para arquivos que misturam régua e metadados — `pyproject.toml` (tabelas `[tool.X]` de
   `LINT_TOOLS`), `setup.cfg` e `tox.ini` (seções `INI_LINT_SECTIONS`) — e para `lib.rs`/`main.rs`
   (`#![allow(...)]`). O veredito vem da comparação ANTES×DEPOIS do texto que a ferramenta produziria
   (Edit/MultiEdit simulados sobre o arquivo em disco; Write = o conteúdo novo), nunca de procurar o cabeçalho
   no trecho editado: `select = ["E"]` → `select = []` não contém `[tool.ruff]` e é exatamente o atalho. Em
   pyproject a comparação é semântica (`tomllib`: comentário e espaço não contam); TOML inválido cai na
   comparação por seção do texto. Em Rust, só entra lint NOVO (multiconjunto de nomes dentro de
   `#![allow(...)]`): remover ou apertar passa; `#[allow]` de item e a menção em comentário também.

FAIL-OPEN, NUNCA MUDO
=====================
Payload ilegível, tool desconhecida, caminho ausente, arquivo que não abre: stdout vazio e exit 0 — um bug
aqui não pode travar o trabalho. O motivo da falha sai em stderr (o canal de diagnóstico). O kill switch é
HUMANO: `TOURING_CONFIG_PROTECTION_DISABLED=1` no ambiente do Claude Code (o Bash do modelo não o alcança).

Registro: `client/claude/hooks.settings.json` (PreToolUse, matcher `Edit|Write|MultiEdit|NotebookEdit`) →
`scripts/install_claude_hooks.py --apply`. Testes: `client/hooks/tests/test_config_protection.py`.
"""

from __future__ import annotations

import json
import os
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any

import tomllib

KILL_SWITCH = "TOURING_CONFIG_PROTECTION_DISABLED"
MAX_STDIN_BYTES = 8 * 1024 * 1024
REASON = (
    "mudar a régua para passar no check é o atalho que a REGRA #21 proíbe; corrija o código, "
    "ou confirme se a mudança da régua é intencional"
)

PROTECTED_NAMES = frozenset({
    "ruff.toml", ".ruff.toml", "clippy.toml", ".clippy.toml", "rustfmt.toml", ".rustfmt.toml", "deny.toml",
    "biome.json", "biome.jsonc", "pyrightconfig.json", "mypy.ini", ".pre-commit-config.yaml",
    "pytest.ini", ".coveragerc", ".flake8", ".pylintrc", ".shellcheckrc",
})
#: `.eslintrc`, `.eslintrc.json`, `eslint.config.mjs`, `.prettierrc.yaml`, `prettier.config.js`, …
PROTECTED_PREFIXES = (".eslintrc", "eslint.config.", ".prettierrc", "prettier.config.")
PROTECTED_SUFFIXES = ("/.cargo/mutants.toml",)

#: `[tool.X]` tables of pyproject.toml that hold lint, format, type-check, test or coverage settings.
LINT_TOOLS = ("ruff", "black", "isort", "pylint", "mypy", "pyright", "pytest", "coverage", "bandit")
#: First token of a setup.cfg / tox.ini section name (after a leading `tool:`) that holds the same settings.
INI_LINT_SECTIONS = frozenset({
    "flake8", "pycodestyle", "pep8", "pydocstyle", "pylint", "isort", "mypy", "pytest", "coverage", "bandit",
})
RUST_ROOTS = frozenset({"lib.rs", "main.rs"})
EDIT_TOOLS = frozenset({"Edit", "Write", "MultiEdit", "NotebookEdit"})

_TOML_NAME = r"""(?:[A-Za-z0-9_-]+|"[^"]*"|'[^']*')"""
TOML_HEADER = re.compile(rf"^\s*\[\[?\s*({_TOML_NAME}(?:\s*\.\s*{_TOML_NAME})*)\s*\]\]?\s*(?:#.*)?$")
INI_HEADER = re.compile(r"^\s*\[([^\]]+)\]")
INNER_ALLOW = re.compile(r"^[ \t]*#!\[\s*allow\s*\((.*?)\)\s*\]", re.MULTILINE | re.DOTALL)


# ── what the tool would leave on disk ─────────────────────────────────────────────────────────────────


def _read(path: Path) -> str | None:
    """The file's text, "" when it does not exist yet, None when it exists but cannot be read."""
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return ""
    except OSError:
        return None


def _apply_edit(text: str, old: str, new: str, replace_all: bool) -> str:
    if old == "":
        return new if text == "" else text  # Edit with an empty old_string creates the file
    if old not in text:
        return text  # the tool would reject it; nothing changes
    return text.replace(old, new) if replace_all else text.replace(old, new, 1)


def _multi_after(tool_input: dict[str, Any], before: str) -> str | None:
    edits = tool_input.get("edits")
    if not isinstance(edits, list):
        return None
    text = before
    for edit in edits:
        pair = _edit_pair(edit if isinstance(edit, dict) else {})
        if pair is None:
            return None
        text = _apply_edit(text, *pair, bool(edit.get("replace_all")))
    return text


def _edit_pair(fields: dict[str, Any]) -> tuple[str, str] | None:
    old, new = fields.get("old_string"), fields.get("new_string")
    return (old, new) if isinstance(old, str) and isinstance(new, str) else None


def _after(tool: str, tool_input: dict[str, Any], before: str) -> str | None:
    """The text the tool would write; None when the payload carries no recognisable change."""
    if tool == "Write":
        content = tool_input.get("content")
        return content if isinstance(content, str) else None
    if tool == "MultiEdit":
        return _multi_after(tool_input, before)
    pair = _edit_pair(tool_input)
    return None if pair is None else _apply_edit(before, *pair, bool(tool_input.get("replace_all")))


# ── lint views: what the "ruler" of a mixed file is ───────────────────────────────────────────────────


def _toml_semantic(text: str) -> Any:
    """`{tool: settings}` for every lint tool present; raises on invalid TOML."""
    tool = tomllib.loads(text).get("tool", {})
    return {name: tool[name] for name in LINT_TOOLS if isinstance(tool, dict) and name in tool}


def _clean_lines(lines: list[str], comment_marks: tuple[str, ...]) -> list[str]:
    """Trailing space, blank lines and whole-line comments do not change a ruler."""
    kept = (line.rstrip() for line in lines)
    return [line for line in kept if line.strip() and not line.lstrip().startswith(comment_marks)]


def _sections(text: str, header: re.Pattern[str], is_lint: Any, comment_marks: tuple[str, ...]) -> dict[str, list[str]]:
    sections: dict[str, list[str]] = {}
    current: str | None = None
    for line in text.splitlines():
        match = header.match(line)
        if match:
            name = re.sub(r"[\s\"']", "", match.group(1)) if header is TOML_HEADER else match.group(1).strip()
            current = name if is_lint(name) else None
            if current is not None:
                sections.setdefault(current, []).append("")  # an empty section still exists
            continue
        if current is not None:
            sections[current].append(line)
    return {name: _clean_lines(body, comment_marks) for name, body in sections.items()}


def _is_toml_lint(name: str) -> bool:
    return any(name == f"tool.{tool}" or name.startswith(f"tool.{tool}.") for tool in LINT_TOOLS)


def _is_ini_lint(name: str) -> bool:
    base = name.strip().lower().removeprefix("tool:")
    return re.split(r"[:.\-]", base, maxsplit=1)[0] in INI_LINT_SECTIONS


def _pyproject_changed(before: str, after: str) -> bool:
    try:
        return _toml_semantic(before) != _toml_semantic(after)
    except (tomllib.TOMLDecodeError, TypeError, ValueError):
        pass
    textual = (TOML_HEADER, _is_toml_lint, ("#",))
    return _sections(before, *textual) != _sections(after, *textual)


def _ini_changed(before: str, after: str) -> bool:
    textual = (INI_HEADER, _is_ini_lint, ("#", ";"))
    return _sections(before, *textual) != _sections(after, *textual)


def _allowed_lints(text: str) -> Counter[str]:
    names: Counter[str] = Counter()
    for body in INNER_ALLOW.findall(text):
        names.update(item.strip() for item in body.split(",") if item.strip())
    return names


def _rust_loosened(before: str, after: str) -> list[str]:
    return sorted((_allowed_lints(after) - _allowed_lints(before)).elements())


# ── the verdict ───────────────────────────────────────────────────────────────────────────────────────


def _is_protected_name(path: str) -> bool:
    lower = path.replace("\\", "/").lower()
    base = lower.rsplit("/", 1)[-1]
    return (base in PROTECTED_NAMES or base.startswith(PROTECTED_PREFIXES)
            or any(lower.endswith(suffix) for suffix in PROTECTED_SUFFIXES))


def _target(tool_input: dict[str, Any], cwd: Any) -> tuple[str, Path] | None:
    raw = tool_input.get("file_path") or tool_input.get("notebook_path")
    if not isinstance(raw, str) or not raw:
        return None
    path = Path(raw)
    if not path.is_absolute() and isinstance(cwd, str) and cwd:
        path = Path(cwd) / path
    return raw, path


def _content_verdict(kind: str, tool: str, tool_input: dict[str, Any], path: Path) -> str | None:
    """The ask-reason for a file that mixes ruler and metadata, from the text before × after."""
    before = _read(path)
    after = _after(tool, tool_input, before) if before is not None else None
    if before is None or after is None:
        return None  # unreadable target or an unrecognisable change: the tool itself will reject or fail
    name = path.name
    if kind == "toml" and _pyproject_changed(before, after):
        return f"esta mudança altera uma tabela `[tool.*]` de lint/teste de `{name}`"
    if kind == "ini" and _ini_changed(before, after):
        return f"esta mudança altera uma seção de lint/teste de `{name}`"
    added = _rust_loosened(before, after) if kind == "rust" else []
    return f"esta mudança introduz `#![allow({', '.join(added)})]` em `{name}`" if added else None


def _kind(name: str) -> str | None:
    if name == "pyproject.toml":
        return "toml"
    if name in {"setup.cfg", "tox.ini"}:
        return "ini"
    return "rust" if name in RUST_ROOTS else None


def judge(payload: Any) -> str | None:
    """The ask-reason for this tool call, or None when it leaves the ruler alone."""
    if not isinstance(payload, dict) or not isinstance(payload.get("tool_input"), dict):
        return None
    tool, tool_input = payload.get("tool_name"), payload["tool_input"]
    if tool not in EDIT_TOOLS:
        return None  # the matcher already narrows this; a Read of ruff.toml must never ask
    target = _target(tool_input, payload.get("cwd"))
    if target is None:
        return None
    raw, path = target
    if _is_protected_name(raw):
        return f"`{path.name}` é config de lint/format/teste"
    kind = _kind(path.name)
    if kind is None or tool == "NotebookEdit":
        return None
    return _content_verdict(kind, tool, tool_input, path)


def main() -> int:
    if os.environ.get(KILL_SWITCH) == "1":
        return 0
    try:
        payload = json.loads(sys.stdin.buffer.read(MAX_STDIN_BYTES).decode("utf-8", errors="replace"))
        why = judge(payload)
    except Exception as exc:  # noqa: BLE001 — fail-open by contract: a bug here must not stall the work
        sys.stderr.write(f"config_protection: payload não avaliado ({type(exc).__name__}: {exc})\n")
        return 0
    if why is None:
        return 0
    verdict = {"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "ask",
        "permissionDecisionReason": f"{why}: {REASON}",
    }}
    sys.stdout.write(json.dumps(verdict) + "\n")  # ASCII-escaped: no terminal config can corrupt the verdict
    return 0


if __name__ == "__main__":
    sys.exit(main())
