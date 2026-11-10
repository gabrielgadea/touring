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
#        client/hooks/tests/test_s25_block_git_hooks_bypass.py  (D2: --no-verify & co. where the repo has active hooks)
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

# ── HOOK BYPASS (D2, 17/10/2026) ───────────────────────────────────────────────
# Skipping a repository's git hooks (`--no-verify`, `-c core.hooksPath=…`, `SKIP=…`, `HUSKY=0`, …) loses no data,
# so it does not need the REGRA #11 ritual; but where the repository has ACTIVE hooks it turns the checks the
# repository asked for off.  The masker notes every such form; whether it matters is a filesystem fact (does the
# target repo hold an executable hook?), resolved here — and ONLY here, so a command without a bypass pays nothing.
# It runs BEFORE the GIT_DESTRUCTIVE_OK token: that token is for lost data and must not also waive this check.
# The way out is a token of its own, GIT_HOOKS_BYPASS_OK=1 (code at any level, never data).
payload_cwd() {  # python3, not jq: the resolver below needs python3 anyway; empty = the hook's own cwd
    local dir
    dir=$(printf '%s' "${INPUT}" | python3 -c 'import json, sys; print(json.load(sys.stdin).get("cwd") or "", end="")' 2>/dev/null)
    printf '%s' "${dir}"
}

BYPASS_NOTES=$(printf '%s\n' "${NOTES}" | grep -aF 'BLOCK_GIT_NOTE:hooks-bypass:' || true)
if [[ -n "${BYPASS_NOTES}" ]] && ! has_str 'GIT_HOOKS_BYPASS_OK=1' "${MASKED_CMD}"; then
    if ! BYPASS_MSG=$(printf '%s\n' "${BYPASS_NOTES}" | python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import _shell_mask; sys.exit(_shell_mask.resolve_main(sys.argv[2]))' "${_SCRIPT_DIR}" "$(payload_cwd)" 2>/dev/null); then
        deny "Contorno de hooks do git: o comando usa uma forma que pula hooks (--no-verify, core.hooksPath, SKIP, HUSKY…) e o guard não conseguiu verificar se o repositório tem hooks ativos. Rode sem o contorno, ou use GIT_HOOKS_BYPASS_OK=1 neste comando se for intencional."
    fi
    [[ -n "${BYPASS_MSG}" ]] && deny "${BYPASS_MSG}"
fi

# The token counts when it is CODE at any level — the outer command or a nested one — never when it is
# data (a quoted literal, a heredoc body).
if has_str 'GIT_DESTRUCTIVE_OK=1' "${MASKED_CMD}"; then
    exit 0
fi

while IFS= read -r note; do
    [[ -z "${note}" ]] && continue
    kind="${note#*BLOCK_GIT_NOTE:}"
    case "${kind}" in
        hooks-bypass:*) ;; # judged above (only a bypass against a repo with active hooks is a violation)
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

# git accepts any unambiguous PREFIX of a long option (`--forc` runs `--force`): a word names
# <full> when its part before `=` is a prefix of <full> at least <min> characters long.
# Mirror: `names_long` in crates/touring-hooks-shared/src/command_policy/git/direct.rs.
names_long() {
    local w="${1%%=*}"
    ((${#w} >= $3)) && [[ "$2" == "${w}"* ]]
}

# A short-option cluster (`-qf`) holds <letter> before any letter of <valued>, which takes the
# rest of the cluster as its value (getopt: `-bf` is `-b f`). Mirror: `cluster_has`.
cluster_has() {
    local w="$1" i ch
    [[ "${w}" == -[!-]* ]] || return 1
    w="${w:1}"
    for ((i = 0; i < ${#w}; i++)); do
        ch="${w:i:1}"
        [[ "${ch}" == "$2" ]] && return 0
        [[ "$3" == *"${ch}"* ]] && return 1
    done
    return 1
}

# The words of an occurrence after `git <subcommand>`, up to `--` (exclusive), one per line.
# `read -a`, not $(…): an unquoted expansion would glob `*`.
options_of() {
    local -a words
    read -ra words <<<"$1"
    local word
    for word in "${words[@]:2}"; do
        [[ "${word}" == "--" ]] && break
        printf '%s\n' "${word}"
    done
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

# `--` must stand alone: `--detach`/`--quiet` as the first option used to match the bare `--` (10/11/2026).
check_pattern 'git[[:space:]]+checkout[[:space:]]+(-f|--([[:space:]]|$)|\.)' \
    "REGRA #11 v2: git checkout destrutivo (descarta working tree) requer GIT_DESTRUCTIVE_OK=1"

# `git checkout <ref> -- <path>` overwrites <path> from <ref>: a `--` with a word after it, in any
# position — the pattern above read only the first option (09/11/2026). Mirror: git/direct.rs.
check_pattern 'git[[:space:]]+checkout[[:space:]][^;&|)]*[[:space:]]--[[:space:]]+[^[:space:];&|)]' \
    "REGRA #11 v2: git checkout <ref> -- <caminho> sobrescreve o working tree — requer GIT_DESTRUCTIVE_OK=1"

# ...and any option before `--` that forces, in any position: `-f` in a cluster (`-qf`; `-b`/`-B`
# take the rest as a branch name), `--force` or a prefix of it, and a bare `.` (every path).
while IFS= read -r occ; do
    [[ -z "${occ}" ]] && continue
    while IFS= read -r word; do
        if [[ "${word}" == "." ]] || { [[ "${word}" == --* ]] && names_long "${word}" --force 3; } \
            || cluster_has "${word}" f bB; then
            deny "REGRA #11 v2: git checkout destrutivo (descarta working tree) requer GIT_DESTRUCTIVE_OK=1"
        fi
    done < <(options_of "${occ}")
done < <(occurrences checkout)

# A bare `git checkout <word>` (no `--`) is a path whenever <word> is not a ref, and checking out a
# path discards its uncommitted changes — the 01:04 incident. Gabriel's decision (10/11/2026): copy
# git. The first operand that resolves as a commit (or as a remote branch git would track) is a ref,
# so a branch named like a file is switched to; every other operand, and every operand after the
# ref, is a path. Unresolvable (no repo, a quoted word the masker blanked) = path: fail-closed.
# Mirror: `bare_checkout_paths` in crates/touring-hooks-shared/src/command_policy/git/direct.rs.
is_ref() {
    local dir="$1" word="$2"
    [[ "${word}" == "-" ]] && return 0
    [[ -z "${word}" || "${word}" == *'"'* || "${word}" == *"'"* ]] && return 1
    git -C "${dir}" rev-parse --verify --quiet --end-of-options "${word}^{commit}" >/dev/null 2>&1 && return 0
    [[ -n "$(git -C "${dir}" for-each-ref --count=1 --format=x "refs/remotes/*/${word}" 2>/dev/null)" ]]
}

CHECKOUT_DIR=""
while IFS= read -r occ; do
    [[ -z "${occ}" ]] && continue
    has_re "[[:space:]]--([[:space:]]|$)" "${occ}" && continue  # `--` forms are judged above
    operands=()
    creates=0
    while IFS= read -r word; do
        if [[ "${word}" == --* ]]; then
            { names_long "${word}" --orphan 3 || names_long "${word}" --pathspec-from-file 3; } && creates=1
            names_long "${word}" --pathspec-from-file 3 && creates=2
        elif [[ "${word}" == -?* ]]; then
            cluster_has "${word}" b "" || cluster_has "${word}" B "" && creates=1
        else
            operands+=("${word}")
        fi
    done < <(options_of "${occ}")
    ((creates == 2)) && deny "REGRA #11 v2: git checkout --pathspec-from-file descarta as mudanças dos caminhos listados — requer GIT_DESTRUCTIVE_OK=1"
    ((creates == 1 || ${#operands[@]} == 0)) && continue
    [[ -z "${CHECKOUT_DIR}" ]] && CHECKOUT_DIR="$(payload_cwd)" && CHECKOUT_DIR="${CHECKOUT_DIR:-${PWD}}"
    first=1
    for word in "${operands[@]}"; do
        if ((first)) && is_ref "${CHECKOUT_DIR}" "${word}"; then
            first=0
            continue
        fi
        deny "REGRA #11 v2: git checkout ${word} descarta as mudanças não salvas de '${word}' — não é branch nem commit em ${CHECKOUT_DIR}, então o git o lê como caminho. Trocar de branch: git switch <branch>. Descartar de propósito: GIT_DESTRUCTIVE_OK=1 <o mesmo comando>."
    done
done < <(occurrences checkout)

check_pattern 'git[[:space:]]+switch[[:space:]]+--discard-changes' \
    "REGRA #11 v2: git switch --discard-changes descarta working tree — requer GIT_DESTRUCTIVE_OK=1"

# git restore: only an unstage (--staged / -S, and no --worktree / -W) is ADITIVA; any other
# occurrence rewrites the working tree — even when another occurrence in the same line unstages.
# Options only before `--`, as git reads them (09/11/2026): a long prefix counts (`--wor` is
# `--worktree`, `--st` is `--staged`; `--s` is ambiguous with `--source`), a cluster counts
# (`-SW`; `-s` takes the rest as its source), and a path named `-S` after `--` unstages nothing.
# Mirror: `restore_is_destructive` in git/direct.rs.
while IFS= read -r occ; do
    [[ -z "${occ}" ]] && continue
    staged=0 worktree=0 negated=0
    while IFS= read -r word; do
        # `--no-<option>` (any prefix: `--no-st`) undoes an earlier option in git's parser, so
        # any negation is destructive (fail-closed) instead of modelling order and prefixes.
        if [[ "${word}" == --no-* ]]; then
            negated=1
        elif [[ "${word}" == --* ]]; then
            names_long "${word}" --staged 4 && staged=1
            names_long "${word}" --worktree 3 && worktree=1
        else
            cluster_has "${word}" S s && staged=1
            cluster_has "${word}" W s && worktree=1
        fi
    done < <(options_of "${occ}")
    if ((!staged || worktree || negated)); then
        deny "REGRA #11 v2: git restore (working tree) descarta mudanças — use só --staged/-S para unstage, ou GIT_DESTRUCTIVE_OK=1"
    fi
done < <(occurrences restore)

# git clean is destructive unless it is a DRY RUN (fail-closed, 09/11/2026): the old pattern read
# only the first option and only `-f`/`-d`, so `git clean --force`, `-x -f` and `--forc` passed.
# A dry run is a real `-n` (in a cluster, before `-e`, which takes the rest as its pattern) or
# `--dry-run`/a prefix of it — and the word after a bare `-e`/`--exclude` is a pattern, never
# an option. Mirror: `clean_is_dry_run` in git/direct.rs.
while IFS= read -r occ; do
    [[ -z "${occ}" ]] && continue
    dry=0 value_next=0 negated=0
    while IFS= read -r word; do
        if ((value_next)); then
            value_next=0
            continue
        fi
        if [[ "${word}" == --no-* ]]; then
            negated=1 # `--no-dry-run`/`--no-d` undoes `-n`: fail-closed on any negation
        elif [[ "${word}" == --* ]]; then
            names_long "${word}" --dry-run 3 && dry=1
            [[ "${word}" != *=* ]] && names_long "${word}" --exclude 3 && value_next=1
        elif [[ "${word}" == -[!-]* ]]; then
            cluster_has "${word}" n e && dry=1
            letters="${word:1}"
            # the FIRST `e` is the cluster's last letter: its pattern is the next word
            [[ "${letters}" == *e* && "${letters%%e*}e" == "${letters}" ]] && value_next=1
        fi
    done < <(options_of "${occ}")
    ((dry && !negated)) || deny "REGRA #11 v2: git clean apaga arquivos não rastreados — rode com -n primeiro (dry-run) e depois GIT_DESTRUCTIVE_OK=1"
done < <(occurrences clean)

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
