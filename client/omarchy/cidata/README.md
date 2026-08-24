# `cidata/` — Omarchy unattended install drive

Generates a `cidata` ISO (cloud-init NoCloud format) that lets the Omarchy
installer run without a wizard. Attach it as a second CD-ROM alongside the
Omarchy ISO, or pass it to `virt-install --disk`.

## Files in the generated ISO

| File | Required | Content |
|---|---|---|
| `user_configuration.json` | Yes | Disk by-id, hostname, timezone, keyboard |
| `user_credentials.json` | Yes | `{"username":"…","password_hash":"$6$…"}` |
| `authorized_keys` | Optional | SSH public keys (one per line) |
| `user_encrypt_installation.txt` | Optional | `true` when LUKS block is present |

**SECURITY**: `user_credentials.json` contains a hashed password.  
`user_encrypt_installation.txt` + the `disk_encryption` passphrase  
(if present) are **secrets** — the ISO is built into `/dev/shm` (tmpfs)  
and **never written to repo or to a FAT pendrive**.

## Build

```bash
# Interactive (asks for username, password, optionally LUKS passphrase):
client/omarchy/bin/cidata_build.sh

# No encryption (for VMs / testing):
client/omarchy/bin/cidata_build.sh --no-encrypt

# Custom output path and SSH key:
client/omarchy/bin/cidata_build.sh --out /dev/shm/cidata.iso --ssh-key ~/.ssh/id_ed25519.pub
```

The script requires `genisoimage` (Debian/Ubuntu/Pop: `apt install genisoimage`;
Arch: `cdrtools` or `xorriso` as fallback).

## Template

`user_configuration.template.json` contains the **non-secret** fields for this
machine (disk serial, hostname, timezone, keyboard). It is safe to version.  
The build script copies it and merges credentials + optional LUKS block in
memory (in `/dev/shm`) before calling `genisoimage`.

## VM test

```bash
# Build without encryption (safe for VM testing):
client/omarchy/bin/cidata_build.sh --no-encrypt --out /dev/shm/cidata-test.iso

virt-install --name omarchy-proto --memory 8192 --vcpus 8 --disk size=40 \
  --cdrom ~/omarchy-kit/omarchy-4.0.0.iso \
  --disk /dev/shm/cidata-test.iso,device=cdrom --boot uefi
```

## Disk identifier

This template targets disk serial `KP102L1HDJDW`  
(`/dev/disk/by-id/nvme-SM2P41C8-001TC5_KP102L1HDJDW`).  
For a different machine, edit `disk` in `user_configuration.template.json`.
