#!/usr/bin/env bash
# wipe_target.sh — Guarda do wipefs: apaga assinaturas do disco NVMe alvo
#
# IMPORTANTE: Este é o ÚNICO script desta árvore que usa `sudo`.
# Roda no Pop!_OS de forma interativa (como root) imediatamente antes do
# reboot para instalação do Omarchy. Todos os demais scripts NUNCA usam sudo.
#
# Uso: wipe_target.sh [--serial S] [--pop-serial S] [--yes] [--i-typed-the-serial] [--dry-run]
set -euo pipefail

SCRIPT_NAME=$(basename "$0")
DEFAULT_SERIAL="KP102L1HDJDW"
DEFAULT_POP_SERIAL="KP102L1H5AWP"
BY_ID_PREFIX="/dev/disk/by-id/nvme-SM2P41C8-001TC5_"

usage() {
    cat << EOF
Uso: $SCRIPT_NAME [OPÇÕES]

Guarda de segurança para wipefs do disco NVMe alvo KP102L1HDJDW.
SOMENTE executa wipefs/blkdiscard com --yes + confirmação digitada do serial.

Opções:
  --serial SERIAL          Serial do disco alvo    (default: $DEFAULT_SERIAL)
  --pop-serial SERIAL      Serial do disco Pop     (default: $DEFAULT_POP_SERIAL)
  --yes                    Autoriza execução destrutiva (exige confirmação interativa)
  --i-typed-the-serial     Pula confirmação interativa (somente com --yes)
  --dry-run                Valida (a)-(c) e mostra plano; NÃO executa nada
  --help                   Exibe este texto

Verificações (a)-(c) executadas ANTES de qualquer ação:
  (a) /dev/disk/by-id/nvme-...<serial> existe e serial em /sys/block/*/device/serial == SERIAL
  (b) Nenhuma partição do disco alvo está montada
  (c) Disco Pop (POP_SERIAL) tem crypto_LUKS em lsblk e /dev/mapper/* ativo

NOTA: Este script usa sudo para wipefs/blkdiscard apenas quando --yes é passado.
      É o ÚNICO script desta árvore com sudo (roda no Pop de forma interativa).
EOF
}

# ── Arg parsing ───────────────────────────────────────────────────────────────
OPT_SERIAL="$DEFAULT_SERIAL"
OPT_POP_SERIAL="$DEFAULT_POP_SERIAL"
OPT_YES=0
OPT_TYPED=0
OPT_DRY=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --serial)              OPT_SERIAL="$2"; shift 2 ;;
        --pop-serial)          OPT_POP_SERIAL="$2"; shift 2 ;;
        --yes)                 OPT_YES=1; shift ;;
        --i-typed-the-serial)  OPT_TYPED=1; shift ;;
        --dry-run)             OPT_DRY=1; shift ;;
        --help|-h)             usage; exit 0 ;;
        *)
            printf 'Opção desconhecida: %s\n' "$1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

# ── (a) Resolver disco alvo e verificar serial ────────────────────────────────
TARGET_BY_ID="${BY_ID_PREFIX}${OPT_SERIAL}"

if [[ ! -e "$TARGET_BY_ID" ]]; then
    printf 'ERRO: disco alvo não encontrado: %s\n' "$TARGET_BY_ID" >&2
    printf '      serial esperado: %s\n' "$OPT_SERIAL" >&2
    exit 1
fi

TARGET_DEV=""
TARGET_DEV=$(readlink -f "$TARGET_BY_ID") || {
    printf 'ERRO: readlink -f falhou para %s\n' "$TARGET_BY_ID" >&2
    exit 1
}

TARGET_BLK=$(basename "$TARGET_DEV")
SYS_SERIAL=""
serial_path="/sys/block/${TARGET_BLK}/device/serial"
if [[ -r "$serial_path" ]]; then
    SYS_SERIAL=$(tr -d '[:space:]' < "$serial_path")
fi

if [[ -z "$SYS_SERIAL" ]]; then
    printf 'ERRO: não foi possível ler serial de /sys/block/%s/device/serial\n' "$TARGET_BLK" >&2
    printf '      serial esperado: %s\n' "$OPT_SERIAL" >&2
    exit 1
fi

if [[ "$SYS_SERIAL" != "$OPT_SERIAL" ]]; then
    printf 'ERRO: serial lido (%s) != serial esperado (%s)\n' "$SYS_SERIAL" "$OPT_SERIAL" >&2
    printf '      verifique o dispositivo %s antes de prosseguir\n' "$TARGET_DEV" >&2
    exit 1
fi

printf 'ok (a): disco alvo %s serial=%s\n' "$TARGET_DEV" "$SYS_SERIAL"

# ── (b) Verificar montagens ───────────────────────────────────────────────────
MOUNTED=$(lsblk -no MOUNTPOINT "$TARGET_DEV" 2>/dev/null | grep -v '^[[:space:]]*$' || true)
if [[ -n "$MOUNTED" ]]; then
    printf 'ERRO (b): disco alvo tem partições montadas:\n%s\n' "$MOUNTED" >&2
    exit 1
fi
printf 'ok (b): nenhuma montagem ativa em %s\n' "$TARGET_DEV"

# ── (c) Verificar Pop com LUKS ativo ─────────────────────────────────────────
POP_BY_ID="${BY_ID_PREFIX}${OPT_POP_SERIAL}"
POP_LUKS_OK=0
POP_DEV=""

if [[ -e "$POP_BY_ID" ]]; then
    POP_DEV=$(readlink -f "$POP_BY_ID" 2>/dev/null || printf '')
    if [[ -n "$POP_DEV" ]]; then
        POP_BLK=$(basename "$POP_DEV")
        # Verify crypto_LUKS on Pop disk
        if lsblk -no FSTYPE "$POP_DEV" 2>/dev/null | grep -q 'crypto_LUKS'; then
            # Verify active /dev/mapper entry (LUKS is open = Pop is running)
            if find /dev/mapper -mindepth 1 ! -name 'control' 2>/dev/null | grep -q .; then
                # Check if any dm device is backed by Pop block device
                for dm_slave in /sys/block/dm-*/slaves/*; do
                    [[ -e "$dm_slave" ]] || continue
                    slave_name=$(basename "$dm_slave")
                    if [[ "$slave_name" == "${POP_BLK}"* ]]; then
                        POP_LUKS_OK=1
                        break
                    fi
                done
                # No fallback on purpose: "some mapper is open" would also be
                # true on a live USB with an unrelated LUKS volume unlocked, and
                # check (c) exists precisely to prove the Pop runs from the OTHER
                # disk. Only a dm device whose slave sits on the Pop disk counts.
            fi
        fi
    fi
fi

if [[ "$POP_LUKS_OK" -eq 0 ]]; then
    printf 'ERRO (c): disco Pop (%s) não tem crypto_LUKS ativo em /dev/mapper/*\n' "$OPT_POP_SERIAL" >&2
    printf '          prova de que o Pop está no outro disco é necessária\n' >&2
    exit 1
fi
printf 'ok (c): Pop (%s) tem crypto_LUKS ativo em /dev/mapper\n' "$OPT_POP_SERIAL"

# ── Mostrar plano ─────────────────────────────────────────────────────────────
printf '\n=== PLANO DE EXECUÇÃO (IRREVERSÍVEL) ===\n'
printf 'Alvo  : %s (serial %s)\n' "$TARGET_DEV" "$OPT_SERIAL"
printf 'Ações :\n'
printf '  sudo wipefs -a %s\n' "$TARGET_DEV"
printf '  sudo blkdiscard -f %s\n' "$TARGET_DEV"
printf '\nAVISO: estas operações apagam todas as assinaturas e dados do disco.\n\n'

# ── dry-run: para aqui ────────────────────────────────────────────────────────
if [[ "$OPT_DRY" -eq 1 ]]; then
    printf '[dry-run] verificações (a)-(c) OK, plano mostrado. Nenhuma ação executada.\n'
    exit 0
fi

# ── sem --yes: mostrar plano e sair sem executar ──────────────────────────────
if [[ "$OPT_YES" -eq 0 ]]; then
    printf 'Para executar o plano acima, use: %s --yes\n' "$SCRIPT_NAME"
    exit 0
fi

# ── Confirmação interativa (a menos de --i-typed-the-serial) ─────────────────
if [[ "$OPT_TYPED" -eq 0 ]]; then
    printf 'Digite o serial do disco alvo para confirmar (%s): ' "$OPT_SERIAL"
    TYPED_SERIAL=""
    read -r TYPED_SERIAL
    if [[ "$TYPED_SERIAL" != "$OPT_SERIAL" ]]; then
        printf 'ERRO: serial digitado (%s) != serial alvo (%s). Abortando.\n' \
            "$TYPED_SERIAL" "$OPT_SERIAL" >&2
        exit 1
    fi
fi

# ── Execução destrutiva (sudo — ÚNICO ponto sudo desta árvore) ───────────────
printf 'Executando wipefs -a %s ...\n' "$TARGET_DEV"
sudo wipefs -a "$TARGET_DEV"

printf 'Executando blkdiscard -f %s ...\n' "$TARGET_DEV"
if sudo blkdiscard -f "$TARGET_DEV" 2>/dev/null; then
    printf 'blkdiscard concluído.\n'
else
    printf 'blkdiscard falhou (não crítico em alguns SSDs).\n'
fi

printf '\nConcluído. Verifique:\n'
printf '  lsblk -f %s\n' "$TARGET_DEV"
printf 'Esperado: sem partições listadas.\n'
