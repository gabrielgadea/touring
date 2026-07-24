#!/usr/bin/env bash
# debug-touring-daemon.sh — Attach BugStalker to a running touring serve daemon.
#
# Wave 6 (2026-04-26) — Touring v4.18.0 helper.
# Docs: ~/.claude/skills/Touring/references/touring-cli-debugging-bugstalker.md
#
# Usage:
#   ./debug-touring-daemon.sh              # console mode
#   ./debug-touring-daemon.sh --tui        # TUI mode
#   ./debug-touring-daemon.sh --oracle     # tokio oracle pre-loaded
#   ./debug-touring-daemon.sh --dap        # DAP mode (VSCode)
#   ./debug-touring-daemon.sh --pid 12345  # explicit PID instead of pgrep

set -euo pipefail

readonly SCRIPT_NAME="${0##*/}"

print_err() { printf '\e[1;31m[%s]\e[0m %s\n' "$SCRIPT_NAME" "$*" >&2; }
print_warn() { printf '\e[1;33m[%s]\e[0m %s\n' "$SCRIPT_NAME" "$*" >&2; }
print_info() { printf '\e[1;32m[%s]\e[0m %s\n' "$SCRIPT_NAME" "$*"; }

usage() {
    cat <<'EOF'
Usage: debug-touring-daemon.sh [OPTIONS]

Attach BugStalker to a running `touring serve` daemon for interactive
debugging without rebuild instrumentation overhead.

Options:
  --tui            Launch BugStalker in Terminal UI mode
  --oracle         Pre-load the tokio oracle (recommended for daemon hangs)
  --dap            Launch in Debug Adapter Protocol mode (for VSCode)
  --pid <PID>      Use explicit PID instead of `pgrep -f "touring serve"`
  -h, --help       Show this help

Examples:
  debug-touring-daemon.sh                      # console mode, auto-detect PID
  debug-touring-daemon.sh --oracle             # tokio task tree pre-loaded
  debug-touring-daemon.sh --tui --pid 54321    # explicit PID, TUI mode

When BugStalker exits (Ctrl+D in console, q in TUI), the daemon resumes.
SIGKILL on bs is also safe — kernel auto-detaches ptrace.

See full reference at:
  ~/.claude/skills/Touring/references/touring-cli-debugging-bugstalker.md
EOF
}

# ─── Argument parsing ────────────────────────────────────────────────────────
mode_flags=()
explicit_pid=""

while (( $# > 0 )); do
    case "$1" in
        --tui)    mode_flags+=("--tui");                shift ;;
        --oracle) mode_flags+=("--oracle" "tokio");     shift ;;
        --dap)    mode_flags+=("--dap");                shift ;;
        --pid)    explicit_pid="${2:?--pid requires a value}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) print_err "unknown argument: $1"; usage; exit 2 ;;
    esac
done

# ─── Sanity checks ───────────────────────────────────────────────────────────

if ! command -v bs >/dev/null 2>&1; then
    print_err "BugStalker not installed. Install with: cargo install bugstalker"
    print_err "See: ~/.claude/skills/Touring/references/touring-cli-debugging-bugstalker.md"
    exit 3
fi

bs_version="$(bs --version 2>/dev/null | head -1 || echo "unknown")"
print_info "BugStalker: $bs_version"

# ptrace_scope check — informative, not blocking
if [[ -r /proc/sys/kernel/yama/ptrace_scope ]]; then
    ptrace_scope="$(cat /proc/sys/kernel/yama/ptrace_scope)"
    case "$ptrace_scope" in
        0|1) ;; # safe for same-uid attach
        2)
            print_warn "ptrace_scope=2 — bs may need 'sudo' to attach"
            print_warn "Temporary fix: sudo sysctl kernel.yama.ptrace_scope=1"
            ;;
        3)
            print_err "ptrace_scope=3 — ptrace disabled, kernel rebuild required"
            exit 4
            ;;
    esac
fi

# ─── PID resolution ──────────────────────────────────────────────────────────

if [[ -n "$explicit_pid" ]]; then
    target_pid="$explicit_pid"
    if ! kill -0 "$target_pid" 2>/dev/null; then
        print_err "PID $target_pid is not running or not accessible"
        exit 5
    fi
else
    # Find first 'touring serve' process owned by current user
    target_pid="$(pgrep -u "$USER" -f 'touring serve' | head -1 || true)"
    if [[ -z "$target_pid" ]]; then
        print_err "No 'touring serve' daemon found. Start one with: touring serve"
        exit 6
    fi
fi

# Quick sanity: confirm it's actually a touring process
proc_cmd="$(tr '\0' ' ' < "/proc/$target_pid/cmdline" 2>/dev/null || echo "<unreadable>")"
case "$proc_cmd" in
    *touring*) ;;
    *)
        print_err "PID $target_pid does not look like a touring process: $proc_cmd"
        exit 7
        ;;
esac

print_info "Attaching to PID $target_pid: $proc_cmd"
print_info "Mode flags: ${mode_flags[*]:-(console)}"
print_info "Press Ctrl+D (console) or 'q' (TUI) to detach without killing daemon."
echo

# ─── Launch BugStalker ───────────────────────────────────────────────────────
exec bs "${mode_flags[@]}" -p "$target_pid"
