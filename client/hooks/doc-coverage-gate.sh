#!/usr/bin/env bash
# doc-coverage-gate.sh — PostToolUse hook: soft doc-coverage signal for Rust pub items.
#
# S41 (plan 2026-09-25): reads tool_input.file_path from stdin JSON (jq) instead of $1;
# communicates via hookSpecificOutput.additionalContext so the model actually sees the signal.
#
# Previous bug: registered as `command: doc-coverage-gate.sh "$ARGUMENTS.tool_input.file_path"`
# but Claude Code does NOT expand $ARGUMENTS in hook commands — $1 was always empty.
#
# Contract: PostToolUse (Edit, Write). Exit 0 always (soft signal only).

set -uo pipefail
export LC_ALL=C

# Read full stdin — required by hook protocol; extract file_path via jq.
INPUT=$(cat)
FILE_PATH=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null)

if [[ -z "${FILE_PATH}" || ! -f "${FILE_PATH}" ]]; then
    exit 0
fi

# Only inspect Touring workspace Rust source files.
case "${FILE_PATH}" in
    *crates/*/*.rs) ;;
    *) exit 0 ;;
esac

pub_count=$(grep -cE '^[[:space:]]*pub[[:space:]]+(fn|struct|enum|trait|const|static|type|use|mod)\b' "${FILE_PATH}" 2>/dev/null) || pub_count=0
doc_count=$(grep -cE '^[[:space:]]*///' "${FILE_PATH}" 2>/dev/null) || doc_count=0
pub_count=$(printf '%s' "${pub_count}" | tr -d '[:space:]')
doc_count=$(printf '%s' "${doc_count}" | tr -d '[:space:]')
[[ -z "${pub_count}" ]] && pub_count=0
[[ -z "${doc_count}" ]] && doc_count=0

# Skip trivially small files.
if [[ "${pub_count}" -le 3 ]]; then
    exit 0
fi

MSG=""
if [[ "${doc_count}" -eq 0 ]]; then
    MSG="[doc-coverage] $(basename "${FILE_PATH}"): ${pub_count} pub items, ZERO /// docs — run check-doc-coverage.sh"
elif [[ "${doc_count}" -gt 0 ]]; then
    ratio=$(( pub_count * 100 / doc_count ))
    if [[ "${ratio}" -gt 150 ]]; then
        MSG="[doc-coverage] $(basename "${FILE_PATH}"): ${pub_count} pub items, ${doc_count} /// docs (ratio ${ratio}%) — run check-doc-coverage.sh"
    fi
fi

if [[ -n "${MSG}" ]]; then
    # Emit via hookSpecificOutput.additionalContext so the model sees it (S41).
    MSG_ESC=$(printf '%s' "${MSG}" | sed 's/\\/\\\\/g; s/"/\\"/g')
    printf '{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"%s"}}\n' "${MSG_ESC}"
fi

exit 0
