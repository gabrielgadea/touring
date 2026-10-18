#!/usr/bin/env bash
# touring-quality-block-all.sh — PreToolUse: 6 BLOCK dims gate (S46 delta-mode).
#
# S46 (plan 2026-09-25, D6 aprovada):
#   • DELTA mode for existing files: deny only when a P0 dim WORSENS (≥0.5 → <0.5).
#   • ABSOLUTE mode for new files (Write on non-existent path): current behavior.
#   • Pragma: "# pragma: allowlist secret" on same line suppresses F2.4 for that finding.
#   • Cache key: SHA256 of content hash, not mktemp path (fixes: cache never hit).
#   • Lock budget: TOURING_BLOCK_ALL_LOCK_WAIT_SECS (else TOURING_QUALITY_LOCK_WAIT_SECS, else 20),
#     capped at 25s (< the 30s hook timeout) and handed to the scorer's lock wrapper.
#
# Fail-OPEN: any failure path logs WARN and exits 0 (never blocks session).
# BLOCK mechanism: print JSON {"hookSpecificOutput":{"permissionDecision":"deny",...}} + exit 0.

set -uo pipefail
export LC_ALL=C
LOG=/tmp/hook_invocations.log
log() { echo "[$(date +%H:%M:%S)] $*" >> "$LOG" 2>/dev/null; }
log "touring-quality-block-all.sh invoked PID=$$"

INPUT=$(cat)

TQ_BIN=$(command -v touring-quality 2>/dev/null || true)
{ [ -n "$TQ_BIN" ] && [ -x "$TQ_BIN" ]; } || TQ_BIN="$HOME/projects/touring/crates/touring-quality/target/release/touring-quality"
[ -x "$TQ_BIN" ] || TQ_BIN="$HOME/projects/touring/crates/touring-quality/target/debug/touring-quality"
if [ ! -x "$TQ_BIN" ]; then log "WARN fail-open: touring-quality binary not found"; exit 0; fi
if ! command -v jq >/dev/null 2>&1; then log "WARN fail-open: jq not found"; exit 0; fi
if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1; then
    log "WARN fail-open: sha256sum/shasum not found"; exit 0
fi

FILE_PATH=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null)
TOOL_NAME=$(printf '%s' "$INPUT" | jq -r '.tool_name // ""' 2>/dev/null)
NEW_CONTENT=$(printf '%s' "$INPUT" | jq -r '.tool_input.content // ""' 2>/dev/null)

[ -z "$FILE_PATH" ] && exit 0

# Extension filter: only score source files (not .md/.json/.toml/.yaml/.txt).
case "$FILE_PATH" in
    *.rs|*.py|*.ts|*.tsx|*.js|*.jsx|*.c|*.cpp|*.cc|*.cxx|*.h|*.hpp|*.kt|*.swift|*.go|*.java) ;;
    *) exit 0 ;;
esac

# ── Build candidate file in temp dir (content hash-keyed cache) ───────────────
CACHE_DIR="${TMPDIR:-/tmp}/tq_cache"
mkdir -p "$CACHE_DIR"

# For Edit: apply the new_string patch to arrive at proposed content.
if [ "$TOOL_NAME" = "Edit" ] && [ -f "$FILE_PATH" ]; then
    OLD_STRING=$(printf '%s' "$INPUT" | jq -r '.tool_input.old_string // ""' 2>/dev/null)
    NEW_STRING=$(printf '%s' "$INPUT" | jq -r '.tool_input.new_string // ""' 2>/dev/null)
    # Build proposed content via Python string replacement (exact match, first occurrence).
    NEW_CONTENT=$(python3 -c "
import sys
with open(sys.argv[1]) as f:
    content = f.read()
old = sys.argv[2]
new = sys.argv[3]
if old in content:
    print(content.replace(old, new, 1), end='')
else:
    print(content, end='')
" "$FILE_PATH" "$OLD_STRING" "$NEW_STRING" 2>/dev/null || cat "$FILE_PATH" 2>/dev/null || true)
fi

# Cache key: the PATH and the content (S46b). The same bytes score differently at different
# paths (a tests/ file vs production), so a content-only key served one path's score to another.
if command -v sha256sum >/dev/null 2>&1; then
    CONTENT_HASH=$(printf '%s\n%s' "$FILE_PATH" "$NEW_CONTENT" | sha256sum | cut -c1-16)
else
    CONTENT_HASH=$(printf '%s\n%s' "$FILE_PATH" "$NEW_CONTENT" | shasum -a 256 | cut -c1-16)
fi

CACHE_FILE="$CACHE_DIR/${CONTENT_HASH}.json"
CACHE_TTL=120  # seconds

if [ -f "$CACHE_FILE" ]; then
    # Cache hit: check freshness.
    CACHE_AGE=$(( $(date +%s) - $(stat -c%Y "$CACHE_FILE" 2>/dev/null || echo 0) ))
    if [ "$CACHE_AGE" -lt "$CACHE_TTL" ]; then
        log "cache-hit $CONTENT_HASH"
        SCORE_JSON=$(cat "$CACHE_FILE")
    else
        rm -f "$CACHE_FILE"
        SCORE_JSON=""
    fi
else
    SCORE_JSON=""
fi

if [ -z "$SCORE_JSON" ]; then
    # Write proposed content to a named temp file for scoring.
    LOCK_WAIT="${TOURING_BLOCK_ALL_LOCK_WAIT_SECS:-${TOURING_QUALITY_LOCK_WAIT_SECS:-20}}"
    case "$LOCK_WAIT" in ''|*[!0-9]*) LOCK_WAIT=20 ;; esac
    # Cap lock wait at 25s (< timeout of 30s).
    [ "$LOCK_WAIT" -gt 25 ] && LOCK_WAIT=25
    # The budget reaches the scorer through the wrapper's own variable: the PATH `touring-quality`
    # is the singleton-lock wrapper (scripts/touring-quality-score), whose default wait is 900s, a
    # patience meant for the judge. Until 17/10/2026 this value was computed and never passed, so a
    # directory score holding the lock (165-184s for ~/projects/analise) parked every Write/Edit
    # here until the 30s hook timeout. The hook's knob has its own name for the same reason: one
    # name for two quantities (hook budget < 30s, judge patience 900s) hid the bug from its test.
    export TOURING_QUALITY_LOCK_WAIT_SECS="$LOCK_WAIT"

    # S46b: score the candidate at a path that MIRRORS the original — the classification
    # (a tests/ file, a scripts/ program, the extension) depends on the path, and the delta
    # compares it with the on-disk original. A bare mktemp path turned every edit of a test
    # file holding vulnerable fixtures into a false P0 "regression".
    TMPFILE="$CACHE_DIR/tree/${CONTENT_HASH}${FILE_PATH}"
    mkdir -p "$(dirname "$TMPFILE")"
    printf '%s' "$NEW_CONTENT" > "$TMPFILE"

    TQ_RC=0
    if [ -n "$NEW_CONTENT" ]; then
        SCORE_JSON=$("$TQ_BIN" score "$TMPFILE" --format json 2>/dev/null) || TQ_RC=$?
    elif [ -f "$FILE_PATH" ]; then
        SCORE_JSON=$("$TQ_BIN" score "$FILE_PATH" --format json 2>/dev/null) || TQ_RC=$?
    fi
    rm -f "$TMPFILE"
    # empty mirror directories only (non-recursive: stops at the first non-empty parent)
    rmdir -p --ignore-fail-on-non-empty "$(dirname "$TMPFILE")" 2>/dev/null || true

    if [ -z "$SCORE_JSON" ]; then
        # 75 is the wrapper's "busy": another score held the lock past LOCK_WAIT, nothing measured.
        if [ "$TQ_RC" -eq 75 ]; then
            log "WARN fail-open: quality lock busy for more than ${LOCK_WAIT}s, $FILE_PATH not scored"
        else
            log "WARN fail-open: empty score JSON for $FILE_PATH (rc=$TQ_RC)"
        fi
        exit 0
    fi
    printf '%s' "$SCORE_JSON" > "$CACHE_FILE"
fi

# ── Delta mode: for existing files, also score on-disk baseline ───────────────
BASELINE_JSON=""
IS_NEW_FILE=false
if [ ! -f "$FILE_PATH" ]; then
    IS_NEW_FILE=true
else
    BASELINE_JSON=$("$TQ_BIN" score "$FILE_PATH" --format json 2>/dev/null || true)
fi

# ── Pragma: scan proposed content for allowlist pragmas on secret lines ────────
# Lines ending with "# pragma: allowlist secret" suppress F2.4 findings on that line.
has_pragma_for_dim() {
    local dim="$1"
    if [ "$dim" = "F2.4" ] && [ -n "$NEW_CONTENT" ]; then
        if printf '%s' "$NEW_CONTENT" | grep -qF '# pragma: allowlist secret'; then
            return 0
        fi
    fi
    return 1
}

# ── Evaluate dims ──────────────────────────────────────────────────────────────
BLOCKER=""
for dim in F2.1 F2.4 F2.5 F2.6 F4.3 F4.5; do
    key=${dim//./_}

    NEW_SCORE=$(printf '%s' "$SCORE_JSON" | jq -r ".dimensions.${key}.value // 1.0" 2>/dev/null || echo 1.0)
    [ -z "$NEW_SCORE" ] && NEW_SCORE=1.0

    # Check pragma suppression.
    if has_pragma_for_dim "$dim"; then
        log "pragma-suppressed $dim for $FILE_PATH"
        continue
    fi

    if $IS_NEW_FILE; then
        # Absolute mode for new files: deny if score < 0.5.
        if awk -v s="$NEW_SCORE" 'BEGIN{exit !(s+0 < 0.5)}'; then
            EVIDENCE=$(printf '%s' "$SCORE_JSON" | jq -r ".dimensions.${key}.evidence // \"\"" 2>/dev/null)
            BLOCKER="${BLOCKER}${dim} ($(printf '%.2f' "$NEW_SCORE")): ${EVIDENCE}\n"
        fi
    else
        # Delta mode: only deny if WORSENING (was ≥0.5, now <0.5).
        BASELINE_SCORE=$(printf '%s' "$BASELINE_JSON" | jq -r ".dimensions.${key}.value // 1.0" 2>/dev/null || echo 1.0)
        [ -z "$BASELINE_SCORE" ] && BASELINE_SCORE=1.0

        WAS_OK=false
        awk -v s="$BASELINE_SCORE" 'BEGIN{exit !(s+0 >= 0.5)}' && WAS_OK=true
        NOW_FAIL=false
        awk -v s="$NEW_SCORE" 'BEGIN{exit !(s+0 < 0.5)}' && NOW_FAIL=true

        if $WAS_OK && $NOW_FAIL; then
            EVIDENCE=$(printf '%s' "$SCORE_JSON" | jq -r ".dimensions.${key}.evidence // \"\"" 2>/dev/null)
            BLOCKER="${BLOCKER}${dim} WORSENED (was $(printf '%.2f' "$BASELINE_SCORE") → now $(printf '%.2f' "$NEW_SCORE")): ${EVIDENCE}\n"
        fi
    fi
done

if [ -n "$BLOCKER" ]; then
    MODE="ABSOLUTE"
    $IS_NEW_FILE || MODE="DELTA"
    REASON="⛔ BLOCK [$MODE]: 50-dim P0 regression in $FILE_PATH — $(printf '%b' "$BLOCKER" | tr '\n' '|') Fix: touring-quality check --gate <dim> --target $FILE_PATH"
    REASON_ESC=$(printf '%s' "$REASON" | tr '\n' ' ' | sed 's/"/'\''/g')
    printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$REASON_ESC"
    log "DENY [$MODE] $FILE_PATH"
    exit 0
fi

exit 0
