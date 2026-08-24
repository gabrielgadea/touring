#!/usr/bin/env bash
# herdr_branch.sh — execute one branch of a herdr-fanout ADW node.
# Called by the ADW `code` node inside the herdr-fanout fragment.
# Part of OmarchyOS Agêntico (S-0.7, P7.5).
#
# Usage: herdr_branch.sh <branch> <cwd> <prompt> <journal_dir>
#
# Environment (optional):
#   HERDR_BRANCH_MODEL   claude model   (default: sonnet)
#   HERDR_BRANCH_EFFORT  reasoning tier (default: medium)
#   HERDR_BRANCH_MODE    permission mode(default: plan)
#
# Exit codes:
#   0  — agent finished in state `done`; journal written
#   1  — agent blocked or timed out; journal written (pane read captured)
#
# The journal file <journal_dir>/<branch>.txt is ALWAYS written before exit
# (Law L3: artefato em disco é o único veredito). The ADW runner reads it.
#
# ── Four contract points of herdr 0.8.0, all measured on 2026-08-23 while the
#    --herdr button (S-5.6) was executed for the first time. This script carried
#    every one of them as a latent failure; it had never been run.
#
#  1. The pane id comes FROM `workspace create`. The previous version invented
#     `0:<branch>`, an address herdr never issues (real ones are `w1:p1`), so
#     `agent start` could only ever answer `agent_pane_not_found`.
#  2. `agent start` requires the pane to be AT its shell prompt, and its own
#     --timeout waits for AGENT readiness, not the shell: firing right after
#     `workspace create` yields `agent_pane_busy`. Poll for the prompt.
#  3. model/effort/permission-mode reach the agent only through herdr's `--`
#     passthrough. Without it herdr launches a bare `claude`.
#  4. The default permission mode here is `plan`, not acceptEdits, for two
#     reasons: an interactive agent that stops to ask for permission never
#     reaches `done` (it sits `blocked` until the 30-min timeout), and this
#     fragment fans several agents over the SAME working tree — concurrent
#     editors overwrite each other. That is the flow's own `when_not_to_use`.

set -euo pipefail

BRANCH="${1:?usage: herdr_branch.sh <branch> <cwd> <prompt> <journal_dir>}"
CWD="${2:?}"
PROMPT="${3:?}"
JOURNAL_DIR="${4:?}"
# MILLISECONDS. `herdr agent wait --timeout` takes ms, not seconds: the previous
# `1800` was commented "30 minutes per branch" and was in fact 1.8 SECONDS, which
# is why the `docs` branch reported `timeout` seconds into a run whose ceiling was
# supposed to be half an hour. Measured 2026-08-23 on the first real fan-out.
WAIT_TIMEOUT_MS=1800000   # 30 minutes per branch

MODEL="${HERDR_BRANCH_MODEL:-sonnet}"
EFFORT="${HERDR_BRANCH_EFFORT:-medium}"
MODE="${HERDR_BRANCH_MODE:-plan}"

# ---- Helpers -----------------------------------------------------------------
log() { printf '[herdr_branch] %s\n' "$*" >&2; }

write_journal() {
    local content="$1"
    mkdir -p "${JOURNAL_DIR}"
    printf '%s\n' "${content}" > "${JOURNAL_DIR}/${BRANCH}.txt"
}

# herdr agent names: lowercase letter first, then [a-z0-9_-], 1-32 chars.
herdr_agent_name() {
    local name
    name="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9_-' '-')"
    while [[ "$name" == [-_]* ]]; do name="${name#?}"; done
    [[ "$name" =~ ^[a-z] ]] || name="branch-$name"
    name="${name:0:32}"
    while [[ "$name" == *[-_] ]]; do name="${name%?}"; done
    printf '%s' "${name:-branch}"
}

wait_for_shell_prompt() {
    local pane="$1" deadline=$(( SECONDS + 20 ))
    while (( SECONDS < deadline )); do
        if [[ -n "$(herdr pane read "$pane" --source recent 2>/dev/null | tr -d '[:space:]')" ]]; then
            return 0
        fi
        sleep 0.25
    done
    return 1
}

AGENT="$(herdr_agent_name "${BRANCH}")"

# ---- Execution ---------------------------------------------------------------
log "Starting branch=${BRANCH} agent=${AGENT} cwd=${CWD} model=${MODEL} mode=${MODE}"

# 1. Create a Herdr workspace for this branch and take the pane id from it.
WS_JSON="$(herdr workspace create --cwd "${CWD}" --label "${BRANCH}" 2>&1)" || {
    log "workspace create failed"
    write_journal "ERROR: herdr workspace create failed for branch=${BRANCH}
${WS_JSON}"
    exit 1
}
PANE_ADDR="$(printf '%s' "${WS_JSON}" | jq -r '.result.root_pane.pane_id // empty' 2>/dev/null)"
WS_ID="$(printf '%s' "${WS_JSON}" | jq -r '.result.workspace.workspace_id // empty' 2>/dev/null)"
if [[ -z "${PANE_ADDR}" ]]; then
    log "could not read root_pane.pane_id from the create response"
    write_journal "ERROR: no pane_id in herdr workspace create response for branch=${BRANCH}
${WS_JSON}"
    exit 1
fi
log "workspace=${WS_ID} pane=${PANE_ADDR}"

# 2. Wait for the pane's shell, then start a Claude agent carrying its settings.
if ! wait_for_shell_prompt "${PANE_ADDR}"; then
    log "pane never reached a shell prompt"
    write_journal "ERROR: pane ${PANE_ADDR} never reached a shell prompt for branch=${BRANCH}"
    exit 1
fi
if ! herdr agent start "${AGENT}" --kind claude --pane "${PANE_ADDR}" \
        -- --model "${MODEL}" --effort "${EFFORT}" --permission-mode "${MODE}" 2>/dev/null; then
    log "agent start failed"
    write_journal "ERROR: herdr agent start failed for branch=${BRANCH} (agent=${AGENT} pane=${PANE_ADDR})"
    exit 1
fi

# 3. Send the prompt and wait for the agent to acknowledge receipt.
FULL_PROMPT="${PROMPT} (ramo: ${BRANCH})"
herdr agent prompt "${AGENT}" "${FULL_PROMPT}" --wait 2>/dev/null || {
    log "agent prompt failed"
    write_journal "ERROR: herdr agent prompt failed for branch=${BRANCH}"
    exit 1
}

# 4a. Observe the transition to `working` BEFORE waiting for a terminal state.
# Without this edge the terminal wait matches `idle` instantly — the agent is
# still idle when the prompt lands, so `wait --until idle` returns before any
# work happens and the journal captures the banner instead of the answer.
# Measured 2026-08-23: both branches settled in seconds with the pane showing
# context at 0.0%. A non-fatal timeout here means the agent answered faster than
# the poll; the terminal wait below is still the authority.
herdr agent wait "${AGENT}" --until 'working' --timeout 60000 >/dev/null 2>&1 || true

# 4b. Wait until the agent settles. `--until` is repeatable and we need BOTH
# terminal states: `done` is transient — an agent that answers and returns to its
# prompt is read as `idle` by herdr's detection (rule live_prompt_box), so waiting
# only for `done` is a coin flip on timing. Measured 2026-08-23: of two identical
# branches, `audit` was caught `done` and `docs` `idle`, and only the first passed.
# `herdr agent wait` also exits 0 while printing {"error":{"code":"timeout"}}, so
# the `||` fallback never fires — the JSON on stdout is the only signal.
# `done` quoted: bash reads the bare word as the loop keyword (shellcheck SC1010).
WAIT_OUT="$(herdr agent wait "${AGENT}" --until 'done' --until 'idle' --until 'blocked' \
    --timeout "${WAIT_TIMEOUT_MS}" 2>&1)" || true

# Normalise state: parse herdr output tolerantly.
STATE="unknown"
if printf '%s' "${WAIT_OUT}" | grep -qiE '"code"[[:space:]]*:[[:space:]]*"timeout"|\btimed out\b'; then
    STATE="timeout"
elif printf '%s' "${WAIT_OUT}" | grep -qiE '\bblocked\b'; then
    STATE="blocked"
elif printf '%s' "${WAIT_OUT}" | grep -qiE '\bdone\b'; then
    STATE="done"
elif printf '%s' "${WAIT_OUT}" | grep -qiE '\bidle\b'; then
    STATE="idle"
fi

log "agent final state=${STATE}"

# 5. Capture pane output regardless of outcome (Law L3).
PANE_OUT="$(herdr pane read "${PANE_ADDR}" --source recent-unwrapped --lines 120 2>/dev/null)" || PANE_OUT=""

JOURNAL_CONTENT="$(printf 'branch=%s agent=%s pane=%s state=%s\n---\n%s' \
    "${BRANCH}" "${AGENT}" "${PANE_ADDR}" "${STATE}" "${PANE_OUT}")"
write_journal "${JOURNAL_CONTENT}"

# 6. Exit code determines ADW node pass/fail. `idle` counts as success: the
# prompt was acknowledged (step 3 used --wait), so a return to the prompt box
# means the agent answered. Only `blocked`, `timeout` and `unknown` are failures.
if [ "${STATE}" = "done" ] || [ "${STATE}" = "idle" ]; then
    log "branch settled (${STATE}) — journal written to ${JOURNAL_DIR}/${BRANCH}.txt"
    exit 0
else
    log "branch ${STATE} — exiting 1 (journal written)"
    exit 1
fi
