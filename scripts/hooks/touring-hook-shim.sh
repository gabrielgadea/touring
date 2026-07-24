#!/usr/bin/env bash
# touring-hook-shim — W12.6 (2026-05-23) rustup-pattern hook dispatcher.
#
# Walks up from CWD (or $CLAUDE_PROJECT_DIR if set) looking for a per-project
# `.touring/bin/touring-hook`. Falls back to the user-default toolchain at
# `~/.touring/toolchains/<default>/bin/touring-hook`. Final fallback is the
# global release binary at `~/.claude/rust/target/release/touring-hook`.
#
# **Fail-open contract**: every Touring hook in `~/.claude/settings.json` is
# fail-open (exit 0 even on internal error). This shim honors that — any
# failure path (no binary found, exec failure) exits 0 so the user's tool
# call is never blocked by missing infrastructure.
#
# Side-by-side install: this shim is NOT (yet) symlinked into
# `~/.claude/hooks/touring-hook`. To opt in, swap the symlink atomically:
#
#   ln -sfn ~/.claude/rust/scripts/hooks/touring-hook-shim.sh \
#       ~/.claude/hooks/touring-hook
#
# To revert: re-point to `~/.claude/rust/target/release/touring-hook`.
#
# Environment:
#   TOURING_HOOK_SHIM_TRACE=1      — log each lookup step to stderr (off by default)
#   TOURING_HOOK_SHIM_FORCE_BIN=…  — bypass the walk-up entirely, use this path
#   TOURING_HOME                   — override ~/.touring (default user-toolchain root)
#   CLAUDE_PROJECT_DIR             — start walk-up here instead of $PWD
#
# Exit code: forwarded from the resolved binary, OR 0 if no binary found.

set -u
# Note: NO `set -e` — we MUST fail-open per the hook contract.

trace() {
    if [[ "${TOURING_HOOK_SHIM_TRACE:-0}" = "1" ]]; then
        printf 'touring-hook-shim: %s\n' "$*" >&2
    fi
}

# ── Layer 1: explicit override ───────────────────────────────────────────────
if [[ -n "${TOURING_HOOK_SHIM_FORCE_BIN:-}" ]]; then
    if [[ -x "$TOURING_HOOK_SHIM_FORCE_BIN" ]]; then
        trace "force_bin: $TOURING_HOOK_SHIM_FORCE_BIN"
        exec "$TOURING_HOOK_SHIM_FORCE_BIN" "$@"
    fi
    trace "force_bin not executable, falling through: $TOURING_HOOK_SHIM_FORCE_BIN"
fi

# ── Layer 2: per-project walk-up ─────────────────────────────────────────────
start_dir="${CLAUDE_PROJECT_DIR:-$PWD}"
walk_up_project_bin() {
    local dir="$1"
    while :; do
        local candidate="$dir/.touring/bin/touring-hook"
        if [[ -x "$candidate" ]]; then
            printf '%s' "$candidate"
            return 0
        fi
        # Stop at root (parent == self)
        local parent
        parent=$(dirname "$dir")
        if [[ "$parent" = "$dir" ]]; then
            return 1
        fi
        dir="$parent"
    done
}
if project_bin=$(walk_up_project_bin "$start_dir"); then
    trace "project_bin: $project_bin (from $start_dir)"
    exec "$project_bin" "$@"
fi
trace "no project_bin found from $start_dir"

# ── Layer 3: user-default toolchain ──────────────────────────────────────────
touring_home="${TOURING_HOME:-$HOME/.touring}"
default_file="$touring_home/default"
if [[ -f "$default_file" ]]; then
    default_version=$(head -n 1 "$default_file" | tr -d '[:space:]')
    if [[ -n "$default_version" ]]; then
        user_bin="$touring_home/toolchains/$default_version/bin/touring-hook"
        if [[ -x "$user_bin" ]]; then
            trace "user_bin: $user_bin (version=$default_version)"
            exec "$user_bin" "$@"
        fi
        trace "user_bin not executable: $user_bin"
    else
        trace "$default_file is empty"
    fi
else
    trace "no $default_file"
fi

# ── Layer 4: dev-channel / canonical-source fallback ─────────────────────────
# F2 (2026-07-24): the update-touring managed dev symlink comes first (always
# tracks the freshly built binary wherever the source root lives), then the
# canonical source root's release build directly. The historical
# ~/.claude/rust fallback is gone — that tree is a FROZEN copy (F4' move).
for global_bin in \
    "$HOME/.local/bin/touring-hook" \
    "$HOME/projects/touring/target/release/touring-hook"; do
    if [[ -x "$global_bin" ]]; then
        trace "global_bin: $global_bin"
        exec "$global_bin" "$@"
    fi
done

# ── Fail-open: no binary found ───────────────────────────────────────────────
# Per the hook contract, never block the user's tool call. Silently exit 0.
trace "no touring-hook binary found in any layer — fail-open exit 0"
exit 0
