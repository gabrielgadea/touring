"""Contract tests for validate_pN.sh and validate_all.sh.

Verifica:
1. --help exit 0
2. --json produz JSON com chaves phase, ok, checks
3. FORCE_FAIL=1 → exit nonzero e check "forced" com status=fail

Roda com PATH mínimo (bash + python3) para que quase tudo vire FAIL missing-tool.
O contrato é o que se testa, não o ambiente.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

import pytest

BIN = Path(__file__).resolve().parents[1] / "bin"

VALIDATORS = [BIN / f"validate_p{n}.sh" for n in range(9)] + [BIN / "validate_all.sh"]


@pytest.fixture(scope="module")
def minimal_env():
    """PATH com apenas bash e python3 (binário real) para forçar FAIL missing-tool.

    Usa o binário ELF real do python3 (não wrapper scripts que dependem de taskset
    ou outros utilitários externos ausentes do PATH mínimo).
    """
    tmpdir = Path(tempfile.mkdtemp(prefix="validate_contract_"))
    # bash: usar shutil.which (geralmente /usr/bin/bash, binário real)
    src_bash = shutil.which("bash")
    if src_bash:
        (tmpdir / "bash").symlink_to(src_bash)
    # python3: preferir binário ELF real em vez de wrapper que precisa de taskset
    for candidate in (
        "/usr/bin/python3.12",
        "/usr/bin/python3.13",
        "/usr/bin/python3.11",
        "/usr/bin/python3.10",
        "/usr/bin/python3",
    ):
        if Path(candidate).is_file() and not Path(candidate).is_symlink():
            (tmpdir / "python3").symlink_to(candidate)
            break
    else:
        src_py = shutil.which("python3")
        if src_py:
            (tmpdir / "python3").symlink_to(src_py)
    env = {
        "HOME": os.environ.get("HOME", "/tmp"),
        "PATH": str(tmpdir),
        "TMPDIR": "/tmp",
        "TERM": "dumb",
    }
    yield env
    shutil.rmtree(tmpdir, ignore_errors=True)


@pytest.mark.parametrize("script", VALIDATORS, ids=[p.name for p in VALIDATORS])
def test_script_exists_and_executable(script: Path) -> None:
    assert script.is_file(), f"{script.name} not found in bin/"
    assert os.access(script, os.X_OK), f"{script.name} not executable"


@pytest.mark.parametrize("script", VALIDATORS, ids=[p.name for p in VALIDATORS])
def test_help_exits_zero(script: Path) -> None:
    result = subprocess.run(
        ["bash", str(script), "--help"],
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert result.returncode == 0, (
        f"--help returned {result.returncode}:\nstdout={result.stdout[:200]}\nstderr={result.stderr[:200]}"
    )


@pytest.mark.parametrize("script", VALIDATORS, ids=[p.name for p in VALIDATORS])
def test_json_output_valid_structure(script: Path, minimal_env: dict) -> None:
    result = subprocess.run(
        ["bash", str(script), "--json"],
        env=minimal_env,
        capture_output=True,
        text=True,
        timeout=60,
    )
    stdout = result.stdout.strip()
    assert stdout, (
        f"No stdout from {script.name}. stderr: {result.stderr[:300]}"
    )
    try:
        data = json.loads(stdout)
    except json.JSONDecodeError as exc:
        pytest.fail(f"{script.name} --json produced invalid JSON: {exc}\nOutput: {stdout[:300]}")

    assert "phase" in data, f"'phase' key missing in {script.name} JSON"
    assert "ok" in data, f"'ok' key missing in {script.name} JSON"
    assert "checks" in data, f"'checks' key missing in {script.name} JSON"
    assert isinstance(data["ok"], bool), f"'ok' must be bool in {script.name}"
    assert isinstance(data["checks"], list), f"'checks' must be list in {script.name}"

    for check in data["checks"]:
        assert "name" in check, f"check missing 'name' in {script.name}: {check}"
        assert "status" in check, f"check missing 'status' in {script.name}: {check}"
        assert check["status"] in ("ok", "fail", "warn"), (
            f"invalid status '{check['status']}' in {script.name}"
        )


@pytest.mark.parametrize("script", VALIDATORS, ids=[p.name for p in VALIDATORS])
def test_force_fail_exits_nonzero(script: Path, minimal_env: dict) -> None:
    env = {**minimal_env, "FORCE_FAIL": "1"}
    result = subprocess.run(
        ["bash", str(script), "--json"],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
    )
    assert result.returncode != 0, (
        f"FORCE_FAIL=1 should cause nonzero exit in {script.name}"
    )
    stdout = result.stdout.strip()
    assert stdout, f"No stdout with FORCE_FAIL=1 from {script.name}"
    try:
        data = json.loads(stdout)
    except json.JSONDecodeError as exc:
        pytest.fail(f"{script.name} FORCE_FAIL JSON invalid: {exc}\nOutput: {stdout[:300]}")

    forced_checks = [c for c in data.get("checks", []) if c.get("name") == "forced"]
    assert forced_checks, (
        f"No 'forced' check in {script.name} FORCE_FAIL output. "
        f"checks: {data.get('checks', [])}"
    )
    assert forced_checks[0]["status"] == "fail", (
        f"'forced' check should have status=fail in {script.name}, "
        f"got: {forced_checks[0]['status']}"
    )
