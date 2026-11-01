"""Regressao: validate_p2.sh deve ler SOMENTE o primeiro entry de BootOrder.

A versao original usava `grep -oEm1 '[0-9A-Fa-f]{4}'`, mas `-m1` limita linhas de
entrada, nao matches: com `-o` o grep imprimia TODOS os IDs da linha
("0004\n0005\n0000\n..."), o pattern `^Boot<multilinha>` nunca casava e o check
caia na linha `BootCurrent`. Resultado: `efi-boot-order` era impossivel de passar,
mesmo com o Limine em primeiro.
"""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
from pathlib import Path

import pytest

BIN = Path(__file__).resolve().parents[1] / "bin"
VALIDATOR = BIN / "validate_p2.sh"

LIMINE_FIRST = """BootCurrent: 0000
Timeout: 0 seconds
BootOrder: 0000,0004,0005
Boot0000* Limine\tHD(1,GPT,b19ce53e,0x800,0x400000)/\\EFI\\limine\\limine_x64.efi
Boot0004* Pop!_OS 24.04 LTS\tHD(1,GPT,10f3c3bf,0x1000,0x1fefff)/\\EFI\\systemd\\systemd-bootx64.efi
Boot0005* UEFI OS\tHD(1,GPT,10f3c3bf,0x1000,0x1fefff)/\\EFI\\BOOT\\BOOTX64.EFI
"""

POP_FIRST = LIMINE_FIRST.replace("BootOrder: 0000,0004,0005", "BootOrder: 0004,0005,0000")


def _efi_check(efibootmgr_output: str) -> dict:
    """Roda validate_p2.sh com um efibootmgr stub e devolve o check efi-boot-order."""
    with tempfile.TemporaryDirectory(prefix="p2_bootorder_") as tmp:
        stub = Path(tmp) / "efibootmgr"
        stub.write_text("#!/usr/bin/env bash\ncat <<'EOF'\n" + efibootmgr_output + "EOF\n")
        stub.chmod(0o755)
        env = dict(os.environ, PATH=f"{tmp}:{os.environ['PATH']}")
        proc = subprocess.run(
            ["bash", str(VALIDATOR), "--json"],
            capture_output=True, text=True, env=env, check=False,
        )
        data = json.loads(proc.stdout)
    for check in data["checks"]:
        if check["name"] == "efi-boot-order":
            return check
    pytest.fail("check efi-boot-order ausente na saida --json")


def test_limine_first_passes():
    check = _efi_check(LIMINE_FIRST)
    assert check["status"] == "ok", check["evidence"]
    assert "first=0000" in check["evidence"]


def test_pop_first_fails_naming_only_the_first_entry():
    check = _efi_check(POP_FIRST)
    assert check["status"] == "fail"
    # o bug antigo produzia "(0004\n0005\n0000)" e evidencia da linha BootCurrent
    assert "(0004)" in check["evidence"]
    assert "Pop!_OS" in check["evidence"]
    assert "BootCurrent" not in check["evidence"]
