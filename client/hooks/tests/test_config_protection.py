"""test_config_protection.py — a régua de lint/format/teste não muda para fazer o check passar.

Um agente cuja tarefa é "passar no clippy/ruff/pytest" tem um atalho que não corrige nada: afrouxar a
régua (`ignore = [...]`, `fail_under = 0`, `#![allow(dead_code)]`). A REGRA #21 proíbe o atalho; este hook
é o executor dela (D8: a regra dita no anúncio não se aplica sozinha). PreToolUse em
`Edit|Write|MultiEdit|NotebookEdit` devolve `permissionDecision: "ask"` — não bloqueia, **pergunta** — quando o
caminho é config de lint/format/teste, ou quando o texto novo muda uma seção de lint (pyproject.toml, setup.cfg,
tox.ini) ou introduz um `#![allow(...)]` em lib.rs/main.rs.

Precedente do motivo: `config-protection.js` do ECC (análise 17/10/2026). Divergências deliberadas:
criar o arquivo TAMBÉM pergunta (um `ruff.toml` novo num subdiretório vence o do raiz para aquela árvore —
é o jeito mais barato de silenciar uma pasta inteira); e o veredito da edição de pyproject/setup.cfg/tox.ini/
lib.rs/main.rs vem da comparação ANTES×DEPOIS do texto, não de procurar o cabeçalho no trecho editado
(`select = ["E"]` → `select = []` não contém `[tool.ruff]`).

Cada teste roda o hook REAL (subprocesso, payload no stdin). `CONFIG_PROTECTION_HOOK` troca o script —
é por onde o runner de mutantes aplica cada mutação sem tocar o original.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest
from helpers import CLIENT_HOOKS, REPO_ROOT, HookResult

HOOK = Path(os.environ.get("CONFIG_PROTECTION_HOOK") or CLIENT_HOOKS / "config_protection.py")
SETTINGS_SOURCE = Path(os.environ.get("HOOKS_SETTINGS_SOURCE") or REPO_ROOT / "client" / "claude" / "hooks.settings.json")
REASON_CORE = "mudar a régua para passar no check é o atalho que a REGRA #21 proíbe"
Verdict = dict[str, Any] | None


def _run(payload: Any, *, raw: bytes | None = None, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[bytes]:
    data = raw if raw is not None else json.dumps(payload).encode()
    full_env = {**os.environ, "TOURING_CONFIG_PROTECTION_DISABLED": "", **(env or {})}
    return subprocess.run([sys.executable, str(HOOK)], input=data, capture_output=True, env=full_env,
                          timeout=20, check=False)


def _verdict(proc: subprocess.CompletedProcess[bytes]) -> Verdict:
    """The parsed answer; None when the hook stayed silent. A silent hook must also exit 0."""
    assert proc.returncode == 0, proc.stderr.decode(errors="replace")
    out = proc.stdout.decode().strip()
    return json.loads(out) if out else None


def _decision(verdict: Verdict) -> str | None:
    return None if verdict is None else verdict["hookSpecificOutput"]["permissionDecision"]


def _payload(tool: str, tool_input: dict[str, Any], **extra: Any) -> dict[str, Any]:
    return {"hook_event_name": "PreToolUse", "tool_name": tool, "tool_input": tool_input, "session_id": "cfg-sess", **extra}


def _edit(path: Path, old: str, new: str, **kw: Any) -> dict[str, Any]:
    return _payload("Edit", {"file_path": str(path), "old_string": old, "new_string": new, **kw})


def _write(path: Path, content: str) -> dict[str, Any]:
    return _payload("Write", {"file_path": str(path), "content": content})


def _multi(path: Path, edits: list[tuple[str, str]]) -> dict[str, Any]:
    return _payload("MultiEdit", {"file_path": str(path), "edits": [{"old_string": o, "new_string": n} for o, n in edits]})


def _asks(payload: dict[str, Any]) -> Verdict:
    verdict = _verdict(_run(payload))
    assert _decision(verdict) == "ask", f"esperava ask, veio {verdict!r}"
    return verdict


def _silent(payload: dict[str, Any]) -> None:
    proc = _run(payload)
    assert _verdict(proc) is None, proc.stdout.decode()


def _file(tmp_path: Path, rel: str, text: str) -> Path:
    path = tmp_path / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


PYPROJECT = """\
[project]
name = "demo"
dependencies = ["requests"]

[tool.ruff]
line-length = 100

[tool.ruff.lint]
select = ["E", "F"]
ignore = []

[tool.pytest.ini_options]
addopts = "-q"

[tool.coverage.report]
fail_under = 90
"""

# ── o motivo e o formato da resposta ─────────────────────────────────────────────────────────────────


def test_the_verdict_is_a_pretooluse_ask_that_names_the_rule_and_the_file(tmp_path: Path) -> None:
    payload = _edit(_file(tmp_path, "ruff.toml", "x = 1\n"), "x = 1", "x = 2")
    proc = _run(payload)
    verdict = _verdict(proc)
    assert verdict is not None and _decision(verdict) == "ask"
    hso = verdict["hookSpecificOutput"]
    assert hso["hookEventName"] == "PreToolUse" and set(hso) == {"hookEventName", "permissionDecision", "permissionDecisionReason"}
    assert REASON_CORE in hso["permissionDecisionReason"]
    assert "corrija o código, ou confirme se a mudança da régua é intencional" in hso["permissionDecisionReason"]
    assert "ruff.toml" in hso["permissionDecisionReason"]
    assert set(verdict) == {"hookSpecificOutput"}
    assert HookResult(HOOK, payload, proc).schema_ok("PreToolUse")  # the shared hooks contract


def test_the_answer_is_ascii_bytes_whatever_the_locale(tmp_path: Path) -> None:
    proc = _run(_edit(_file(tmp_path, "ruff.toml", "x"), "x", "y"), env={"PYTHONIOENCODING": "ascii", "LC_ALL": "C"})
    proc.stdout.decode("ascii")  # raises on a non-ASCII byte: a verdict must survive any terminal config
    assert _decision(_verdict(proc)) == "ask"


# ── arquivos que SÃO a régua, pelo nome ──────────────────────────────────────────────────────────────

PROTECTED_NAMES = [
    "ruff.toml", ".ruff.toml", "clippy.toml", ".clippy.toml", "rustfmt.toml", ".rustfmt.toml", "deny.toml",
    ".eslintrc", ".eslintrc.json", ".eslintrc.cjs", ".eslintrc.yml", "eslint.config.js", "eslint.config.mjs",
    "eslint.config.ts", ".prettierrc", ".prettierrc.json", ".prettierrc.yaml", "prettier.config.js",
    "biome.json", "biome.jsonc", "pyrightconfig.json", "mypy.ini", ".pre-commit-config.yaml",
    ".cargo/mutants.toml", "pytest.ini", ".coveragerc", ".flake8", ".pylintrc", ".shellcheckrc",
]


@pytest.mark.parametrize("name", PROTECTED_NAMES)
def test_editing_an_existing_config_file_asks(tmp_path: Path, name: str) -> None:
    path = _file(tmp_path, f"proj/{name}", "a = 1\n")
    _asks(_edit(path, "a = 1", "a = 2"))


@pytest.mark.parametrize("name", PROTECTED_NAMES)
def test_overwriting_or_creating_a_config_file_asks(tmp_path: Path, name: str) -> None:
    _asks(_write(tmp_path / "proj" / name, "[x]\n"))  # not on disk: a NEW ruff.toml still overrides the root one
    _asks(_write(_file(tmp_path, f"other/{name}", "old\n"), "new\n"))


def test_multiedit_on_a_config_file_asks(tmp_path: Path) -> None:
    _asks(_multi(_file(tmp_path, "ruff.toml", "a\nb\n"), [("a", "c"), ("b", "d")]))


def test_the_name_match_ignores_case(tmp_path: Path) -> None:
    _asks(_write(tmp_path / "RUFF.TOML", "x\n"))
    _asks(_write(tmp_path / ".ESLintRC.JSON", "x\n"))


@pytest.mark.parametrize("name", [
    "ruff_notes.toml", "myruff.toml", "ruff.toml.md", "ruff.toml.bak", "mutants.toml", "eslint.configuration.md",
    "deny.toml.example", "biome.json5", "clippy.rs", "README.md", "app.py", "Cargo.toml", "package.json",
    "tsconfig.json", "pre-commit-config.yaml",
])
def test_files_that_only_resemble_a_config_name_stay_silent(tmp_path: Path, name: str) -> None:
    _silent(_edit(_file(tmp_path, f"proj/{name}", "a = 1\n"), "a = 1", "a = 2"))
    _silent(_write(tmp_path / "proj2" / name, "a = 1\n"))


def test_mutants_toml_is_protected_only_inside_dot_cargo(tmp_path: Path) -> None:
    _silent(_write(tmp_path / "docs" / "mutants.toml", "x\n"))
    _asks(_write(tmp_path / "proj" / ".cargo" / "mutants.toml", "x\n"))


# ── pyproject.toml: só a seção de lint, medida antes×depois ─────────────────────────────────────────


@pytest.mark.parametrize(("old", "new"), [
    ('select = ["E", "F"]', 'select = ["E"]'),                        # a key inside the section; no header in the strings
    ("ignore = []", 'ignore = ["E501", "F401"]'),
    ("line-length = 100", "line-length = 400"),
    ('addopts = "-q"', 'addopts = "-q --no-cov"'),
    ("fail_under = 90", "fail_under = 0"),
    ("[tool.pytest.ini_options]", "[tool.pytest.ini_options]\nasyncio_mode = 'auto'"),
    ("[tool.coverage.report]\nfail_under = 90\n", ""),                # deleting a lint section
])
def test_editing_inside_a_lint_section_of_pyproject_asks(tmp_path: Path, old: str, new: str) -> None:
    _asks(_edit(_file(tmp_path, "pyproject.toml", PYPROJECT), old, new))


@pytest.mark.parametrize("header", ["[tool.ruff]", "[tool.pytest.ini_options]", "[tool.mypy]", "[tool.pyright]",
                                    "[tool.coverage.run]", "[tool.black]", "[tool.isort]", "[tool.pylint.main]",
                                    "[tool.bandit]"])
def test_adding_a_lint_table_to_pyproject_asks(tmp_path: Path, header: str) -> None:
    path = _file(tmp_path, "pyproject.toml", PYPROJECT)
    _asks(_edit(path, 'name = "demo"', f'name = "demo"\n\n{header}\nx = 1'))


def test_writing_a_pyproject_that_changes_a_lint_section_asks(tmp_path: Path) -> None:
    path = _file(tmp_path, "pyproject.toml", PYPROJECT)
    _asks(_write(path, PYPROJECT.replace('select = ["E", "F"]', "select = []")))


def test_creating_a_pyproject_with_lint_config_asks_but_without_it_stays_silent(tmp_path: Path) -> None:
    _asks(_write(tmp_path / "new1" / "pyproject.toml", PYPROJECT))
    _silent(_write(tmp_path / "new2" / "pyproject.toml", '[project]\nname = "x"\ndependencies = ["a"]\n'))


@pytest.mark.parametrize(("old", "new"), [
    ('dependencies = ["requests"]', 'dependencies = ["requests", "httpx"]'),
    ('name = "demo"', 'name = "demo2"'),
])
def test_editing_outside_the_lint_sections_stays_silent(tmp_path: Path, old: str, new: str) -> None:
    path = _file(tmp_path, "pyproject.toml", PYPROJECT)
    _silent(_edit(path, old, new))
    _silent(_write(path, PYPROJECT.replace(old, new)))


def test_a_comment_or_whitespace_change_inside_a_lint_section_is_not_a_weakening(tmp_path: Path) -> None:
    path = _file(tmp_path, "pyproject.toml", PYPROJECT)
    _silent(_edit(path, "line-length = 100", "line-length = 100  # keep in sync with black\n"))
    _silent(_edit(path, "[tool.ruff.lint]", "[tool.ruff.lint]\n\n"))


def test_a_whole_line_comment_is_not_a_weakening_in_the_text_path_either(tmp_path: Path) -> None:
    """Invalid TOML (and every ini file) is compared as text: a note above a key must not read as a changed ruler."""
    broken = _file(tmp_path, "broken/pyproject.toml", PYPROJECT + "\n[tool.other\nkey = \n")
    _silent(_edit(broken, "line-length = 100", "# keep in sync with black\nline-length = 100"))
    ini = _file(tmp_path, "tox.ini", INI)
    _silent(_edit(ini, "max-line-length = 100", "# a note\nmax-line-length = 100"))
    _silent(_edit(ini, "[flake8]", "[flake8]\n; another note"))


def test_an_edit_whose_old_string_is_not_in_the_file_changes_nothing_and_stays_silent(tmp_path: Path) -> None:
    _silent(_edit(_file(tmp_path, "pyproject.toml", PYPROJECT), "select = ['nope']", "select = []"))


def test_replace_all_counts_every_occurrence(tmp_path: Path) -> None:
    text = "[project]\nname = 'a'\n\n[tool.ruff]\nname = 'a'\n"
    path = _file(tmp_path, "pyproject.toml", text)
    _asks(_edit(path, "name = 'a'", "name = 'b'", replace_all=True))


def test_multiedit_is_judged_on_the_text_after_all_its_edits(tmp_path: Path) -> None:
    path = _file(tmp_path, "pyproject.toml", PYPROJECT)
    _silent(_multi(path, [('name = "demo"', 'name = "d2"'), ('requests', 'httpx')]))
    _asks(_multi(path, [('name = "demo"', 'name = "d2"'), ("fail_under = 90", "fail_under = 10")]))
    # the second edit only exists after the first: the sequence is applied in order
    _asks(_multi(path, [("ignore = []", "ignore = ['X']"), ("ignore = ['X']", "ignore = ['X', 'Y']")]))


def test_a_broken_toml_falls_back_to_the_section_text(tmp_path: Path) -> None:
    broken = PYPROJECT + "\n[tool.other\nkey = \n"
    path = _file(tmp_path, "pyproject.toml", broken)
    _asks(_edit(path, 'select = ["E", "F"]', "select = []"))
    _silent(_edit(path, 'dependencies = ["requests"]', 'dependencies = ["httpx"]'))


def test_a_quoted_table_name_is_still_a_lint_table(tmp_path: Path) -> None:
    text = '[project]\nname = "a"\n\n[tool."ruff".lint]\nselect = ["E"]\n\n[tool.other\n'  # invalid TOML: text path
    _asks(_edit(_file(tmp_path, "pyproject.toml", text), 'select = ["E"]', "select = []"))


# ── setup.cfg / tox.ini ──────────────────────────────────────────────────────────────────────────────

INI = """\
[metadata]
name = demo

[flake8]
max-line-length = 100
extend-ignore = E203

[tool:pytest]
addopts = -q

[coverage:run]
branch = True

[mypy-vendor.*]
ignore_errors = False

[options]
install_requires = requests
"""


@pytest.mark.parametrize("name", ["setup.cfg", "tox.ini"])
@pytest.mark.parametrize(("old", "new"), [
    ("extend-ignore = E203", "extend-ignore = E203,E501,F401"),
    ("addopts = -q", "addopts = -q -p no:cacheprovider"),
    ("branch = True", "branch = False"),
    ("ignore_errors = False", "ignore_errors = True"),
])
def test_editing_a_lint_section_of_an_ini_config_asks(tmp_path: Path, name: str, old: str, new: str) -> None:
    _asks(_edit(_file(tmp_path, name, INI), old, new))


@pytest.mark.parametrize("name", ["setup.cfg", "tox.ini"])
def test_editing_outside_the_lint_sections_of_an_ini_config_stays_silent(tmp_path: Path, name: str) -> None:
    path = _file(tmp_path, name, INI)
    _silent(_edit(path, "name = demo", "name = demo2"))
    _silent(_edit(path, "install_requires = requests", "install_requires = httpx"))
    _silent(_write(path, INI.replace("name = demo", "name = demo3")))


def test_adding_a_lint_section_to_an_ini_config_asks(tmp_path: Path) -> None:
    _asks(_edit(_file(tmp_path, "tox.ini", INI), "[options]", "[pycodestyle]\nignore = E1\n\n[options]"))


# ── #![allow(...)] novo em lib.rs / main.rs ─────────────────────────────────────────────────────────

LIB = "//! crate docs\n#![deny(missing_docs)]\n#![allow(clippy::module_name_repetitions)]\n\npub fn f() {}\n"


@pytest.mark.parametrize("name", ["lib.rs", "main.rs"])
def test_a_new_crate_level_allow_asks(tmp_path: Path, name: str) -> None:
    path = _file(tmp_path, f"src/{name}", LIB)
    _asks(_edit(path, "pub fn f() {}", "#![allow(dead_code)]\npub fn f() {}"))
    _asks(_write(path, LIB + "#![allow(unused)]\n"))


def test_widening_an_existing_allow_asks(tmp_path: Path) -> None:
    path = _file(tmp_path, "src/lib.rs", LIB)
    _asks(_edit(path, "clippy::module_name_repetitions", "clippy::module_name_repetitions, clippy::all"))


def test_a_multiline_allow_is_seen_too(tmp_path: Path) -> None:
    path = _file(tmp_path, "src/lib.rs", LIB)
    _asks(_edit(path, "pub fn f() {}", "#![allow(\n    dead_code,\n    unused\n)]\npub fn f() {}"))


def test_a_new_lib_rs_with_an_allow_asks_and_without_it_stays_silent(tmp_path: Path) -> None:
    _asks(_write(tmp_path / "n1" / "src" / "lib.rs", "#![allow(dead_code)]\n"))
    _silent(_write(tmp_path / "n2" / "src" / "lib.rs", "pub fn f() {}\n"))


@pytest.mark.parametrize(("old", "new"), [
    ("pub fn f() {}", "pub fn f() { println!(); }"),                       # code edit, allow untouched
    ("#![allow(clippy::module_name_repetitions)]\n", ""),                  # removing a loosening
    ("#![deny(missing_docs)]", "#![deny(missing_docs, unsafe_code)]"),     # tightening
    ("//! crate docs", "//! crate docs\n//! mentions #![allow(dead_code)] in prose"),
    ("pub fn f() {}", "#[allow(dead_code)]\npub fn f() {}"),               # an item-level allow is not the crate-level one
])
def test_edits_that_do_not_loosen_the_crate_attributes_stay_silent(tmp_path: Path, old: str, new: str) -> None:
    _silent(_edit(_file(tmp_path, "src/lib.rs", LIB), old, new))


def test_the_same_allow_in_another_rust_file_is_out_of_scope(tmp_path: Path) -> None:
    _silent(_write(tmp_path / "src" / "module.rs", "#![allow(dead_code)]\n"))
    _silent(_write(tmp_path / "src" / "mod.rs", "#![allow(dead_code)]\n"))


# ── negativos: editar código não é mexer na régua ───────────────────────────────────────────────────


@pytest.mark.parametrize("name", ["src/app.py", "src/engine.rs", "web/index.ts", "README.md", "scripts/run.sh", "notes.txt"])
def test_editing_ordinary_files_stays_silent(tmp_path: Path, name: str) -> None:
    path = _file(tmp_path, name, "a\n")
    _silent(_edit(path, "a", "b"))
    _silent(_write(path, "c\n"))
    _silent(_multi(path, [("a", "b")]))


def test_a_notebook_edit_is_judged_by_notebook_path(tmp_path: Path) -> None:
    _silent(_payload("NotebookEdit", {"notebook_path": str(tmp_path / "a.ipynb"), "new_source": "x = 1", "cell_id": "c1"}))
    _asks(_payload("NotebookEdit", {"notebook_path": str(tmp_path / "ruff.toml"), "new_source": "x = 1"}))


def test_a_relative_path_is_resolved_against_the_payload_cwd(tmp_path: Path) -> None:
    _file(tmp_path, "proj/pyproject.toml", PYPROJECT)
    payload = _payload("Edit", {"file_path": "pyproject.toml", "old_string": "fail_under = 90", "new_string": "fail_under = 0"},
                       cwd=str(tmp_path / "proj"))
    _asks(payload)
    _silent({**payload, "cwd": str(tmp_path)})  # a different cwd: the file does not exist, nothing to compare


# ── fail-open: payload ilegível jamais trava o trabalho ─────────────────────────────────────────────


@pytest.mark.parametrize("raw", [b"", b"not json", b"[1, 2]", b'"str"', b"null", b"{}", b'{"tool_name": "Edit"}',
                                 b'{"tool_name": "Edit", "tool_input": null}', b'{"tool_name": "Edit", "tool_input": []}',
                                 b'{"tool_name": "Edit", "tool_input": {"file_path": 7}}',
                                 b'{"tool_name": "Write", "tool_input": {"file_path": "pyproject.toml", "content": 3}}',
                                 b"\xff\xfe\x00bad bytes"])
def test_a_malformed_payload_is_silence_with_exit_zero(raw: bytes) -> None:
    proc = _run(None, raw=raw)
    assert proc.returncode == 0 and not proc.stdout.strip()


def test_a_tool_that_is_not_an_edit_is_silence() -> None:
    _silent(_payload("Bash", {"command": "echo hi > ruff.toml"}))
    _silent(_payload("Read", {"file_path": "/x/ruff.toml"}))


def test_an_unreadable_target_is_silence_not_a_crash(tmp_path: Path) -> None:
    (tmp_path / "pyproject.toml").mkdir()  # a directory where a file is expected
    _silent(_edit(tmp_path / "pyproject.toml", "a", "b"))


def test_the_human_kill_switch_silences_the_hook(tmp_path: Path) -> None:
    payload = _write(tmp_path / "ruff.toml", "x\n")
    assert _verdict(_run(payload, env={"TOURING_CONFIG_PROTECTION_DISABLED": "1"})) is None
    assert _decision(_verdict(_run(payload, env={"TOURING_CONFIG_PROTECTION_DISABLED": "0"}))) == "ask"


# ── registro: o hook existe, está no settings-fonte e não quebra o lint do instalador ───────────────


def _registered_groups() -> list[dict[str, Any]]:
    hooks = json.loads(SETTINGS_SOURCE.read_text(encoding="utf-8"))
    return [g for g in hooks["PreToolUse"]
            if any("config_protection.py" in h.get("command", "") for h in g.get("hooks", []))]


def test_the_hook_is_registered_once_on_the_four_edit_tools() -> None:
    groups = _registered_groups()
    assert len(groups) == 1, groups
    group = groups[0]
    assert set(group["matcher"].split("|")) == {"Edit", "Write", "MultiEdit", "NotebookEdit"}
    (hook,) = group["hooks"]
    assert hook["command"] == "python3 $HOME/.claude/hooks/config_protection.py"
    assert hook["type"] == "command" and 0 < hook["timeout"] <= 5
    assert "if" not in hook and "if" not in group  # one rule per `if`, and the content filter lives in the handler


def test_the_registered_script_exists_in_the_client_mirror() -> None:
    assert (CLIENT_HOOKS / "config_protection.py").is_file()
