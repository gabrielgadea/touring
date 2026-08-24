#!/usr/bin/env bash
# validate_p5.sh — Gate P5: Skills Deck (plugin + menu + executor)
# Roda no Omarchy após instalar o plugin e o menu.
# Uso: validate_p5.sh [--json]
set -euo pipefail

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
_PHASE="P5"
_JSON_MODE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)   _JSON_MODE=1; shift ;;
        --help|-h)
            printf 'Uso: validate_p5.sh [--json]\n'
            printf 'Gate P5: plugin gabriel.skills-deck, omarchy-skill-run, runs.log, menu.\n'
            exit 0 ;;
        *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
    esac
done

# shellcheck source=_validate_lib.sh
source "$SCRIPT_DIR/_validate_lib.sh"

# ── 1. omarchy plugin list contém gabriel.skills-deck ─────────────────────────
if need_tool omarchy; then
    if omarchy plugin list 2>/dev/null | grep -qi 'gabriel.skills-deck'; then
        check_ok "plugin-skills-deck-listed" "gabriel.skills-deck in omarchy plugin list"
    else
        check_fail "plugin-skills-deck-listed" "gabriel.skills-deck not in omarchy plugin list"
    fi
fi

# ── 2. omarchy-skill-run --dry-run exit 0 e imprime claude -p ────────────────
SKILL_RUN="$HOME/.local/bin/omarchy-skill-run"
if [[ -x "$SKILL_RUN" ]]; then
    dry_out=$("$SKILL_RUN" --dry-run TACO-cross-audit fable high acceptEdits "$HOME/projects/touring" 2>/dev/null || printf '')
    if printf '%s' "$dry_out" | grep -q 'claude -p'; then
        check_ok "omarchy-skill-run-dry" "prints 'claude -p' (dry-run ok)"
    else
        check_fail "omarchy-skill-run-dry" "dry-run did not print 'claude -p'"
    fi
else
    need_tool omarchy-skill-run
fi

# ── 3. ~/Work/runs.log: ultima linha tem exit=0 E artefato existente ─────────
# O validador do plano cobra as DUAS coisas ("tem exit=0 e caminho de artefato
# existente"); a versao anterior deste check so olhava o exit=0, e portanto
# aprovava uma linha com `artifact=-`. Metade de um contrato verificada e' um
# check que passa sem provar o que foi pedido. Medido 2026-08-23.
RUNS_LOG="$HOME/Work/runs.log"
if [[ -f "$RUNS_LOG" ]]; then
    last_line=$(tail -1 "$RUNS_LOG" 2>/dev/null || printf '')
    if ! printf '%s' "$last_line" | grep -q 'exit=0'; then
        check_fail "runs-log-last-exit0" "last line does not contain exit=0: $last_line"
    else
        last_artifact="${last_line##*artifact=}"; last_artifact="${last_artifact%% *}"
        if [[ "$last_line" != *artifact=* || "$last_artifact" == "-" ]]; then
            check_fail "runs-log-last-exit0" "exit=0 but no artifact path: $last_line"
        elif [[ -f "$last_artifact" ]]; then
            check_ok "runs-log-last-exit0" "exit=0, artifact exists: $last_artifact"
        else
            check_fail "runs-log-last-exit0" "exit=0 but artifact missing on disk: $last_artifact"
        fi
    fi
else
    check_fail "runs-log-exists" "$RUNS_LOG not found (run a skill first)"
fi

# ── 4. journalctl omarchy-shell skills-deck errors == 0 ──────────────────────
if command -v journalctl >/dev/null 2>&1; then
    # `-t` (SYSLOG IDENTIFIER), NAO `-u` (unit): o shell do Omarchy nao roda como
    # unit systemd — `omarchy-launch-shell` sobe o `quickshell` direto do
    # autostart do Hyprland, e ele loga sob o identifier `omarchy-shell`.
    # Medido 2026-08-23 no primeiro deploy real: `journalctl --user -u
    # omarchy-shell` devolve ZERO linhas (systemctl --user is-active
    # omarchy-shell = inactive), enquanto `-t omarchy-shell` traz o
    # "Local plugin changed, reloading: gabriel.skills-deck". Com `-u` o
    # contador de erros dava 0 porque o journal estava VAZIO — o check passava
    # sem provar nada. O probe antigo tambem nao ajudava: `--lines=1 && printf
    # yes` da "yes" mesmo vazio, porque journalctl sai 0 imprimindo
    # "-- No entries --". Agora exigimos linhas REAIS para poder atestar.
    # `x=$(...) || x=0`, NAO `x=$(... || printf '0')`: `grep -c` imprime a
    # contagem ("0") E sai com status 1 quando nao ha match, entao a forma com
    # `|| printf` ANEXA um segundo zero e a variavel vira "0\n0" — o `[[ -eq ]]`
    # seguinte morre com "arithmetic syntax error". Medido 2026-08-23.
    shell_lines=$(journalctl --user -t omarchy-shell -b --no-pager 2>/dev/null \
        | grep -ci 'skills-deck') || shell_lines=0
    if [[ "$shell_lines" -gt 0 ]]; then
        err_count=$(journalctl --user -t omarchy-shell -b --no-pager 2>/dev/null \
            | grep -i 'skills-deck' | grep -ci 'error') || err_count=0
        if [[ "$err_count" -eq 0 ]]; then
            check_ok "omarchy-shell-skills-deck-errors" "0 errors in $shell_lines skills-deck line(s)"
        else
            check_fail "omarchy-shell-skills-deck-errors" "$err_count error(s) in omarchy-shell journal"
        fi
    else
        check_warn "omarchy-shell-skills-deck-errors" "no skills-deck lines under identifier omarchy-shell — cannot attest"
    fi
else
    need_tool journalctl
fi

# ── 5. omarchy-menu.jsonc contém "skills.audit" ──────────────────────────────
MENU_INSTALLED="$HOME/.config/omarchy/extensions/omarchy-menu.jsonc"
if [[ -f "$MENU_INSTALLED" ]]; then
    if grep -q '"skills.audit"' "$MENU_INSTALLED"; then
        check_ok "menu-skills-audit" "skills.audit entry found in $MENU_INSTALLED"
    else
        check_fail "menu-skills-audit" "skills.audit not in $MENU_INSTALLED"
    fi
else
    check_fail "menu-installed" "$MENU_INSTALLED not found"
fi

# ── 6. keybind do skills deck registrado no Hyprland (S-5.5) ─────────────────
# O validador do plano e' `hyprctl binds | grep -i skills`. Casamos pela
# DESCRICAO ("Skills deck"), que e' o campo que o o.bind() propaga para o
# hyprctl (helpers.lua:84-86) — o comando em si vira dispatcher `__lua` e NAO
# aparece no dump, entao procurar pelo comando nunca casaria.
if command -v hyprctl >/dev/null 2>&1; then
    binds_out=$(hyprctl binds 2>/dev/null || printf '')
    if [[ -z "$binds_out" ]]; then
        check_warn "skills-deck-keybind" "hyprctl returned nothing (no live Hyprland session?)"
    else
        bind_lines=$(printf '%s\n' "$binds_out" | grep -ci 'description: Skills deck') || bind_lines=0
        if [[ "$bind_lines" -ge 1 ]]; then
            check_ok "skills-deck-keybind" "$bind_lines bind(s) with description 'Skills deck'"
        else
            check_fail "skills-deck-keybind" "no bind with description 'Skills deck' in hyprctl binds"
        fi
    fi
else
    check_warn "skills-deck-keybind" "hyprctl not available"
fi

# ── 7. runs.log: cada linha aponta para um artefato existente (S-5.2/S-5.6) ───
# O plano exige "caminho de artefato existente". O runner antigo deduzia o
# artefato varrendo o arquivo mais novo de ~/Work/artifacts, e escrevia `-`
# quando nao achava nada — um `artifact=-` passava sem que ninguem notasse.
# Aqui o gate cobra o contrato: todo `artifact=` referencia um arquivo real.
if [[ -f "$RUNS_LOG" ]]; then
    missing=0; total=0
    while IFS= read -r line; do
        [[ "$line" == *artifact=* ]] || continue
        total=$((total + 1))
        a="${line##*artifact=}"; a="${a%% *}"
        [[ -f "$a" ]] || missing=$((missing + 1))
    done < "$RUNS_LOG"
    if [[ "$total" -eq 0 ]]; then
        check_warn "runs-log-artifacts-exist" "no artifact= field in $RUNS_LOG yet"
    elif [[ "$missing" -eq 0 ]]; then
        check_ok "runs-log-artifacts-exist" "$total/$total artifact path(s) exist on disk"
    else
        # warn, nao fail: o contrato do plano e' a ULTIMA linha (check 3). Linhas
        # historicas podem ter sido escritas por uma versao anterior do runner,
        # cuja heuristica de artefato gravava `-`. Reprovar a fase por causa
        # delas puniria o registro correto de um defeito ja corrigido.
        check_warn "runs-log-artifacts-exist" "$missing of $total artifact path(s) missing (older runner versions?)"
    fi
fi

finish "$_PHASE"
