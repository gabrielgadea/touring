"""S-0.9 — the cidata files must be what the ISO wizard writes (archinstall
format), not an invented schema.

P0V's first catch (23/08/2026): the original hand-written template
(`{"disk": ..., "keyboard": ...}`) was NOT the wizard's format — the manual says
the files "are exactly what the installer's own wizard writes", and the wizard
(`omacom-io/omarchy-iso`, `configs/airootfs/root/configurator`) writes the full
archinstall JSON with computed partition offsets. `cidata_gen.py` is a port of
that heredoc; these tests pin the port to the wizard's own numbers.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

CLIENT_OMARCHY = Path(__file__).resolve().parents[1]
GEN = CLIENT_OMARCHY / "bin" / "cidata_gen.py"
BUILD_SCRIPT = CLIENT_OMARCHY / "bin" / "cidata_build.sh"
TEMPLATE = CLIENT_OMARCHY / "cidata" / "user_configuration.template.json"

MIB = 1024 * 1024
GIB = MIB * 1024

sys.path.insert(0, str(CLIENT_OMARCHY / "bin"))
import cidata_gen  # noqa: E402


# ── the generator (unit) ─────────────────────────────────────────────────────


def test_partition_math_matches_the_wizard() -> None:
    """boot: start 1 MiB, size 2 GiB; main: rest minus the 1 MiB GPT reserve,
    on a disk size rounded down to a MiB — verbatim from the configurator."""
    size = 40 * GIB
    lay = cidata_gen.partition_layout(size)
    assert lay["boot_start"] == MIB
    assert lay["boot_size"] == 2 * GIB
    assert lay["main_start"] == 2 * GIB + MIB
    assert lay["main_size"] == size - lay["main_start"] - MIB


def test_disk_too_small_is_refused() -> None:
    try:
        cidata_gen.partition_layout(2 * GIB)
    except ValueError as e:
        assert "too small" in str(e)
    else:  # pragma: no cover
        raise AssertionError("a 2 GiB disk cannot hold the layout")


def test_user_configuration_shape() -> None:
    conf = cidata_gen.user_configuration(
        disk="/dev/vda", disk_size=40 * GIB, hostname="storm",
        timezone="America/Sao_Paulo", keyboard="br-abnt2", kernel="linux",
        encrypt=False, luks_secret=None,
    )
    assert conf["bootloader_config"]["bootloader"] == "Limine"
    dm = conf["disk_config"]["device_modifications"][0]
    assert dm["device"] == "/dev/vda" and dm["wipe"] is True
    boot, main = dm["partitions"]
    assert boot["fs_type"] == "fat32" and boot["mountpoint"] == "/boot" and boot["flags"] == ["boot", "esp"]
    assert main["fs_type"] == "btrfs"
    assert [b["name"] for b in main["btrfs"]] == ["@", "@home", "@log", "@pkg"]
    assert conf["locale_config"]["kb_layout"] == "br-abnt2"
    assert conf["omarchy_install"]["mode"] == "full_disk"
    assert "disk_encryption" not in conf["disk_config"]


def test_encrypted_configuration_carries_the_luks_block() -> None:
    conf = cidata_gen.user_configuration(
        disk="/dev/vda", disk_size=40 * GIB, hostname="storm",
        timezone="America/Sao_Paulo", keyboard="br-abnt2", kernel="linux",
        encrypt=True, luks_secret="s3cret",
    )
    enc = conf["disk_config"]["disk_encryption"]
    assert enc["encryption_type"] == "luks"
    assert enc["partitions"] == [cidata_gen.MAIN_OBJ_ID]
    assert enc[cidata_gen.KEY_LUKS_SECRET] == "s3cret"


def test_credentials_shape_matches_the_wizard() -> None:
    creds = cidata_gen.user_credentials(username="gabrielgadea", hashed="$6$x$y", encrypt=False, luks_secret=None)
    assert creds[cidata_gen.KEY_ROOT_HASH] == "$6$x$y"
    (user,) = creds["users"]
    assert user["username"] == "gabrielgadea" and user["sudo"] is True and user[cidata_gen.KEY_USER_HASH] == "$6$x$y"


def _run_gen(tmp_path: Path, *extra: str, env_hash: str | None = "$6$t$h") -> subprocess.CompletedProcess:
    env = {**os.environ}
    env.pop("CIDATA_PASSWORD_HASH", None)
    if env_hash is not None:
        env["CIDATA_PASSWORD_HASH"] = env_hash
    return subprocess.run(
        [sys.executable, str(GEN), "--disk", "/dev/vda", "--disk-size-bytes", str(40 * GIB),
         "--out-dir", str(tmp_path / "cidata"), *extra],
        capture_output=True, text=True, env=env,
    )


def test_gen_cli_writes_the_wizard_file_set(tmp_path: Path) -> None:
    result = _run_gen(tmp_path)
    assert result.returncode == 0, result.stderr
    names = sorted(p.name for p in (tmp_path / "cidata").iterdir())
    assert names == [
        "user_configuration.json", "user_credentials.json", "user_email_address.txt",
        "user_encrypt_installation.txt", "user_full_name.txt",
    ]
    assert (tmp_path / "cidata" / "user_encrypt_installation.txt").read_text().strip() == "false"
    for p in (tmp_path / "cidata").iterdir():
        assert (p.stat().st_mode & 0o777) == 0o600


def test_gen_cli_refuses_to_run_without_the_hash(tmp_path: Path) -> None:
    result = _run_gen(tmp_path, env_hash=None)
    assert result.returncode == 2
    assert "CIDATA_PASSWORD_HASH" in result.stderr


def test_gen_defer_provisioning_ships_no_credentials(tmp_path: Path) -> None:
    result = _run_gen(tmp_path, "--defer-provisioning", env_hash=None)
    assert result.returncode == 0, result.stderr
    creds = json.loads((tmp_path / "cidata" / "user_credentials.json").read_text())
    assert creds == {"users": []}
    assert (tmp_path / "cidata" / "defer-provisioning").exists()


# ── the template (documentation fixture) ─────────────────────────────────────


def _template_json() -> dict:
    text = TEMPLATE.read_text(encoding="utf-8")
    return json.loads(re.sub(r"^\s*//[^\n]*\n", "", text, flags=re.M))


def test_template_is_the_archinstall_format() -> None:
    data = _template_json()
    dm = data["disk_config"]["device_modifications"][0]
    assert dm["device"].startswith("/dev/disk/by-id/nvme-SM2P41C8-001TC5_KP102L1HDJDW")
    assert data["bootloader_config"]["bootloader"] == "Limine"
    assert data["locale_config"]["kb_layout"] == "br-abnt2"
    assert data["hostname"] == "storm" and data["timezone"] == "America/Sao_Paulo"


def test_template_carries_no_secret() -> None:
    data = _template_json()
    assert "disk_encryption" not in data["disk_config"], "the template must never carry the LUKS block"
    text = TEMPLATE.read_text(encoding="utf-8")
    assert "$6$" not in text, "no real password hash in the template"


# ── the build script (integration, fake ISO tool) ────────────────────────────


def _fake_bin(tmp_path: Path) -> Path:
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir(exist_ok=True)
    for tool in ("genisoimage", "mkisofs", "xorrisofs"):
        (fake_bin / tool).write_text(
            "#!/usr/bin/env bash\nset -euo pipefail\n"
            'out=""; while (( $# )); do case "$1" in -output) out="$2"; shift 2;; *) shift;; esac; done\n'
            'touch "$out"\n',
            encoding="utf-8",
        )
        (fake_bin / tool).chmod(0o755)
    return fake_bin


def _run_build(tmp_path: Path, *extra: str, stdin: str = "\nhunter2\n") -> subprocess.CompletedProcess:
    env = {**os.environ, "PATH": f"{_fake_bin(tmp_path)}:{os.environ.get('PATH', '')}"}
    return subprocess.run(
        ["bash", str(BUILD_SCRIPT), "--disk", "/dev/vda", "--disk-size-bytes", str(40 * GIB),
         "--out", str(tmp_path / "cidata.iso"), "--allow-disk", *extra],
        input=stdin, capture_output=True, text=True, env=env,
    )


def test_build_no_encrypt_produces_the_wizard_files(tmp_path: Path) -> None:
    result = _run_build(tmp_path, "--no-encrypt")
    assert result.returncode == 0, result.stdout + result.stderr
    assert (tmp_path / "cidata.iso").exists()
    assert "user_configuration.json" in result.stdout and "user_credentials.json" in result.stdout


def test_build_requires_disk_and_size() -> None:
    result = subprocess.run(["bash", str(BUILD_SCRIPT), "--no-encrypt"], input="\nx\n", capture_output=True, text=True)
    assert result.returncode != 0 and "--disk" in result.stderr


def _fs_type(path: Path) -> str:
    """Filesystem de `path` como o cidata_build.sh o le (`stat -f -c %T`)."""
    out = subprocess.run(["stat", "-f", "-c", "%T", str(path)],
                         capture_output=True, text=True, check=False)
    return out.stdout.strip() or "unknown"


def test_refuses_iso_with_secrets_outside_tmpfs(tmp_path: Path) -> None:
    """A recusa so' pode ser exercitada com o destino em armazenamento persistente.

    NAO assumir que o `tmp_path` do pytest serve: em Omarchy/Arch **/tmp e'
    tmpfs** (medido 2026-08-23), entao o destino cai em tmpfs, o script —
    corretamente — nao recusa, e o teste acusava um defeito inexistente. No
    Pop!_OS /tmp era disco, e por isso passava la'. Ancorar num diretorio
    comprovadamente nao-tmpfs em vez de confiar no default do pytest.
    """
    out_dir = tmp_path
    if _fs_type(out_dir) == "tmpfs":
        # $HOME e' disco persistente nas duas maquinas do programa.
        out_dir = Path(tempfile.mkdtemp(prefix="cidata_disk_", dir=Path.home()))
    if _fs_type(out_dir) == "tmpfs":
        pytest.skip("nenhum destino nao-tmpfs disponivel para exercitar a recusa")

    out_iso = out_dir / "cidata.iso"
    try:
        env = {**os.environ, "PATH": f"{_fake_bin(tmp_path)}:{os.environ.get('PATH', '')}"}
        result = subprocess.run(
            ["bash", str(BUILD_SCRIPT), "--disk", "/dev/vda", "--disk-size-bytes", str(40 * GIB),
             "--out", str(out_iso), "--no-encrypt"],
            input="\nhunter2\n", capture_output=True, text=True, env=env,
        )
        assert result.returncode != 0
        assert "out-not-tmpfs" in result.stderr
        assert not out_iso.exists()
    finally:
        if out_dir is not tmp_path:
            shutil.rmtree(out_dir, ignore_errors=True)


def test_password_never_appears_in_argv(tmp_path: Path) -> None:
    """`openssl passwd -6 <pw>` put the password in /proc/<pid>/cmdline (P1).
    A fake `openssl` records its argv; the password must only arrive on stdin."""
    fake_bin = _fake_bin(tmp_path)
    argv_log = tmp_path / "openssl-argv.log"
    (fake_bin / "openssl").write_text(
        "#!/usr/bin/env bash\n"
        f"printf '%s ' \"$@\" >> '{argv_log}'\n"
        "cat >/dev/null\nprintf '$6$fake$hash\\n'\n",
        encoding="utf-8",
    )
    (fake_bin / "openssl").chmod(0o755)
    result = _run_build(tmp_path, "--no-encrypt")
    assert result.returncode == 0, result.stdout + result.stderr
    logged = argv_log.read_text(encoding="utf-8") if argv_log.exists() else ""
    assert "hunter2" not in logged, f"password leaked into openssl argv: {logged}"
    assert "-stdin" in logged
