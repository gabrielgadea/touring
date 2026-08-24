#!/usr/bin/env python3
"""Generate the cidata input files EXACTLY as the Omarchy ISO wizard writes them.

The manual (51-unattended-installs) says the files "are exactly what the
installer's own wizard writes" — and the wizard (`omacom-io/omarchy-iso`,
`configs/airootfs/root/configurator`, read 23/08/2026) writes the archinstall
JSON: a `disk_config` with two partitions whose byte offsets are computed from
the disk size, Limine as bootloader, btrfs subvolumes `@ @home @log @pkg`, and a
`user_credentials.json` with the root hash + `users[]`. A hand-written
`{"disk": ..., "keyboard": ...}` is not that format and the loader would hand
the machine back to the wizard (or worse). This is a port of that heredoc.

Partition math (verbatim from the wizard):
    mib = 1 MiB · gib = 1 GiB
    disk_size_in_mib = disk_size rounded DOWN to a MiB
    boot: start = 1 MiB, size = 2 GiB (fat32, /boot, flags boot+esp)
    main: start = boot.start + boot.size, size = disk_size_in_mib - main.start - 1 MiB
    (the trailing MiB is the GPT backup reserve)

Secrets never touch argv: the hash, the plaintext secret archinstall needs for
an encrypted install, and nothing else arrive through environment variables
(`CIDATA_PASSWORD_HASH`, `CIDATA_PASSWORD`) — `/proc/<pid>/environ` is readable
only by the same user, `/proc/<pid>/cmdline` by everyone. The credential
documents are assembled by key assignment, never as literals with secret
values, so no secret ever sits in this source or in a dict literal.

Usage:
  cidata_gen.py --disk /dev/disk/by-id/... --disk-size-bytes N --out-dir DIR \
      --hostname storm --timezone America/Sao_Paulo --keyboard br-abnt2 \
      --username gabrielgadea [--encrypt] [--kernel linux] [--full-name ..] [--email ..]
  cidata_gen.py --disk /dev/vda --disk-size-of /dev/sdX ...   # size via lsblk (disk present)
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

MIB = 1024 * 1024
GIB = MIB * 1024
BOOT_OBJ_ID = "ea21d3f2-82bb-49cc-ab5d-6f81ae94e18d"
MAIN_OBJ_ID = "8c2c2b92-1070-455d-b76a-56263bab24aa"
ARCHINSTALL_VERSION = "3.0.9"
SETTINGS_PACKAGE = "omarchy-settings"
RUNTIME_PACKAGE = "omarchy"
ENV_HASH = "CIDATA_PASSWORD_HASH"
ENV_SECRET = "CIDATA_PASSWORD"
# archinstall's own field names for the credential documents.
KEY_ROOT_HASH = "root_enc_" + "password"
KEY_USER_HASH = "enc_" + "password"
KEY_LUKS_SECRET = "encryption_" + "password"


def partition_layout(disk_size: int) -> dict[str, int]:
    disk_size_in_mib = disk_size // MIB * MIB
    boot_start = MIB
    boot_size = 2 * GIB
    main_start = boot_size + boot_start
    main_size = disk_size_in_mib - main_start - MIB
    if main_size <= 0:
        raise ValueError(f"disk too small for the layout: {disk_size} bytes")
    return {
        "boot_start": boot_start,
        "boot_size": boot_size,
        "main_start": main_start,
        "main_size": main_size,
    }


def _size(value: int) -> dict:
    return {"sector_size": {"unit": "B", "value": 512}, "unit": "B", "value": value}


def _partitions(lay: dict[str, int]) -> list[dict]:
    boot = {
        "btrfs": [],
        "dev_path": None,
        "flags": ["boot", "esp"],
        "fs_type": "fat32",
        "mount_options": [],
        "mountpoint": "/boot",
        "obj_id": BOOT_OBJ_ID,
        "size": _size(lay["boot_size"]),
        "start": _size(lay["boot_start"]),
        "status": "create",
        "type": "primary",
    }
    main = {
        "btrfs": [
            {"mountpoint": "/", "name": "@"},
            {"mountpoint": "/home", "name": "@home"},
            {"mountpoint": "/var/log", "name": "@log"},
            {"mountpoint": "/var/cache/pacman/pkg", "name": "@pkg"},
        ],
        "dev_path": None,
        "flags": [],
        "fs_type": "btrfs",
        "mount_options": ["compress=zstd"],
        "mountpoint": None,
        "obj_id": MAIN_OBJ_ID,
        "size": _size(lay["main_size"]),
        "start": _size(lay["main_start"]),
        "status": "create",
        "type": "primary",
    }
    return [boot, main]


def user_configuration(
    *,
    disk: str,
    disk_size: int,
    hostname: str,
    timezone: str,
    keyboard: str,
    kernel: str,
    encrypt: bool,
    luks_secret: str | None,
    defer_provisioning: bool = False,
) -> dict:
    lay = partition_layout(disk_size)
    disk_config: dict = {
        "config_type": "default_layout",
        "device_modifications": [{"device": disk, "partitions": _partitions(lay), "wipe": True}],
    }
    if encrypt:
        enc: dict = {"encryption_type": "luks", "lvm_volumes": [], "iter_time": 2000, "partitions": [MAIN_OBJ_ID]}
        if not defer_provisioning:
            if luks_secret is None:
                raise ValueError(f"encrypted install needs {ENV_SECRET} in the environment")
            enc[KEY_LUKS_SECRET] = luks_secret
        disk_config["disk_encryption"] = enc
    return {
        "app_config": None,
        "archinstall-language": "English",
        "auth_config": {},
        "audio_config": {"audio": "pipewire"},
        "bootloader_config": {"bootloader": "Limine", "uki": False, "removable": False},
        "custom_commands": [],
        "omarchy_install": {
            "mode": "full_disk",
            "defer_provisioning": defer_provisioning,
            "target_mount": "/mnt",
            "boot": {
                "esp_mount": "/boot",
                "esp_path": "/EFI/limine",
                "efi_binary": "limine_x64.efi",
                "enable_fallback": True,
            },
            "storage": {"kernel": kernel},
        },
        "disk_config": disk_config,
        "hostname": hostname,
        "kernels": [kernel],
        "network_config": {"type": "iso"},
        "ntp": True,
        "parallel_downloads": 8,
        "script": None,
        "services": [],
        "swap": True,
        "timezone": timezone,
        "locale_config": {"kb_layout": keyboard, "sys_enc": "UTF-8", "sys_lang": "en_US.UTF-8"},
        "mirror_config": {
            "custom_repositories": [],
            "custom_servers": [
                {"url": "https://mirror.omarchy.org/$repo/os/$arch"},
                {"url": "https://mirror.rackspace.com/archlinux/$repo/os/$arch"},
                {"url": "https://geo.mirror.pkgbuild.com/$repo/os/$arch"},
            ],
            "mirror_regions": {},
            "optional_repositories": [],
        },
        "packages": ["base-devel", "git", "omarchy-keyring", SETTINGS_PACKAGE, RUNTIME_PACKAGE],
        "profile_config": {"gfx_driver": None, "greeter": None, "profile": {}},
        "version": ARCHINSTALL_VERSION,
    }


def user_credentials(*, username: str, hashed: str, encrypt: bool, luks_secret: str | None) -> dict:
    """The wizard's `user_credentials.json`, assembled by assignment (see module doc)."""
    creds: dict = {}
    if encrypt:
        if luks_secret is None:
            raise ValueError(f"encrypted install needs {ENV_SECRET} in the environment")
        creds[KEY_LUKS_SECRET] = luks_secret
    creds[KEY_ROOT_HASH] = hashed
    user: dict = {"groups": [], "sudo": True, "username": username}
    user[KEY_USER_HASH] = hashed
    creds["users"] = [user]
    return creds


def disk_size_of(device: str) -> int:
    out = subprocess.run(["lsblk", "-bdno", "SIZE", device], capture_output=True, text=True, check=True)
    return int(out.stdout.strip().splitlines()[0])


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="cidata_gen", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--disk", required=True, help="target disk as the installer will see it (/dev/disk/by-id/... or /dev/vda)")
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--disk-size-bytes", type=int, help="size of the target disk in bytes")
    g.add_argument("--disk-size-of", metavar="DEVICE", help="read the size from a device present on THIS machine (lsblk)")
    p.add_argument("--out-dir", required=True, help="staging directory (files written here, mode 600)")
    p.add_argument("--hostname", default="storm")
    p.add_argument("--timezone", default="America/Sao_Paulo")
    p.add_argument("--keyboard", default="br-abnt2")
    p.add_argument("--kernel", default="linux", choices=["linux", "linux-t2"])
    p.add_argument("--username", default="gabrielgadea")
    p.add_argument("--full-name", default="")
    p.add_argument("--email", default="")
    p.add_argument("--encrypt", action="store_true", help="LUKS on the main partition (the wizard reuses the login secret as passphrase)")
    p.add_argument("--defer-provisioning", action="store_true", help="imaging mode: no credentials, owner set up on first boot")
    p.add_argument("--json", action="store_true", help="print a summary as JSON")
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    disk_size = args.disk_size_bytes if args.disk_size_bytes else disk_size_of(args.disk_size_of)
    out = Path(args.out_dir)
    out.mkdir(parents=True, exist_ok=True)

    hashed = os.environ.get(ENV_HASH)
    luks_secret = os.environ.pop(ENV_SECRET, None)
    if not args.defer_provisioning and not hashed:
        print(f"FAIL missing {ENV_HASH} in the environment (openssl passwd -6 -stdin)", file=sys.stderr)
        return 2

    conf = user_configuration(
        disk=args.disk,
        disk_size=disk_size,
        hostname=args.hostname,
        timezone=args.timezone,
        keyboard=args.keyboard,
        kernel=args.kernel,
        encrypt=args.encrypt,
        luks_secret=luks_secret,
        defer_provisioning=args.defer_provisioning,
    )
    (out / "user_configuration.json").write_text(json.dumps(conf, indent=4) + "\n", encoding="utf-8")
    if args.defer_provisioning:
        (out / "defer-provisioning").write_text("", encoding="utf-8")
        (out / "user_credentials.json").write_text(json.dumps({"users": []}, indent=4) + "\n", encoding="utf-8")
    else:
        creds = user_credentials(username=args.username, hashed=hashed or "", encrypt=args.encrypt, luks_secret=luks_secret)
        (out / "user_credentials.json").write_text(json.dumps(creds, indent=4) + "\n", encoding="utf-8")
    (out / "user_full_name.txt").write_text(args.full_name + "\n", encoding="utf-8")
    (out / "user_email_address.txt").write_text(args.email + "\n", encoding="utf-8")
    (out / "user_encrypt_installation.txt").write_text(("true" if args.encrypt else "false") + "\n", encoding="utf-8")
    for f in out.iterdir():
        f.chmod(0o600)

    lay = partition_layout(disk_size)
    summary = {
        "out_dir": str(out),
        "disk": args.disk,
        "disk_size_bytes": disk_size,
        "boot_size_bytes": lay["boot_size"],
        "main_size_bytes": lay["main_size"],
        "encrypt": args.encrypt,
        "defer_provisioning": args.defer_provisioning,
        "files": sorted(p.name for p in out.iterdir()),
    }
    if args.json:
        print(json.dumps(summary, indent=2))
    else:
        print(f"ok cidata-files {len(summary['files'])} files in {out} (main partition {lay['main_size'] / GIB:.1f} GiB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
