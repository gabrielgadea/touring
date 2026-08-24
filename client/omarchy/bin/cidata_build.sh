#!/usr/bin/env bash
# cidata_build.sh — build the `cidata` drive image for an Omarchy unattended install.
# Part of OmarchyOS Agêntico (S-0.9, P0; proven in P0V against a VM).
#
# Usage:
#   cidata_build.sh --disk <path-as-the-installer-sees-it> (--disk-size-bytes N | --disk-size-of <local-dev>)
#                   [--out <path>] [--no-encrypt] [--ssh-key <pubkey>] [--allow-disk]
#                   [--hostname storm] [--timezone America/Sao_Paulo] [--keyboard br-abnt2]
#                   [--username gabrielgadea] [--full-name ".."] [--email ".."] [--kernel linux]
#
# Examples:
#   # metal (the target NVMe is present on this machine — read its size):
#   cidata_build.sh --disk /dev/disk/by-id/nvme-SM2P41C8-001TC5_KP102L1HDJDW \
#                   --disk-size-of /dev/disk/by-id/nvme-SM2P41C8-001TC5_KP102L1HDJDW
#   # VM (40 GiB virtio disk, no encryption, SSH key for headless access):
#   cidata_build.sh --disk /dev/vda --disk-size-bytes $((40*1024*1024*1024)) --no-encrypt --ssh-key ~/.ssh/id_ed25519.pub
#
# What it writes is EXACTLY what the ISO wizard writes (`cidata_gen.py` is a
# port of the wizard's heredoc): archinstall's user_configuration.json +
# user_credentials.json + user_full_name.txt + user_email_address.txt +
# user_encrypt_installation.txt [+ authorized_keys]. The wizard uses the login
# password as the LUKS passphrase on encrypted installs, so there is ONE secret.
#
# Secrets: asked on the terminal (never argv); hashed with `openssl passwd -6
# -stdin`; passed to the generator through the environment; staged in tmpfs;
# the ISO defaults to /dev/shm and refuses persistent storage without
# --allow-disk. The plaintext password ends up INSIDE the image on encrypted
# installs (archinstall needs it) — treat the image as the secret it is.
#
# ISO builder: genisoimage | mkisofs | xorrisofs | cidata_iso.py (pycdlib).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GEN="${SCRIPT_DIR}/cidata_gen.py"

OUT_ISO="/dev/shm/cidata.iso"
NO_ENCRYPT=0
SSH_KEY=""
ALLOW_DISK=0
DISK=""
DISK_SIZE_BYTES=""
DISK_SIZE_OF=""
HOSTNAME_="storm"
TIMEZONE="America/Sao_Paulo"
KEYBOARD="br-abnt2"
USERNAME_DEFAULT="gabrielgadea"
FULL_NAME=""
EMAIL=""
KERNEL="linux"

usage() { sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while (( $# > 0 )); do
    case "$1" in
        --out) OUT_ISO="$2"; shift 2 ;;
        --allow-disk) ALLOW_DISK=1; shift ;;
        --no-encrypt) NO_ENCRYPT=1; shift ;;
        --ssh-key) SSH_KEY="$2"; shift 2 ;;
        --disk) DISK="$2"; shift 2 ;;
        --disk-size-bytes) DISK_SIZE_BYTES="$2"; shift 2 ;;
        --disk-size-of) DISK_SIZE_OF="$2"; shift 2 ;;
        --hostname) HOSTNAME_="$2"; shift 2 ;;
        --timezone) TIMEZONE="$2"; shift 2 ;;
        --keyboard) KEYBOARD="$2"; shift 2 ;;
        --username) USERNAME_DEFAULT="$2"; shift 2 ;;
        --full-name) FULL_NAME="$2"; shift 2 ;;
        --email) EMAIL="$2"; shift 2 ;;
        --kernel) KERNEL="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'FAIL unknown argument: %s\n' "$1" >&2; usage >&2; exit 1 ;;
    esac
done

if [ -z "${DISK}" ]; then
    printf 'FAIL --disk is required (the device path the INSTALLER will see)\n' >&2
    exit 1
fi
if [ -z "${DISK_SIZE_BYTES}" ] && [ -z "${DISK_SIZE_OF}" ]; then
    printf 'FAIL one of --disk-size-bytes N | --disk-size-of <device> is required (the wizard computes partitions from the size)\n' >&2
    exit 1
fi
[ -f "${GEN}" ] || { printf 'FAIL missing generator %s\n' "${GEN}" >&2; exit 1; }
command -v openssl >/dev/null 2>&1 || { printf 'FAIL missing-tool openssl\n' >&2; exit 1; }

# ---- Locate ISO builder ------------------------------------------------------
ISO_CMD=""
# Refuse to write an ISO carrying credentials (and possibly the LUKS passphrase)
# onto persistent storage unless the operator says so explicitly: tmpfs
# (/dev/shm, /run/user/<uid>, /tmp on tmpfs) vanishes at reboot, a disk does not.
out_fs="$(stat -f -c %T "$(dirname "${OUT_ISO}")" 2>/dev/null || printf unknown)"
if [ "${out_fs}" != "tmpfs" ] && [ "${ALLOW_DISK}" -ne 1 ]; then
    printf 'FAIL out-not-tmpfs %s is on %s — pass --allow-disk to accept an ISO with secrets on persistent storage\n' "${OUT_ISO}" "${out_fs}" >&2
    exit 1
fi

for cmd in genisoimage mkisofs xorrisofs; do
    if command -v "${cmd}" >/dev/null 2>&1; then
        ISO_CMD="${cmd}"
        break
    fi
done
# Fourth option: the pure-Python builder next to this script (needs pycdlib).
if [ -z "${ISO_CMD}" ] && python3 -c 'import pycdlib' >/dev/null 2>&1; then
    ISO_CMD="cidata_iso.py"
fi
if [ -z "${ISO_CMD}" ]; then
    printf 'FAIL missing-tool genisoimage (or mkisofs/xorrisofs, or pip install --user pycdlib)\n' >&2
    printf 'Install: apt install genisoimage  (Pop/Ubuntu)\n' >&2
    printf '         pacman -S cdrtools        (Arch)\n' >&2
    exit 1
fi

# ---- Staging area in tmpfs ---------------------------------------------------
STAGE="$(mktemp -d /dev/shm/cidata-XXXXXX)"
trap 'rm -rf "${STAGE}"' EXIT
CIDATA_DIR="${STAGE}/cidata"
mkdir -p "${CIDATA_DIR}"

# ---- Credentials (terminal only) --------------------------------------------
printf 'Username [%s]: ' "${USERNAME_DEFAULT}"
read -r USERNAME_IN || USERNAME_IN=""
USERNAME="${USERNAME_IN:-${USERNAME_DEFAULT}}"
if [ "${NO_ENCRYPT}" -eq 0 ]; then
    printf 'Password (hidden — also the LUKS passphrase, as the wizard does): '
else
    printf 'Password (hidden): '
fi
read -rs PASSWORD || PASSWORD=""
printf '\n'
if [ -z "${PASSWORD}" ]; then
    printf 'FAIL empty password\n' >&2
    exit 1
fi

# -stdin: the password must never appear in argv (/proc/<pid>/cmdline, ps aux).
PASSWORD_HASH="$(printf '%s\n' "${PASSWORD}" | openssl passwd -6 -stdin)"

# ---- Generate the wizard's files --------------------------------------------
gen_args=(--disk "${DISK}" --out-dir "${CIDATA_DIR}" --hostname "${HOSTNAME_}" --timezone "${TIMEZONE}"
          --keyboard "${KEYBOARD}" --username "${USERNAME}" --full-name "${FULL_NAME}" --email "${EMAIL}"
          --kernel "${KERNEL}")
if [ -n "${DISK_SIZE_BYTES}" ]; then gen_args+=(--disk-size-bytes "${DISK_SIZE_BYTES}"); else gen_args+=(--disk-size-of "${DISK_SIZE_OF}"); fi
if [ "${NO_ENCRYPT}" -eq 0 ]; then
    gen_args+=(--encrypt)
    # Secrets by environment, never argv (see header).
    CIDATA_PASSWORD_HASH="${PASSWORD_HASH}" CIDATA_PASSWORD="${PASSWORD}" python3 "${GEN}" "${gen_args[@]}" >/dev/null
else
    CIDATA_PASSWORD_HASH="${PASSWORD_HASH}" python3 "${GEN}" "${gen_args[@]}" >/dev/null
fi
PASSWORD=""   # clear from this shell as soon as the generator consumed it
PASSWORD_HASH=""

# ---- Optional SSH public key -------------------------------------------------
if [ -n "${SSH_KEY}" ]; then
    if [ ! -f "${SSH_KEY}" ]; then
        printf 'FAIL ssh-key not found: %s\n' "${SSH_KEY}" >&2
        exit 1
    fi
    cp "${SSH_KEY}" "${CIDATA_DIR}/authorized_keys"
    chmod 600 "${CIDATA_DIR}/authorized_keys"
fi

# ---- Build ISO ---------------------------------------------------------------
case "${ISO_CMD}" in
    genisoimage|mkisofs)
        "${ISO_CMD}" -output "${OUT_ISO}" -volid cidata -joliet -rock "${CIDATA_DIR}/" 2>/dev/null
        ;;
    xorrisofs)
        xorrisofs -output "${OUT_ISO}" -volid cidata -joliet -rock "${CIDATA_DIR}/" 2>/dev/null
        ;;
    cidata_iso.py)
        python3 "${SCRIPT_DIR}/cidata_iso.py" "${CIDATA_DIR}" "${OUT_ISO}"
        ;;
esac
chmod 600 "${OUT_ISO}" 2>/dev/null || true

printf 'ok cidata-iso %s (%s; files: %s)\n' "${OUT_ISO}" "${ISO_CMD}" "$(ls "${CIDATA_DIR}" | tr '\n' ' ')"
if [ "${NO_ENCRYPT}" -eq 0 ]; then
    printf 'WARNING: encrypted install — the image carries the plaintext passphrase (archinstall needs it). Keep it in tmpfs; delete after use.\n'
else
    printf 'WARNING: the image carries a password hash. Keep it in tmpfs; delete after use.\n'
fi
