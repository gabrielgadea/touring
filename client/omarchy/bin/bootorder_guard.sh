#!/usr/bin/env bash
# bootorder_guard.sh — keep the Limine entry first in the UEFI BootOrder.
#
# Uso: bootorder_guard.sh [--check] [--label NAME] [--json]
#   --check   report only; never write to NVRAM (exit 1 if out of order)
#   --label   boot entry label to keep first (default: Limine)
#
# ── Por que isto existe ───────────────────────────────────────────────────────
# Medido 2026-08-23 (P2, S-2.5): bootar o Pop!_OS REVERTE o BootOrder do firmware
# para Pop-first, desfazendo o S-2.4. Sem reasserção o reboot seguinte entra no
# Pop ignorando o Limine — o arranjo de dual boot não é estável.
#
# A causa está do lado do Pop, não do Omarchy: o Pop!_OS 24.04 arranca por
# systemd-boot (`Boot0004 … \EFI\systemd\systemd-bootx64.efi`) e o `kernelstub`
# reafirma a própria entrada. Nada no Limine reafirma ordem de boot — a
# documentação dele registra a entrada uma vez, com `efibootmgr --create`, e para
# aí (Limine USAGE.md / llms.txt, consultado via Context7 em 23/08/2026). Logo a
# reasserção tem de ser nossa, e do lado que ainda controlamos: todo boot do
# Omarchy devolve o Limine para a frente da fila.
#
# Isto NÃO conserta o caso "reboot direto de dentro do Pop" — nesse caminho o
# Omarchy não roda para poder corrigir nada. Conserta o ciclo real: Pop -> volta
# ao Omarchy -> ordem restaurada antes do próximo desligamento.
#
# Idempotente: se o rótulo já é o primeiro, não escreve na NVRAM.

set -euo pipefail

LABEL="Limine"
CHECK_ONLY=0
JSON_OUT=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --check) CHECK_ONLY=1; shift ;;
        --json)  JSON_OUT=1;   shift ;;
        --label) LABEL="${2:?--label needs a value}"; shift 2 ;;
        --help|-h)
            sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *) printf 'bootorder_guard.sh: unknown argument: %s\n' "$1" >&2; exit 2 ;;
    esac
done

emit() {  # status detail
    if [[ "$JSON_OUT" -eq 1 ]]; then
        printf '{"status":"%s","label":"%s","detail":"%s"}\n' "$1" "$LABEL" "$2"
    else
        printf '%s %s\n' "$1" "$2"
    fi
}

if [[ ! -d /sys/firmware/efi ]]; then
    emit "skip" "not an EFI boot; nothing to guard"
    exit 0
fi
if ! command -v efibootmgr >/dev/null 2>&1; then
    emit "skip" "efibootmgr not installed"
    exit 0
fi

out="$(efibootmgr 2>/dev/null)" || {
    # Reading NVRAM needs root. Saying "out of order" here would be a claim we
    # cannot support — the honest answer is that we could not look.
    emit "skip" "cannot read efibootmgr (needs root)"
    exit 0
}

# BootOrder: 0000,0004,... -> primeiro entry apenas. NAO usar `grep -oEm1`: o -m1
# limita LINHAS de entrada, nao ocorrencias, e imprimiria todos os ids da linha
# (mesmo defeito corrigido no efi-boot-order do validate_p2.sh em 23/08/2026).
order_line="$(printf '%s\n' "$out" | grep -m1 '^BootOrder:' || printf '')"
if [[ -z "$order_line" ]]; then
    emit "skip" "no BootOrder line in efibootmgr output"
    exit 0
fi
order="${order_line#*BootOrder:}"
order="${order//[[:space:]]/}"
first="${order%%,*}"

# Boot0000* Limine  HD(...)  -> id do rotulo procurado
target="$(printf '%s\n' "$out" \
    | grep -E "^Boot[0-9A-Fa-f]{4}\*?[[:space:]]+${LABEL}([[:space:]]|$)" \
    | head -1 | sed -E 's/^Boot([0-9A-Fa-f]{4}).*/\1/')"
if [[ -z "$target" ]]; then
    emit "fail" "no boot entry labelled '$LABEL'"
    exit 1
fi

if [[ "$first" == "$target" ]]; then
    emit "ok" "$LABEL ($target) already first in BootOrder=$order"
    exit 0
fi

if [[ "$CHECK_ONLY" -eq 1 ]]; then
    emit "fail" "$LABEL ($target) is not first; BootOrder=$order"
    exit 1
fi

# Move o alvo para a frente preservando a ordem relativa do resto.
rest="$(printf '%s' "$order" | tr ',' '\n' | grep -vx "$target" | paste -sd, -)"
new_order="$target${rest:+,$rest}"
if efibootmgr -o "$new_order" >/dev/null 2>&1; then
    emit "fixed" "BootOrder $order -> $new_order"
    exit 0
fi
emit "fail" "efibootmgr -o $new_order failed (needs root)"
exit 1
