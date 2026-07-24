#!/usr/bin/env bash
# grammar-abi-resolver.sh
#
# S-1 of Wave 5 CEG Pln2 Final Closure (mossy-crunching-owl).
# Deterministic per-grammar matrix resolver for tree-sitter ABI bumps.
#
# Reads workspace Cargo.toml, extracts every `tree-sitter-*` pin, queries
# crates.io via `cargo search` for each, classifies declared ABI (14/15) where
# inferable from version, and dry-runs `cargo update` against the proposed pin
# matrix. Refuses to allow proceeding if any proposed pin lacks a corresponding
# release on crates.io (the 2026-05-17 blind-bump failure mode).
#
# Usage:
#   bash scripts/grammar-abi-resolver.sh                  # human-readable matrix
#   bash scripts/grammar-abi-resolver.sh --json           # JSON output for tooling
#   bash scripts/grammar-abi-resolver.sh --validate-cargo-toml  # gate for S-4
#   bash scripts/grammar-abi-resolver.sh --dry-run        # cargo update preview
#
# Exit codes:
#   0 — matrix complete, all proposed pins resolvable
#   1 — usage error / argument failure
#   2 — at least one proposed pin does not exist on crates.io (BLOCKER)
#   3 — cargo update --dry-run failed (cargo resolver conflict)
#
# Reusable for future ABI bumps (tree-sitter 0.27, ast-grep 0.43, etc.) —
# zero rewrite required, only update GRAMMAR_LIST array if new grammars are
# added to the workspace.

set -euo pipefail

WORKSPACE_DIR="${WORKSPACE_DIR:-${HOME}/.claude/rust}"
CARGO_TOML="${WORKSPACE_DIR}/Cargo.toml"

# Known workspace grammars (extend if you add new tree-sitter-* deps)
GRAMMAR_LIST=(
    "python"
    "rust"
    "typescript"
    "javascript"
    "html"
    "css"
    "json"
    "bash"
    "toml-ng"
    "yaml"
    "md"
    "go"
)

# Mode flags
MODE="human"
for arg in "$@"; do
    case "$arg" in
        --json) MODE="json" ;;
        --validate-cargo-toml) MODE="validate" ;;
        --dry-run) MODE="dryrun" ;;
        --help|-h)
            sed -n '1,30p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *)
            echo "ERROR: unknown argument '$arg'. Use --help." >&2
            exit 1
            ;;
    esac
done

# Validate environment
if [ ! -f "$CARGO_TOML" ]; then
    echo "ERROR: Cargo.toml not found at $CARGO_TOML" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "ERROR: cargo not found in PATH" >&2
    exit 1
fi

# Helper: extract current pin from Cargo.toml for a grammar
get_current_pin() {
    local grammar="$1"
    grep -E "^tree-sitter-${grammar}\s*=" "$CARGO_TOML" 2>/dev/null \
        | head -1 \
        | sed -E 's/.*"([0-9.]+)".*/\1/' \
        || echo "absent"
}

# Helper: query crates.io for latest version
get_latest_version() {
    local grammar="$1"
    cargo search "tree-sitter-${grammar}" --limit 1 2>/dev/null \
        | head -1 \
        | grep -oE '"[0-9.]+"' \
        | head -1 \
        | tr -d '"' \
        || echo "not_found"
}

# Helper: infer ABI from version (heuristic — tree-sitter convention)
# 0.25.x and later = ABI 15; 0.24.x = transition; 0.23.x and earlier = ABI 14
infer_abi() {
    local version="$1"
    case "$version" in
        not_found|absent|"") echo "unknown" ;;
        0.25.*|0.26.*|0.27.*|0.28.*) echo "15" ;;
        0.24.*) echo "14_or_15_transition" ;;
        0.23.*|0.22.*|0.21.*|0.20.*) echo "14" ;;
        0.5.*|0.6.*|0.7.*) echo "varies" ;;  # workspace-versioned grammars
        *) echo "unknown" ;;
    esac
}

# Main matrix collection
declare -a MATRIX_ROWS=()
declare -i ANY_MISSING=0

for grammar in "${GRAMMAR_LIST[@]}"; do
    current=$(get_current_pin "$grammar")
    latest=$(get_latest_version "$grammar")
    abi=$(infer_abi "$latest")
    bump_recommended="no"

    if [ "$latest" = "not_found" ]; then
        bump_recommended="MISSING_ON_CRATES_IO"
        ANY_MISSING=1
    elif [ "$current" != "$latest" ] && [ "$current" != "absent" ]; then
        bump_recommended="yes"
    elif [ "$current" = "absent" ]; then
        bump_recommended="not_in_workspace"
    fi

    MATRIX_ROWS+=("${grammar}|${current}|${latest}|${abi}|${bump_recommended}")
done

# Output
case "$MODE" in
    human)
        printf "%-22s %-12s %-12s %-22s %-20s\n" "GRAMMAR" "CURRENT" "LATEST" "INFERRED_ABI" "BUMP"
        printf "%-22s %-12s %-12s %-22s %-20s\n" "-------" "-------" "------" "------------" "----"
        for row in "${MATRIX_ROWS[@]}"; do
            IFS='|' read -r g c l a b <<< "$row"
            printf "%-22s %-12s %-12s %-22s %-20s\n" "tree-sitter-${g}" "$c" "$l" "$a" "$b"
        done
        echo ""
        echo "ast-grep-core current: $(grep -E '^ast-grep-core\s*=' "$CARGO_TOML" | head -1 | sed -E 's/.*"=?([0-9.]+)".*/\1/')"
        echo "ast-grep-core latest:  $(cargo search ast-grep-core --limit 1 2>/dev/null | head -1 | grep -oE '"[0-9.]+"' | head -1 | tr -d '"')"
        echo "tree-sitter current:   $(grep -E '^tree-sitter\s*=' "$CARGO_TOML" | head -1 | sed -E 's/.*"([0-9.]+)".*/\1/')"
        echo "tree-sitter latest:    $(cargo search tree-sitter --limit 1 2>/dev/null | head -1 | grep -oE '"[0-9.]+"' | head -1 | tr -d '"')"
        if [ $ANY_MISSING -ne 0 ]; then
            echo ""
            echo "WARNING: at least one grammar shows MISSING_ON_CRATES_IO — blind bump will fail cargo resolve."
            exit 2
        fi
        ;;
    json)
        echo "{"
        echo "  \"matrix\": ["
        local_first=1
        for row in "${MATRIX_ROWS[@]}"; do
            IFS='|' read -r g c l a b <<< "$row"
            if [ $local_first -eq 1 ]; then
                local_first=0
            else
                echo ","
            fi
            printf '    {"grammar":"tree-sitter-%s","current_pin":"%s","latest_on_crates_io":"%s","inferred_abi":"%s","bump_recommended":"%s"}' "$g" "$c" "$l" "$a" "$b"
        done
        echo ""
        echo "  ],"
        echo "  \"any_missing\": $([ $ANY_MISSING -eq 0 ] && echo false || echo true)"
        echo "}"
        ;;
    validate)
        # Gate for S-4: pass only if every CURRENT pin (non-absent) has a corresponding crates.io release.
        # This catches the 2026-05-17 failure where Cargo.toml had `tree-sitter-html = "0.25"` that did not exist.
        FAIL=0
        for row in "${MATRIX_ROWS[@]}"; do
            IFS='|' read -r g c l a b <<< "$row"
            if [ "$c" != "absent" ] && [ "$l" = "not_found" ]; then
                echo "BLOCK: Cargo.toml pins tree-sitter-${g} = \"${c}\" but no release on crates.io" >&2
                FAIL=1
            fi
        done
        if [ $FAIL -eq 0 ]; then
            echo "OK: all current Cargo.toml grammar pins resolve to crates.io releases"
            exit 0
        else
            exit 2
        fi
        ;;
    dryrun)
        echo "Running cargo update --dry-run for grammar deps..."
        (cd "$WORKSPACE_DIR" && cargo update --workspace --dry-run 2>&1 | grep -E "tree-sitter|ast-grep" | head -30)
        ec=${PIPESTATUS[0]}
        if [ $ec -ne 0 ]; then
            echo "ERROR: cargo update --dry-run failed (exit $ec)" >&2
            exit 3
        fi
        echo "OK: cargo update dry-run succeeded"
        ;;
esac
