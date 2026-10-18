#!/usr/bin/env bash
# block_git.sh — PreToolUse Bash hook: git DESTRUTIVA guard (REGRA #11 v2).
#
# Source of truth: client/hooks/block_git.sh (S48 versioning).
# ~/.claude/hooks/block_git.sh is a symlink to this file.
#
# Canonical reference: ~/.claude/rules/git-safety-protocol.md
# Tests: client/hooks/tests/test_s25_block_git_delete.py
#        client/hooks/tests/test_s25_block_git_data_masking.py
#        client/hooks/tests/test_s25_block_git_restore.py
#        client/hooks/tests/test_s25_block_git_occurrences.py
#
#        client/hooks/tests/test_s25_block_git_nested.py  (nested forms: bash -c, eval, heredoc, xargs, …)
#        client/hooks/tests/test_s25_block_git_no_jq.py   (jq absent)
#
# Shell-data masking AND nested-code expansion (bash -c, eval, heredoc into a shell, xargs, wrappers,
# interpreter exec calls): delegated to _shell_mask.py (same directory).
# Semantic reference: touring_ceg::gateway::gate::mask_shell_data

set -uo pipefail
export LC_ALL=C

INPUT=$(cat)

warn() { printf 'block_git: %s\n' "$*" >&2; }

# has_re <ere> <text> / has_str <fixed> <text>: does the text hold a match?  NEVER `printf | grep -q` under pipefail:
# grep -q leaves on its first match, the still-writing printf dies of SIGPIPE (141) and the pipeline reads as
# "no match" for any input over the 64 KB pipe buffer — a destructive command at the start of a big command was
# waved through (measured 17/10/2026: 198 KB, allowed).  Without -q grep reads to EOF, printf finishes, the
# status is grep's.  (A here-string would also work, but bash writes big ones to a temp file: a read-only
# TMPDIR would make every check fail open.)
has_re() { printf '%s' "$2" | grep -E -- "$1" >/dev/null; }
has_str() { printf '%s' "$2" | grep -F -- "$1" >/dev/null; }

# Same JSON-escape the deny output has always used; needs no jq, so it also works on the no-jq path.
deny() {
    REASON_ESC=$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g')
    printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "${REASON_ESC}"
    exit 0
}

# ── READ THE COMMAND ───────────────────────────────────────────────────────────
# jq first, python3 second, the raw JSON last.  A missing reader used to leave CMD empty and the hook
# exit 0 in silence: a guard that cannot read its input guarded nothing, and said nothing (17/10/2026).
CMD=""
READ_FAILED=""
if ! command -v jq >/dev/null 2>&1; then
    READ_FAILED="jq ausente do PATH"
elif ! CMD=$(printf '%s' "${INPUT}" | jq -r '.tool_input.command // ""' 2>/dev/null); then
    READ_FAILED="jq não conseguiu ler o JSON de entrada"
fi
if [[ -n "${READ_FAILED}" ]]; then
    CMD=""
    if command -v python3 >/dev/null 2>&1; then
        CMD=$(printf '%s' "${INPUT}" | python3 -c 'import json, sys; print(json.load(sys.stdin).get("tool_input", {}).get("command") or "", end="")' 2>/dev/null) || CMD=""
    fi
    if [[ -n "${CMD}" ]]; then
        warn "${READ_FAILED}; o comando foi lido com python3 e o guard segue completo"
    else
        # Nothing could parse the input: judge the raw JSON text, coarsely.  A false positive costs one
        # retry with GIT_DESTRUCTIVE_OK=1; a silent exit 0 would cost the work.
        if has_str 'GIT_DESTRUCTIVE_OK=1' "${INPUT}"; then
            warn "${READ_FAILED}; entrada bruta com GIT_DESTRUCTIVE_OK=1: liberado"
            exit 0
        fi
        if has_re 'git.{0,160}(reset|clean|stash|rebase|restore|filter-branch|filter-repo|reflog|--discard-changes)|git.{0,160}checkout.{0,80}( -f| --force| -- |\.)|git.{0,160}push.{0,160}( -f|--force|--delete| -d| :)|git.{0,160}branch.{0,80}-D' "${INPUT}"; then
            warn "${READ_FAILED}; a entrada bruta cita git destrutivo: negado por precaução"
            deny "REGRA #11 v2: o guard não conseguiu ler o comando (${READ_FAILED}) e o texto bruto cita git destrutivo (reset/clean/stash/rebase/restore/checkout --/push --force/branch -D). Instale o jq (ou o python3) para o guard voltar a julgar com precisão, ou use GIT_DESTRUCTIVE_OK=1 neste comando."
        fi
        warn "${READ_FAILED}; sem git destrutivo no texto bruto: liberado SEM o guard completo — instale o jq"
        exit 0
    fi
fi
[[ -z "${CMD}" ]] && exit 0

# ── SHELL-DATA MASKING + NESTED CODE ───────────────────────────────────────────
# _shell_mask.py (standalone Python, so bash never parses it) prints one line per VIEW of the command:
# the command with data masked out, then — recursively — every text handed to something that executes it
# (`bash -c`, `eval`, a heredoc into a shell, `$(...)`, `xargs`/`--`/`env`/`timeout` launchers, an
# interpreter's `os.system(...)`).  A trailing RS-prefixed NOTE line carries what no text pattern can
# express (xargs-supplied git arguments, an exhausted work budget).  Every pattern below runs on ALL views.
# `readlink -f`: Claude Code runs ~/.claude/hooks/block_git.sh, a SYMLINK — the masker lives next to the
# real file, not next to the link. Without it the masker was never found in production (26/09/2026).
_SCRIPT_DIR="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
_MASKER_PY="${_SCRIPT_DIR}/_shell_mask.py"
# Imported, not run as a script: a script is recompiled on every call (~25 ms for this module), an import is
# served from __pycache__ (it falls back to compiling when the directory is not writable).
if [[ -x "${_MASKER_PY}" ]] && MASKED_CMD=$(printf '%s' "${CMD}" | python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import _shell_mask; sys.exit(_shell_mask.main())' "${_SCRIPT_DIR}"); then
    :
else
    warn "mascarador indisponível (${_MASKER_PY}): varrendo o comando bruto, pode haver falso positivo"
    MASKED_CMD="${CMD}"
fi

NOTE_MARK=$'\x1e'
NOTES=$(printf '%s\n' "${MASKED_CMD}" | grep -aF "${NOTE_MARK}BLOCK_GIT_NOTE:" || true)
MASKED_CMD=$(printf '%s\n' "${MASKED_CMD}" | grep -avF "${NOTE_MARK}" || true)

# The token counts when it is CODE at any level — the outer command or a nested one — never when it is
# data (a quoted literal, a heredoc body).
if has_str 'GIT_DESTRUCTIVE_OK=1' "${MASKED_CMD}"; then
    exit 0
fi

while IFS= read -r note; do
    [[ -z "${note}" ]] && continue
    kind="${note#*BLOCK_GIT_NOTE:}"
    case "${kind}" in
        xargs-git:*)
            deny "REGRA #11 v2: 'xargs git ${kind#xargs-git:}' recebe os argumentos do stdin, invisíveis ao guard — a forma destrutiva não é verificável. Escreva os argumentos no próprio comando, ou use GIT_DESTRUCTIVE_OK=1 neste comando." ;;
        budget:*)
            deny "REGRA #11 v2: o comando aninha shells além do orçamento do guard (${kind#budget:}) e não é verificável. Simplifique (script em arquivo, menos camadas) ou use GIT_DESTRUCTIVE_OK=1 neste comando." ;;
        *)
            deny "REGRA #11 v2: forma aninhada não verificável (${kind}). Use GIT_DESTRUCTIVE_OK=1 neste comando se for intencional." ;;
    esac
done < <(printf '%s\n' "${NOTES}")

# ── DESTRUCTIVE PATTERN DETECTION ──────────────────────────────────────────────
# All patterns run against MASKED_CMD (every view: shell-data masked, nested code included), not raw CMD.

check_pattern() {
    if has_re "$1" "${MASKED_CMD}"; then
        deny "$2"
    fi
}

# Each git occurrence is judged on its own (26/09/2026): a verdict over the WHOLE command let one
# harmless form launder a destructive one in the same line, and blocked reads the protocol frees.
# An occurrence runs from the subcommand to the next shell separator.
occurrences() {
    printf '%s' "${MASKED_CMD}" | grep -oE "git[[:space:]]+$1\\b[^;&|)]*" || true
}

# git stash: only `list` and `show` are LEITURA (REGRA #11 v2, "livres, sempre"); every other
# form — bare `git stash` included, which is `push` — mutates.
while IFS= read -r occ; do
    [[ -z "${occ}" ]] && continue
    case "$(printf '%s' "${occ}" | awk '{print $3}')" in
        list | show) ;;
        *) deny "REGRA #11 v2: git stash é PROIBIDO como guarda de trabalho (destruiu 162 módulos 06/04/2026). Leitura livre: git stash list / git stash show. Use: git checkout -b safety/<slug> && git add -A && git commit -m 'WIP safety pre-<op>'. Para usar mesmo assim: GIT_DESTRUCTIVE_OK=1 git stash" ;;
    esac
done < <(occurrences stash)

check_pattern 'git[[:space:]]+reset[[:space:]].*(--hard|--merge|--keep)' \
    "REGRA #11 v2: git reset destrutivo requer ritual (MEDIR→SNAPSHOT→CONFIRMAR) + token GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+checkout[[:space:]]+(-f|--|\.)' \
    "REGRA #11 v2: git checkout destrutivo (descarta working tree) requer GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+switch[[:space:]]+--discard-changes' \
    "REGRA #11 v2: git switch --discard-changes descarta working tree — requer GIT_DESTRUCTIVE_OK=1"

# git restore: only an unstage (--staged / -S, and no --worktree / -W) is ADITIVA; any other
# occurrence rewrites the working tree — even when another occurrence in the same line unstages.
while IFS= read -r occ; do
    [[ -z "${occ}" ]] && continue
    if ! printf '%s ' "${occ}" | grep -qE '[[:space:]](--staged|-S)[[:space:]]' \
        || printf '%s ' "${occ}" | grep -qE '[[:space:]](--worktree|-W)[[:space:]]'; then
        deny "REGRA #11 v2: git restore (working tree) descarta mudanças — use só --staged/-S para unstage, ou GIT_DESTRUCTIVE_OK=1"
    fi
done < <(occurrences restore)

check_pattern 'git[[:space:]]+clean[[:space:]]+-[[:alnum:]]*[fd]' \
    "REGRA #11 v2: git clean apaga arquivos não rastreados — rode com -n primeiro (dry-run) e depois GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+push[[:space:]]+.*(-f|--force|--force-with-lease)' \
    "REGRA #11 v2: git push --force requer aprovação explícita de Gabriel + GIT_DESTRUCTIVE_OK=1. NUNCA em main/master."

check_pattern 'git[[:space:]]+push[[:space:]]+.*(--delete|-d[[:space:]])' \
    "REGRA #11 v2: git push --delete apaga ref remota permanentemente — requer ritual (MEDIR→SNAPSHOT→CONFIRMAR) + GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+push[[:space:]]+[^[:space:]]+[[:space:]]+:[^[:space:]]' \
    "REGRA #11 v2: git push <remote> :<ref> apaga ref remota (refspec vazio à esquerda) — requer GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+rebase([[:space:]]|$)' \
    "REGRA #11 v2: git rebase reescreve histórico — requer snapshot WIP + GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+branch[[:space:]]+-D' \
    "REGRA #11 v2: git branch -D deleta permanentemente — use -d primeiro; se necessário: GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+(filter-branch|filter-repo)' \
    "REGRA #11 v2: reescrita de histórico publicado é PROIBIDA sem ordem de Gabriel + GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+reflog[[:space:]]+expire' \
    "REGRA #11 v2: git reflog expire apaga a rede de segurança — requer GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+gc[[:space:]]+.*--prune=now' \
    "REGRA #11 v2: git gc --prune=now descarta objetos irrecuperáveis — requer GIT_DESTRUCTIVE_OK=1"

exit 0
