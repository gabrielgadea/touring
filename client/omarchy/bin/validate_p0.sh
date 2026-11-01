#!/usr/bin/env bash
# validate_p0.sh — Gate P0: valida o kit de migração no Pop antes do wipe
# Roda no Pop!_OS (máquina de origem), não no Omarchy.
# Uso: validate_p0.sh [--json] [--usb /dev/disk/by-id/usb-...]
set -euo pipefail

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
OMARCHY_ROOT="${SCRIPT_DIR%/*}"
_PHASE="P0"

# ── Arg parsing ───────────────────────────────────────────────────────────────
_JSON_MODE=0
OPT_USB=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)   _JSON_MODE=1; shift ;;
        --usb)    OPT_USB="$2"; shift 2 ;;
        --help|-h)
            printf 'Uso: validate_p0.sh [--json] [--usb /dev/disk/by-id/usb-...]\n'
            printf 'Gate P0: valida kit de migração, validadores, ISO e dependências.\n'
            exit 0 ;;
        *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
    esac
done

# shellcheck source=_validate_lib.sh
source "$SCRIPT_DIR/_validate_lib.sh"

KIT_DIR="$HOME/omarchy-kit"
ISO_FILE="$KIT_DIR/omarchy-4.0.0.iso"
ISO_SIG="$KIT_DIR/omarchy-4.0.0.iso.sig"
ISO_SIZE_EXPECTED=6273040384
GPG_KEY="40DFB630FF42BCFFB047046CF0134EE680CAC571"

# ── 1. Kit íntegro (kit_check.sh --self) ─────────────────────────────────────
if command -v kit_check.sh >/dev/null 2>&1 || [[ -x "$SCRIPT_DIR/kit_check.sh" ]]; then
    local_kit="$SCRIPT_DIR/kit_check.sh"
    if "${local_kit}" "$KIT_DIR" --self >/dev/null 2>&1; then
        check_ok "kit-integrity" "kit_check.sh --self passed"
    else
        check_fail "kit-integrity" "kit_check.sh --self failed"
    fi
else
    check_warn "kit-integrity" "pending-artifact bin/kit_check.sh"
fi

# ── 2. SYMLINKS.tsv ──────────────────────────────────────────────────────────
SYMLINKS_TSV="$KIT_DIR/SYMLINKS.tsv"
if [[ -f "$SYMLINKS_TSV" ]]; then
    # First line is the header (path/target/type); count entries, not lines,
    # and cross-check against kit.json so the TSV cannot drift from the kit.
    tsv_lines=$(( $(wc -l < "$SYMLINKS_TSV") - 1 ))
    # `x=$(...) || x=0`: `grep -c` imprime a contagem E sai 1 sem match, entao
    # `$(... || printf '0')` anexa um segundo zero -> "0\n0" -> "arithmetic
    # syntax error" no `[[ ]]` seguinte (classe medida 2026-08-23).
    tsv_abs=$(grep -c $'\tabs$' "$SYMLINKS_TSV" 2>/dev/null) || tsv_abs=0
    kit_links=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1])).get("symlinks_total","?"))' "$KIT_DIR/kit.json" 2>/dev/null || printf '?')
    if [[ "$kit_links" == "$tsv_lines" ]]; then
        check_ok "symlinks-tsv" "${tsv_lines} symlinks (${tsv_abs} abs) == kit.json"
    else
        check_fail "symlinks-tsv" "${tsv_lines} entries in TSV but kit.json says ${kit_links}"
    fi
    # Every ABSOLUTE target must be one the plan knows how to restore on the
    # Omarchy (D7: clone `analise`; P4: `update-touring` rebuilds target/release).
    # An absolute target outside that allowlist is a skill that would silently
    # die after the wipe — stronger than asserting "exactly N abs".
    unknown_abs=$(awk -F'\t' 'NR>1 && $3=="abs" && $2 !~ "^/home/gabrielgadea/projects/(analise|touring)/" {print $1" -> "$2}' "$SYMLINKS_TSV")
    if [[ -z "$unknown_abs" ]]; then
        check_ok "symlinks-abs-allowlist" "${tsv_abs} abs targets all under ~/projects/{analise,touring}"
    else
        check_fail "symlinks-abs-allowlist" "abs targets outside the restore plan: ${unknown_abs//$'\n'/; }"
    fi
else
    check_fail "symlinks-tsv" "$SYMLINKS_TSV not found"
fi

# ── 3. settings.stage{A,B}.json válidos + hooks_runnable.py stageA ───────────
for stage in A B; do
    sf="$KIT_DIR/settings.stage${stage}.json"
    if [[ -f "$sf" ]]; then
        if python3 -c "import json; json.load(open('$sf'))" 2>/dev/null; then
            check_ok "settings-stage${stage}-valid" "$sf valid JSON"
        else
            check_fail "settings-stage${stage}-valid" "$sf invalid JSON"
        fi
    else
        check_warn "settings-stage${stage}-valid" "pending-artifact $sf"
    fi
done

HOOKS_RUNNABLE="$SCRIPT_DIR/hooks_runnable.py"
STAGE_A="$KIT_DIR/settings.stageA.json"
if [[ -x "$HOOKS_RUNNABLE" ]] || command -v hooks_runnable.py >/dev/null 2>&1; then
    hr="${HOOKS_RUNNABLE}"
    if [[ -f "$STAGE_A" ]]; then
        if python3 "$hr" "$STAGE_A" --home "$HOME" >/dev/null 2>&1; then
            check_ok "hooks-runnable-stageA" "exit 0"
        else
            check_fail "hooks-runnable-stageA" "hooks_runnable.py stageA exit nonzero"
        fi
    else
        check_warn "hooks-runnable-stageA" "pending-artifact $STAGE_A"
    fi
else
    check_warn "hooks-runnable-stageA" "pending-artifact bin/hooks_runnable.py"
fi

# ── 4. bash -n + shellcheck dos 9 validadores ────────────────────────────────
VALIDATOR_SCRIPTS=()
for n in 0 1 2 3 4 5 6 7 8; do
    VALIDATOR_SCRIPTS+=("$SCRIPT_DIR/validate_p${n}.sh")
done
VALIDATOR_SCRIPTS+=("$SCRIPT_DIR/validate_all.sh")

syntax_ok=1
for vscript in "${VALIDATOR_SCRIPTS[@]}"; do
    vname=$(basename "$vscript")
    if [[ -f "$vscript" ]]; then
        if bash -n "$vscript" 2>/dev/null; then
            check_ok "bash-n-${vname}" "syntax ok"
        else
            check_fail "bash-n-${vname}" "syntax error"
            syntax_ok=0
        fi
    else
        check_fail "bash-n-${vname}" "$vscript not found"
        syntax_ok=0
    fi
done

if [[ "$syntax_ok" -eq 1 ]]; then
    if command -v shellcheck >/dev/null 2>&1; then
        # shellcheck disable=SC2068
        if shellcheck -S warning "${VALIDATOR_SCRIPTS[@]}" 2>/dev/null; then
            check_ok "shellcheck-validators" "all validators clean"
        else
            check_fail "shellcheck-validators" "shellcheck found warnings"
        fi
    else
        check_fail "missing-tool" "shellcheck"
    fi
fi

# ── 5. vendor/omarchy-plugin-validate skills-deck ────────────────────────────
PLUGIN_VALIDATE="$OMARCHY_ROOT/vendor/omarchy-plugin-validate"
SKILLS_DECK="$OMARCHY_ROOT/skills-deck"
if [[ -x "$PLUGIN_VALIDATE" ]]; then
    if [[ -d "$SKILLS_DECK" ]]; then
        if "$PLUGIN_VALIDATE" "$SKILLS_DECK" >/dev/null 2>&1; then
            check_ok "plugin-validate-skills-deck" "exit 0"
        else
            check_fail "plugin-validate-skills-deck" "vendor/omarchy-plugin-validate failed"
        fi
    else
        check_warn "plugin-validate-skills-deck" "pending-artifact skills-deck/"
    fi
else
    check_warn "plugin-validate-skills-deck" "pending-artifact vendor/omarchy-plugin-validate"
fi

# ── 6. Menu JSONC válido ──────────────────────────────────────────────────────
MENU_FILE="$OMARCHY_ROOT/extensions/omarchy-menu.jsonc"
if [[ -f "$MENU_FILE" ]]; then
    if python3 - "$MENU_FILE" <<'PYEOF' 2>/dev/null
import json, sys
# String-aware JSONC strip: a bare `//[^\n]*` regex also eats the `//` inside
# "http://127.0.0.1:7777" and reported the menu invalid while the shell would
# load it fine (caught by this very gate on 23/08 — a validator that is
# stricter than the consumer in the wrong place is a false alarm, not safety).
src = open(sys.argv[1], encoding="utf-8").read()
out, i, n, in_str, esc = [], 0, len(src), False, False
while i < n:
    c = src[i]
    if in_str:
        out.append(c)
        if esc: esc = False
        elif c == "\\": esc = True
        elif c == '"': in_str = False
        i += 1
    elif c == '"':
        in_str = True; out.append(c); i += 1
    elif src.startswith("//", i):
        j = src.find("\n", i); i = n if j < 0 else j
    elif src.startswith("/*", i):
        j = src.find("*/", i + 2); i = n if j < 0 else j + 2
    else:
        out.append(c); i += 1
json.loads("".join(out))
PYEOF
    then
        check_ok "menu-jsonc-valid" "$MENU_FILE valid after comment strip"
    else
        check_fail "menu-jsonc-valid" "$MENU_FILE invalid JSON after comment strip"
    fi
else
    check_warn "menu-jsonc-valid" "pending-artifact extensions/omarchy-menu.jsonc"
fi

# ── 7. systemd-analyze --user verify das units de routines_gen.py apply ──────
ROUTINES_GEN="$SCRIPT_DIR/routines_gen.py"
if [[ -f "$ROUTINES_GEN" ]]; then
    if command -v systemd-analyze >/dev/null 2>&1; then
        tmp_prefix=$(mktemp -d /tmp/validate_p0_units_XXXXXX)
        # --dry writes the units but never calls systemctl; the example TOML is
        # the fixture (on the Pop there is no ~/Work/routines.toml yet).
        if python3 "$ROUTINES_GEN" apply --toml "$OMARCHY_ROOT/routines.example.toml" --prefix "$tmp_prefix" --dry >/dev/null 2>&1; then
            unit_files=("$tmp_prefix"/*.service)
            if [[ ${#unit_files[@]} -gt 0 ]] && [[ -f "${unit_files[0]}" ]]; then
                if systemd-analyze --user verify "${unit_files[@]}" >/dev/null 2>&1; then
                    check_ok "systemd-analyze-units" "units valid in $tmp_prefix"
                else
                    check_fail "systemd-analyze-units" "systemd-analyze failed on generated units"
                fi
            else
                check_warn "systemd-analyze-units" "no units generated by routines_gen.py"
            fi
        else
            check_fail "routines-gen-apply" "routines_gen.py apply --prefix failed"
        fi
        rm -rf "$tmp_prefix"
    else
        check_fail "missing-tool" "systemd-analyze"
    fi
else
    check_warn "routines-gen-apply" "pending-artifact bin/routines_gen.py"
fi

# ── 8. touring adw lint herdr-fanout-demo ─────────────────────────────────────
ADW_SPEC_SRC="$OMARCHY_ROOT/adw/herdr-fanout-demo.toml"
ADW_SPEC_PROJ="$HOME/projects/touring/.touring/adw/herdr-fanout-demo.toml"
if [[ -f "$ADW_SPEC_SRC" ]] || [[ -f "$ADW_SPEC_PROJ" ]]; then
    if command -v touring >/dev/null 2>&1; then
        # Per-project: `adw lint` resolve o spec em <proj>/.touring/adw/ pelo CWD.
        # Sem fixar o diretorio o check reprovava sempre que o gate era invocado
        # de fora do repo — por exemplo pela rotina `validate-all`, que roda com
        # WorkingDirectory=~/Work. Medido 2026-08-23.
        if (cd "$HOME/projects/touring" && touring adw lint herdr-fanout-demo) >/dev/null 2>&1; then
            check_ok "adw-lint-herdr-fanout-demo" "exit 0"
        else
            check_fail "adw-lint-herdr-fanout-demo" "touring adw lint failed"
        fi
    else
        check_fail "missing-tool" "touring"
    fi
else
    check_warn "adw-lint-herdr-fanout-demo" "pending-artifact adw/herdr-fanout-demo.toml"
fi

# ── 9. pytest client/omarchy/tests ───────────────────────────────────────────
if need_tool pytest; then
    if PYTHONDONTWRITEBYTECODE=1 pytest -q "$OMARCHY_ROOT/tests" >/dev/null 2>&1; then
        check_ok "pytest-omarchy-tests" "all tests passed"
    else
        check_fail "pytest-omarchy-tests" "pytest -q client/omarchy/tests failed"
    fi
fi

# ── 10. ISO: existe, tamanho correto, gpg --verify ───────────────────────────
if [[ -f "$ISO_FILE" ]]; then
    actual_size=$(stat -c '%s' "$ISO_FILE" 2>/dev/null || printf '0')
    if [[ "$actual_size" -lt "$ISO_SIZE_EXPECTED" ]]; then
        check_fail "iso-incomplete" "size ${actual_size} < ${ISO_SIZE_EXPECTED} (download em andamento?)"
    elif [[ "$actual_size" -ne "$ISO_SIZE_EXPECTED" ]]; then
        check_fail "iso-size" "size ${actual_size} != ${ISO_SIZE_EXPECTED}"
    else
        check_ok "iso-size" "${actual_size} bytes OK"
        # GPG verify
        if command -v gpg >/dev/null 2>&1; then
            if [[ -f "$ISO_SIG" ]]; then
                gpg_out=$(gpg --verify "$ISO_SIG" "$ISO_FILE" 2>&1) && gpg_rc=0 || gpg_rc=$?
                if [[ "$gpg_rc" -eq 0 ]]; then
                    check_ok "iso-gpg-verify" "key $GPG_KEY OK"
                elif printf '%s' "$gpg_out" | grep -qi 'no public key'; then
                    # "Nao tenho a chave" NAO e' "a assinatura e' invalida" — sao
                    # afirmacoes diferentes, e so' a segunda justifica um FAIL. Esta
                    # maquina e' instalacao nova: o chaveiro nasceu vazio, a chave do
                    # Omarchy ficou no Pop. Medido 2026-08-23:
                    #   gpg: Can't check signature: No public key
                    check_warn "iso-gpg-verify" \
                        "cannot attest — public key $GPG_KEY not in this keyring; import with: gpg --recv-keys $GPG_KEY"
                else
                    check_fail "iso-gpg-verify" "gpg --verify failed (key $GPG_KEY): $(printf '%s' "$gpg_out" | tail -1)"
                fi
            else
                check_fail "iso-sig-missing" "$ISO_SIG not found"
            fi
        else
            check_fail "missing-tool" "gpg"
        fi
    fi
else
    check_fail "iso-missing" "$ISO_FILE not found"
fi

# ── 11. Pendrive (opcional) ───────────────────────────────────────────────────
if [[ -n "$OPT_USB" ]]; then
    if [[ -e "$OPT_USB" ]] && [[ -f "$ISO_FILE" ]]; then
        # Reading a block device needs root; without it cmp dies of EACCES and
        # the old branch reported "differ" — a permission error dressed as a
        # data divergence (caught 23/08 against a byte-identical pendrive).
        # Distinguish the three outcomes; use passwordless sudo when present.
        CMP=(cmp -n 67108864 "$ISO_FILE" "$OPT_USB")
        if [[ ! -r "$OPT_USB" ]] && sudo -n true 2>/dev/null; then
            CMP=(sudo -n "${CMP[@]}")
        fi
        if [[ ! -r "$OPT_USB" ]] && ! sudo -n true 2>/dev/null; then
            check_warn "usb-cmp-64mib" "cannot read $OPT_USB without root — run: sudo cmp -n 67108864 $ISO_FILE $OPT_USB"
        elif "${CMP[@]}" >/dev/null 2>&1; then
            check_ok "usb-cmp-64mib" "first 64 MiB match"
        else
            check_fail "usb-cmp-64mib" "first 64 MiB differ: $OPT_USB vs $ISO_FILE"
        fi
    else
        check_fail "usb-not-found" "$OPT_USB not found"
    fi
else
    check_warn "usb-not-given" "--usb not provided; skipping pendrive verification"
fi

# ── 12. efi/efibootmgr-v.txt salvo ──────────────────────────────────────────
EFI_FILE="$KIT_DIR/efi/efibootmgr-v.txt"
if [[ -f "$EFI_FILE" ]]; then
    check_ok "efi-efibootmgr-saved" "$EFI_FILE exists"
else
    check_fail "efi-efibootmgr-saved" "$EFI_FILE not found (run: efibootmgr -v > $EFI_FILE)"
fi

finish "$_PHASE"
