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
#
# Shell-data masking: delegated to _shell_mask.py (same directory).
# Semantic reference: touring_ceg::gateway::gate::mask_shell_data

set -uo pipefail
export LC_ALL=C

INPUT=$(cat)
CMD=$(printf '%s' "${INPUT}" | jq -r '.tool_input.command // ""' 2>/dev/null)
[[ -z "${CMD}" ]] && exit 0

# ── SHELL-DATA MASKING ──────────────────────────────────────────────────────────
# Delegate to _shell_mask.py (standalone Python module) so the masker logic lives
# in Python source that bash never parses.  Falls back to raw CMD on any error.
_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_MASKER_PY="${_SCRIPT_DIR}/_shell_mask.py"
if [[ -x "${_MASKER_PY}" ]]; then
    MASKED_CMD=$(printf '%s' "${CMD}" | python3 "${_MASKER_PY}" 2>/dev/null) || MASKED_CMD="${CMD}"
else
    MASKED_CMD="${CMD}"
fi

# Token checked on MASKED_CMD so a heredoc body cannot inject the bypass.
if printf '%s' "${MASKED_CMD}" | grep -qF 'GIT_DESTRUCTIVE_OK=1'; then
    exit 0
fi

# ── DESTRUCTIVE PATTERN DETECTION ──────────────────────────────────────────────
# All patterns run against MASKED_CMD (shell-data masked), not raw CMD.

check_pattern() {
    local pattern="$1"
    local reason="$2"
    if printf '%s' "${MASKED_CMD}" | grep -qE "${pattern}"; then
        REASON_ESC=$(printf '%s' "${reason}" | sed 's/\\/\\\\/g; s/"/\\"/g')
        printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "${REASON_ESC}"
        exit 0
    fi
}

check_pattern 'git[[:space:]]+(stash)([^[:alnum:]-]|$)' \
    "REGRA #11 v2: git stash é PROIBIDO como guarda de trabalho (destruiu 162 módulos 06/04/2026). Use: git checkout -b safety/<slug> && git add -A && git commit -m 'WIP safety pre-<op>'. Para usar mesmo assim: GIT_DESTRUCTIVE_OK=1 git stash"

check_pattern 'git[[:space:]]+reset[[:space:]].*(--hard|--merge|--keep)' \
    "REGRA #11 v2: git reset destrutivo requer ritual (MEDIR→SNAPSHOT→CONFIRMAR) + token GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+checkout[[:space:]]+(-f|--|\.)' \
    "REGRA #11 v2: git checkout destrutivo (descarta working tree) requer GIT_DESTRUCTIVE_OK=1"

check_pattern 'git[[:space:]]+switch[[:space:]]+--discard-changes' \
    "REGRA #11 v2: git switch --discard-changes descarta working tree — requer GIT_DESTRUCTIVE_OK=1"

# git restore (working tree): grep -E does not support lookahead (?!--staged).
# Two-condition check: deny if cmd has 'git restore' AND does NOT have 'git restore --staged'.
# Edge case: 'git restore --staged foo && git restore bar' → first condition matches,
# second also matches ('git restore --staged' is present) → not denied. Conservative trade-off.
# REGRA #21: pré-existente; now correctly fires.
if printf '%s' "${MASKED_CMD}" | grep -qE 'git[[:space:]]+restore[[:space:]]'; then
    if ! printf '%s' "${MASKED_CMD}" | grep -qE 'git[[:space:]]+restore[[:space:]]+--staged'; then
        REASON_ESC=$(printf '%s' "REGRA #11 v2: git restore (working tree) descarta mudanças — use --staged para só unstage, ou GIT_DESTRUCTIVE_OK=1" | sed 's/\\/\\\\/g; s/"/\\"/g')
        printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "${REASON_ESC}"
        exit 0
    fi
fi

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
