#!/usr/bin/env bash
# disk_guard.sh — Prune only duplicate touring binaries from target/
# Keeps the currently-built binary, removes stale hash artifacts
# Run manually or via cron: disk_guard.sh [--dry-run]

set -euo pipefail

TARGET="target"
DRY_RUN="${1:-}"

# Find all touring-XXXXXXXXXXXXXXXX (hash-suffixed binaries) in deps/
# Exclude the most recently modified one (current build)
cleanup_duplicates() {
    local count=0
    local freed=0

    # Find duplicate touring binaries (excluding the active one)
    # These are ~500MB each from incremental compiles with different flags/profiles
    while IFS= read -r bin; do
        ((count++)) || true
        local size
        size=$(stat -c%s "$bin" 2>/dev/null || stat -f%z "$bin" 2>/dev/null || echo 0)
        if [[ -n "$DRY_RUN" ]]; then
            echo "[DRY-RUN] Would remove: $bin ($(numfmt --to=iec-i --suffix=B "$size"))"
        else
            rm -f "$bin"
            ((freed += size)) || true
            echo "Removed: $bin"
        fi
    done < <(find "$TARGET" -path "*/deps/touring-[a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9]" -type f 2>/dev/null | \
          sort -t/ -k1 | uniq -c | awk 'NR>1 && $1>1 {print $2}' || true)

    # Remove old touring_hooks binaries (same pattern)
    while IFS= read -r bin; do
        ((count++)) || true
        local size
        size=$(stat -c%s "$bin" 2>/dev/null || stat -f%z "$bin" 2>/dev/null || echo 0)
        if [[ -n "$DRY_RUN" ]]; then
            echo "[DRY-RUN] Would remove: $bin ($(numfmt --to=iec-i --suffix=B "$size"))"
        else
            rm -f "$bin"
            ((freed += size)) || true
            echo "Removed: $bin"
        fi
    done < <(find "$TARGET" -path "*/deps/touring_hooks-[a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9]" -type f 2>/dev/null | \
          sort -t/ -k1 | uniq -c | awk 'NR>1 && $1>1 {print $2}' || true)

    # Report
    if [[ -n "$DRY_RUN" ]]; then
        echo ""
        echo "[DRY-RUN] $count duplicate binary(ies) would be removed"
    else
        echo ""
        echo "Cleaned $count duplicate binary(ies), freed ~$(numfmt --to=iec-i --suffix=B "$freed" 2>/dev/null || echo "${freed} bytes")"
    fi
}

# Check disk usage and warn
check_disk() {
    local usage
    usage=$(df -h "$TARGET" | awk 'NR==2 {print $5}' | tr -d '%')
    local path
    path=$(df "$TARGET" | awk 'NR==2 {print $6}')

    echo "Disk usage for $path: ${usage}%"

    if (( usage >= 90 )); then
        echo "⚠️  CRITICAL: Disk usage at ${usage}%! Run: ./scripts/disk_guard.sh"
    elif (( usage >= 80 )); then
        echo "⚠️  WARNING: Disk usage at ${usage}%. Consider running: ./scripts/disk_guard.sh"
    else
        echo "✓ Disk usage OK: ${usage}%"
    fi
}

case "${1:-}" in
    --check|-c)
        check_disk
        ;;
    --dry-run|-n)
        cleanup_duplicates --dry-run
        ;;
    --help|-h)
        echo "Usage: disk_guard.sh [--check|--dry-run]"
        echo "  --check     Show disk usage without cleaning"
        echo "  --dry-run   Show what would be removed without deleting"
        echo "  (no args)  Actually remove duplicate binaries"
        ;;
    *)
        cleanup_duplicates
        ;;
esac
