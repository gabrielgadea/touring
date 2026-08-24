#!/usr/bin/env bash
# validate_p1.sh — Gate P1: pós-instalação no Omarchy (primeiro boot)
# Roda no Omarchy, após o wipe e instalação.
# Uso: validate_p1.sh [--json]
set -euo pipefail

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
_PHASE="P1"
_JSON_MODE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)   _JSON_MODE=1; shift ;;
        --help|-h)
            printf 'Uso: validate_p1.sh [--json]\n'
            printf 'Gate P1: valida pós-instalação do Omarchy (serial LUKS, GPU, rede, herdr).\n'
            exit 0 ;;
        *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
    esac
done

# shellcheck source=_validate_lib.sh
source "$SCRIPT_DIR/_validate_lib.sh"

TARGET_SERIAL="KP102L1HDJDW"
POP_SERIAL="KP102L1H5AWP"
POP_PARTUUID="10f3c3bf-ab65-4b78-b56a-3d504a05129e"

# ── 1. Serial do disco com LUKS == TARGET_SERIAL ─────────────────────────────
found_target=0
for serial_file in /sys/block/nvme*/device/serial; do
    [[ -r "$serial_file" ]] || continue
    s=$(tr -d '[:space:]' < "$serial_file")
    if [[ "$s" == "$TARGET_SERIAL" ]]; then
        found_target=1
        blkdev=$(basename "$(dirname "$(dirname "$serial_file")")")
        # Verify LUKS on this device
        if lsblk -no FSTYPE "/dev/${blkdev}" 2>/dev/null | grep -q 'crypto_LUKS'; then
            check_ok "target-serial-luks" "serial=$s device=/dev/${blkdev} has crypto_LUKS"
        else
            check_fail "target-serial-luks" "serial=$s device=/dev/${blkdev} missing crypto_LUKS"
        fi
        break
    fi
done
if [[ "$found_target" -eq 0 ]]; then
    check_fail "target-serial-luks" "serial $TARGET_SERIAL not found in /sys/block/nvme*/device/serial"
fi

# ── 2. Disco Pop com crypto_LUKS + PARTUUID correto ──────────────────────────
found_pop=0
for serial_file in /sys/block/nvme*/device/serial; do
    [[ -r "$serial_file" ]] || continue
    s=$(tr -d '[:space:]' < "$serial_file")
    if [[ "$s" == "$POP_SERIAL" ]]; then
        found_pop=1
        blkdev=$(basename "$(dirname "$(dirname "$serial_file")")")
        if lsblk -no FSTYPE "/dev/${blkdev}" 2>/dev/null | grep -q 'crypto_LUKS'; then
            check_ok "pop-serial-luks" "serial=$s has crypto_LUKS"
        else
            check_fail "pop-serial-luks" "serial=$s missing crypto_LUKS"
        fi
        break
    fi
done
if [[ "$found_pop" -eq 0 ]]; then
    check_warn "pop-serial-luks" "$POP_SERIAL not found (normal if Pop not installed yet)"
fi

# ── 3. PARTUUID do Pop ───────────────────────────────────────────────────────
if command -v blkid >/dev/null 2>&1; then
    if blkid 2>/dev/null | grep -qi "$POP_PARTUUID"; then
        check_ok "pop-partuuid" "PARTUUID $POP_PARTUUID present in blkid"
    else
        check_warn "pop-partuuid" "PARTUUID $POP_PARTUUID not found in blkid (normal before dual-boot setup)"
    fi
else
    need_tool blkid
fi

# ── 4. omarchy-version == 4.0.* ──────────────────────────────────────────────
if command -v omarchy-version >/dev/null 2>&1; then
    ov=$(omarchy-version 2>/dev/null || printf '')
    if printf '%s' "$ov" | grep -qE '^4\.0\.'; then
        check_ok "omarchy-version" "$ov"
    else
        check_fail "omarchy-version" "expected ^4.0.*, got: $ov"
    fi
else
    need_tool omarchy-version
fi

# ── 5. nvidia-smi exit 0 ─────────────────────────────────────────────────────
if need_tool nvidia-smi; then
    if nvidia-smi >/dev/null 2>&1; then
        check_ok "nvidia-smi" "exit 0"
    else
        check_fail "nvidia-smi" "exit nonzero"
    fi
fi

# ── 6. ip link mostra interface enp*/eth* ────────────────────────────────────
if need_tool ip; then
    if ip link 2>/dev/null | grep -qE 'enp|eth'; then
        check_ok "network-interface" "enp*/eth* found in ip link"
    else
        check_fail "network-interface" "no enp*/eth* interface in ip link"
    fi
fi

# ── 7. herdr --version ───────────────────────────────────────────────────────
if need_tool herdr; then
    hv=$(herdr --version 2>&1 || printf '')
    check_ok "herdr-version" "$hv"
fi

# ── 8. claude presente (warn se ausente — chega em P3) ───────────────────────
if command -v claude >/dev/null 2>&1; then
    _cv="$(claude --version 2>&1)" || _cv="unknown"
    check_ok "claude-present" "${_cv%%$'\n'*}"
else
    check_warn "claude-present" "claude not found (expected after P3)"
fi

# ── 9. root em btrfs subvolume @ ─────────────────────────────────────────────
# `btrfs subvolume list /` exige root — como usuário devolve vazio e, sob
# pipefail, o `|| printf 0` ANEXAVA ao que o wc já tinha impresso ("0\n0" →
# erro aritmético; 3ª ocorrência da classe pipefail+pipe+fallback, ver
# lesson:pipefail-append). `findmnt` responde sem root e prova o que importa:
# raiz em btrfs montada do subvolume @ (layout que o instalador cria).
if need_tool findmnt; then
    root_fs="$(findmnt -no FSTYPE / 2>/dev/null)" || root_fs=""
    root_src="$(findmnt -no FSROOT / 2>/dev/null)" || root_src=""
    if [[ "$root_fs" == "btrfs" && "$root_src" == "/@" ]]; then
        check_ok "btrfs-root-subvol" "/ is btrfs subvol @"
    else
        check_fail "btrfs-root-subvol" "expected btrfs /@, got ${root_fs:-?} ${root_src:-?}"
    fi
fi

# ── 10. snapper list-configs ─────────────────────────────────────────────────
if need_tool snapper; then
    if snapper list-configs >/dev/null 2>&1; then
        check_ok "snapper-configs" "snapper list-configs exit 0"
    else
        check_fail "snapper-configs" "snapper list-configs failed"
    fi
fi

finish "$_PHASE"
