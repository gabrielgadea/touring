#!/usr/bin/env bash
# validate_p2.sh — Gate P2: dual boot + segurança base
# Roda no Omarchy após configurar Limine e dual boot.
# Uso: validate_p2.sh [--json]
set -euo pipefail

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
_PHASE="P2"
_JSON_MODE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)   _JSON_MODE=1; shift ;;
        --help|-h)
            printf 'Uso: validate_p2.sh [--json]\n'
            printf 'Gate P2: dual boot Limine, snapshot, segurança, hooks.\n'
            exit 0 ;;
        *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
    esac
done

# shellcheck source=_validate_lib.sh
source "$SCRIPT_DIR/_validate_lib.sh"

# `/boot` e' a ESP montada com dmask=0077 e snapper/ufw so' respondem a root.
# Rodado sem privilegio — como faz a rotina diaria `routine-validate-all`, que e'
# uma unit `systemd --user` — nenhum destes quatro checks CONSEGUE medir o que
# afirma. Reportar FAIL ali e' afirmar que o dual boot quebrou quando o que houve
# foi falta de permissao: a rotina de auto-prova nasceria vermelha todo dia e
# deixaria de significar qualquer coisa. Com privilegio o comportamento e'
# identico ao anterior — um estado de fato errado continua reprovando.
# Medido 2026-08-23: `ls /boot` => Permission denied para o usuario.
PRIVILEGED=0
if [[ -r /boot ]]; then PRIVILEGED=1; fi
unmeasurable() {
    local name="$1" what="$2"
    check_warn "$name" "cannot attest as an unprivileged user ($what needs root) — re-run with sudo env HOME=\$HOME"
}

# ── 1. Snapshot "P2 baseline" em snapper list ────────────────────────────────
if need_tool snapper; then
    if snapper list 2>/dev/null | grep -q 'P2 baseline'; then
        check_ok "snapper-p2-baseline" "snapshot 'P2 baseline' present"
    elif [[ "$PRIVILEGED" -eq 0 ]]; then
        unmeasurable "snapper-p2-baseline" "snapper list"
    else
        check_fail "snapper-p2-baseline" "snapshot 'P2 baseline' not found in snapper list"
    fi
fi

# ── 2. /boot/limine.conf cita pop|systemd-boot ───────────────────────────────
LIMINE_CONF="/boot/limine.conf"
if [[ -f "$LIMINE_CONF" ]]; then
    if grep -qiE 'pop|systemd-boot' "$LIMINE_CONF"; then
        check_ok "limine-conf-pop-entry" "pop|systemd-boot found in $LIMINE_CONF"
    else
        check_fail "limine-conf-pop-entry" "no pop|systemd-boot entry in $LIMINE_CONF"
    fi
elif [[ "$PRIVILEGED" -eq 0 ]]; then
    unmeasurable "limine-conf-exists" "reading /boot"
else
    check_fail "limine-conf-exists" "$LIMINE_CONF not found"
fi

# ── 3. /boot/limine.conf.bak.p2 existe ──────────────────────────────────────
if [[ -f "/boot/limine.conf.bak.p2" ]]; then
    check_ok "limine-conf-backup" "/boot/limine.conf.bak.p2 exists"
elif [[ "$PRIVILEGED" -eq 0 ]]; then
    unmeasurable "limine-conf-backup" "reading /boot"
else
    check_fail "limine-conf-backup" "/boot/limine.conf.bak.p2 not found"
fi

# ── 4. efibootmgr BootOrder começa por Limine|Omarchy ───────────────────────
if need_tool efibootmgr; then
    _efi="$(efibootmgr 2>/dev/null)" || _efi=""
    boot_order_line=$(printf '%s' "$_efi" | grep -m1 '^BootOrder:' || true)
    if [[ -n "$boot_order_line" ]]; then
        # BootOrder: 0004,0005,... -> somente o primeiro entry.
        # `grep -oEm1` imprimiria TODOS os matches da linha (-m1 limita linhas de
        # entrada, nao matches); expansao pura evita pipe e o pattern multilinha.
        first_boot="${boot_order_line#*BootOrder:}"
        first_boot="${first_boot//[[:space:]]/}"
        first_boot="${first_boot%%,*}"
        if [[ -n "$first_boot" ]]; then
            entry_label=$(printf '%s' "$_efi" | grep -m1 "^Boot${first_boot}" || true)
            if printf '%s' "$entry_label" | grep -qiE 'Limine|Omarchy'; then
                check_ok "efi-boot-order" "BootOrder first=$first_boot label: $entry_label"
            else
                check_fail "efi-boot-order" "first boot entry ($first_boot) not Limine/Omarchy: $entry_label"
            fi
        else
            check_fail "efi-boot-order" "could not parse BootOrder from efibootmgr"
        fi
    else
        check_fail "efi-boot-order" "no BootOrder line in efibootmgr output"
    fi
fi

# ── 5. Prova humana ~/Work/state/p2-pop-boot-ok ──────────────────────────────
P2_PROOF="$HOME/Work/state/p2-pop-boot-ok"
if [[ -f "$P2_PROOF" ]]; then
    check_ok "p2-pop-boot-proof" "$P2_PROOF exists"
else
    check_fail "p2-pop-boot-proof" "$P2_PROOF not found (run: touch $P2_PROOF after booting Pop)"
fi

# ── 6. ufw status active ─────────────────────────────────────────────────────
if need_tool ufw; then
    if ufw status 2>/dev/null | grep -q 'Status: active'; then
        check_ok "ufw-active" "ufw status: active"
    elif [[ "$PRIVILEGED" -eq 0 ]]; then
        unmeasurable "ufw-active" "ufw status"
    else
        check_fail "ufw-active" "ufw not active"
    fi
fi

# ── 6b. guarda de BootOrder habilitado ───────────────────────────────────────
# Sem ele o arranjo de dual boot nao e' estavel: bootar o Pop reverte o BootOrder
# do firmware (medido no S-2.5) e o proximo reboot entra no Pop ignorando o
# Limine. `efi-boot-order` acima mede o ESTADO; este mede se existe algo que o
# RESTAURA. Ordem certa agora sem guarda = sorte, nao arranjo.
if command -v systemctl >/dev/null 2>&1; then
    if systemctl is-enabled omarchy-bootorder-guard.service >/dev/null 2>&1; then
        check_ok "bootorder-guard-enabled" "omarchy-bootorder-guard.service enabled"
    else
        check_fail "bootorder-guard-enabled" \
            "omarchy-bootorder-guard.service not enabled — BootOrder will revert on the next Pop boot"
    fi
fi

# ── 7. 4 hooks nossos executáveis em ~/.config/omarchy/hooks/ ────────────────
HOOKS_DIR="$HOME/.config/omarchy/hooks"
expected_hooks=(
    "post-boot.d/20-touring-daemon"
    "post-update.d/30-touring-doctor"
    "post-update.d/35-hooks-runnable"
    "post-update.d/40-limine-rescan"
)
hooks_found=0
for hook in "${expected_hooks[@]}"; do
    hook_path="$HOOKS_DIR/$hook"
    if [[ -x "$hook_path" ]]; then
        hooks_found=$(( hooks_found + 1 ))
    else
        check_fail "hook-missing-${hook##*/}" "$hook_path not executable or not found"
    fi
done
if [[ "$hooks_found" -eq "${#expected_hooks[@]}" ]]; then
    check_ok "our-hooks-executable" "$hooks_found/4 hooks executable in $HOOKS_DIR"
fi

finish "$_PHASE"
