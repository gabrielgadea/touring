"""test_settings_stage.py — Testes de settings_stage.py e hooks_runnable.py."""
import json
import subprocess
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent
OMARCHY = REPO_ROOT / "client" / "omarchy"
STAGE_PY = OMARCHY / "bin" / "settings_stage.py"
RUNNABLE_PY = OMARCHY / "bin" / "hooks_runnable.py"
REAL_SETTINGS = Path.home() / ".claude" / "settings.json"

DROP_PATTERN = (
    r"touring-hook"
    r"|touring-quality-block-all"
    r"|touring-process-guard"
    r"|touring pre-task-scout"
    r"|gateway-gemini"
    r"|qgis-mcp"
    r"|gitnexus"
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
def _run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    kwargs.setdefault("capture_output", True)
    kwargs.setdefault("text", True)
    return subprocess.run(cmd, **kwargs)


def _stage(settings_path: Path, stage: str, out: Path,
           home: Path | None = None) -> subprocess.CompletedProcess:
    cmd = [
        sys.executable, str(STAGE_PY),
        "--in", str(settings_path),
        "--stage", stage,
        "--out", str(out),
    ]
    if home:
        cmd += ["--home", str(home)]
    return _run(cmd)


def _make_settings(hooks: dict, extra: dict | None = None) -> dict:
    """Monta um settings.json minimal para testes."""
    base: dict = {"model": "claude-fable-5", "hooks": hooks}
    if extra:
        base.update(extra)
    return base


def _write_settings(path: Path, data: dict) -> None:
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------
@pytest.fixture()
def settings_with_touring(tmp_path: Path) -> Path:
    """settings.json com hooks Touring e não-Touring misturados."""
    data = _make_settings(
        hooks={
            "SessionStart": [
                {
                    "hooks": [
                        {"type": "command", "command": "$HOME/.claude/hooks/touring-hook session-start"},
                        {"type": "command", "command": "$HOME/.claude/hooks/session_env_setup.sh"},
                    ]
                }
            ],
            "PreToolUse": [
                {
                    "matcher": "Read",
                    "hooks": [
                        {"type": "command", "command": "$HOME/.claude/hooks/touring-hook pre-read"},
                        {"type": "command", "command": "$HOME/.claude/hooks/block_git.sh"},
                    ]
                }
            ],
            "Stop": [
                {
                    "hooks": [
                        {"type": "command", "command": "$HOME/.gateway-gemini-antigravity/hook.sh"},
                    ]
                }
            ],
        },
        extra={
            "enabledPlugins": {"context7@claude-plugins-official": True},
            "statusLine": "TACO",
        },
    )
    p = tmp_path / "settings.json"
    _write_settings(p, data)
    return p


@pytest.fixture()
def fake_home_for_runnable(tmp_path: Path) -> Path:
    """HOME falso com scripts executáveis e um sem +x para teste de falha."""
    h = tmp_path / "home"
    hooks = h / ".claude" / "hooks"
    hooks.mkdir(parents=True)

    # Script executável
    good = hooks / "good-hook.sh"
    good.write_text("#!/bin/bash\necho ok")
    good.chmod(0o755)

    return h


# ---------------------------------------------------------------------------
# test_stage_a_has_no_touring_hook
# ---------------------------------------------------------------------------
def test_stage_a_has_no_touring_hook(settings_with_touring: Path, tmp_path: Path) -> None:
    """Stage A não deve conter nenhum command que case com touring-hook ou gateway-gemini."""
    import re
    out = tmp_path / "stageA.json"
    result = _stage(settings_with_touring, "A", out)
    assert result.returncode == 0, result.stderr

    data = json.loads(out.read_text())
    rx = re.compile(DROP_PATTERN)

    for _event, entries in data.get("hooks", {}).items():
        for entry in entries:
            for h in entry.get("hooks", []):
                cmd = h.get("command", "")
                assert not rx.search(cmd), (
                    f"Comando proibido sobrou em stage A: {cmd!r}"
                )


# ---------------------------------------------------------------------------
# test_stage_b_identical_to_input
# ---------------------------------------------------------------------------
def test_stage_b_identical_to_input(settings_with_touring: Path, tmp_path: Path) -> None:
    """Stage B deve ser idêntico ao input (conteúdo de hooks e outras chaves)."""
    out = tmp_path / "stageB.json"
    result = _stage(settings_with_touring, "B", out)
    assert result.returncode == 0, result.stderr

    original = json.loads(settings_with_touring.read_text())
    generated = json.loads(out.read_text())
    assert original == generated, "Stage B diverge do input"


# ---------------------------------------------------------------------------
# test_stage_a_keeps_non_hook_keys
# ---------------------------------------------------------------------------
def test_stage_a_keeps_non_hook_keys(settings_with_touring: Path, tmp_path: Path) -> None:
    """Stage A preserva chaves não relacionadas a hooks (model, statusLine, etc.)."""
    out = tmp_path / "stageA.json"
    result = _stage(settings_with_touring, "A", out)
    assert result.returncode == 0, result.stderr

    data = json.loads(out.read_text())
    assert data.get("model") == "claude-fable-5", "chave 'model' perdida"
    assert data.get("statusLine") == "TACO", "chave 'statusLine' perdida"
    # enabledPlugins ainda presente (pode ter sido filtrado, mas a chave existe)
    assert "enabledPlugins" in data, "chave 'enabledPlugins' perdida"


# ---------------------------------------------------------------------------
# test_hooks_runnable_fails_on_missing_x_bit
# ---------------------------------------------------------------------------
def test_hooks_runnable_fails_on_missing_x_bit(
    fake_home_for_runnable: Path, tmp_path: Path
) -> None:
    """hooks_runnable.py deve falhar (exit 1) quando um script não tem +x."""
    h = fake_home_for_runnable
    hooks = h / ".claude" / "hooks"

    # Script sem +x
    bad = hooks / "no-exec.sh"
    bad.write_text("#!/bin/bash\necho nope")
    bad.chmod(0o644)  # sem execução

    settings_data = _make_settings(
        hooks={
            "SessionStart": [
                {
                    "hooks": [
                        {"type": "command", "command": "$HOME/.claude/hooks/good-hook.sh"},
                        {"type": "command", "command": "$HOME/.claude/hooks/no-exec.sh"},
                    ]
                }
            ]
        }
    )
    settings_path = tmp_path / "settings.json"
    _write_settings(settings_path, settings_data)

    result = _run([
        sys.executable, str(RUNNABLE_PY),
        str(settings_path),
        "--home", str(h),
    ])
    assert result.returncode != 0, (
        f"Esperava exit != 0 por script sem +x, mas foi 0.\n{result.stdout}"
    )
    assert "fail" in result.stdout.lower(), (
        f"Esperava 'fail' no output:\n{result.stdout}"
    )


# ---------------------------------------------------------------------------
# test_hooks_runnable_passes_on_real_settings
# ---------------------------------------------------------------------------
@pytest.mark.skipif(
    not REAL_SETTINGS.exists(),
    reason="~/.claude/settings.json não encontrado",
)
def test_hooks_runnable_passes_on_real_settings() -> None:
    """hooks_runnable.py deve passar (exit 0) com o settings.json real do sistema.

    Usa $HOME real; só verifica que os comandos REGISTRADOS são executáveis.
    Falhas legítimas de ambiente (ferramentas ausentes) são propagadas como
    falha de teste — o objetivo é manter o inventário de hooks consistente.
    """
    result = _run([
        sys.executable, str(RUNNABLE_PY),
        str(REAL_SETTINGS),
        "--home", str(Path.home()),
        "--json",
    ])
    data = json.loads(result.stdout)
    failed_checks = [c for c in data.get("checks", []) if c["status"] == "fail"]
    assert result.returncode == 0, (
        "hooks_runnable.py falhou no settings.json real. "
        "Checks falhos:\n" +
        "\n".join(f"  {c['name']}: {c['evidence']}" for c in failed_checks)
    )
