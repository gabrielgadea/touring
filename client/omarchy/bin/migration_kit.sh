#!/usr/bin/env bash
# migration_kit.sh — Gera ~/omarchy-kit (ou --dest <dir>) com MANIFEST.sha256,
# SYMLINKS.tsv e kit.json.  Saída: ok|FAIL|warn <nome> <evidência> (ou --json).
# Uso: migration_kit.sh [--home <dir>] [--dest <dir>] [--dry-run] [--json]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOME_DIR="${HOME}"
DEST_DIR=""
DRY_RUN="false"
JSON_OUT="false"

usage() { echo "Usage: migration_kit.sh [--home DIR] [--dest DIR] [--dry-run] [--json]" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --home)    HOME_DIR="${2:?'--home requires a value'}";  shift 2 ;;
        --dest)    DEST_DIR="${2:?'--dest requires a value'}";  shift 2 ;;
        --dry-run) DRY_RUN="true";  shift ;;
        --json)    JSON_OUT="true";  shift ;;
        -h|--help) usage ;;
        *)         echo "Unknown argument: $1" >&2; usage ;;
    esac
done

DEST_DIR="${DEST_DIR:-$HOME_DIR/omarchy-kit}"
CHECKS_FILE="$(mktemp)"
trap 'rm -f "$CHECKS_FILE"' EXIT
FAIL_COUNT=0

_rec() {
    local status="$1" name="$2" evidence="${3:-}"
    echo "${status}|${name}|${evidence}" >> "$CHECKS_FILE"
    if [[ "$JSON_OUT" == "false" ]]; then
        echo "$status $name $evidence"
    fi
    if [[ "$status" == "FAIL" ]]; then
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
}
ok()   { _rec "ok"   "$1" "${2:-}"; }
fail() { _rec "FAIL" "$1" "${2:-}"; FAIL_COUNT=$((FAIL_COUNT + 1)); }
warn() { _rec "warn" "$1" "${2:-}"; }

# Nota: _rec já incrementa FAIL_COUNT para FAIL; a função fail() incrementaria duas vezes.
# Corrijo: apenas _rec faz o incremento; ok/warn/fail são aliases puros.
# Redefinir sem duplo incremento:
ok()   { local n="$1" e="${2:-}"; echo "ok|$n|$e" >> "$CHECKS_FILE"; [[ "$JSON_OUT" == "false" ]] && echo "ok $n $e"; true; }
fail() { local n="$1" e="${2:-}"; echo "FAIL|$n|$e" >> "$CHECKS_FILE"; [[ "$JSON_OUT" == "false" ]] && echo "FAIL $n $e"; FAIL_COUNT=$((FAIL_COUNT + 1)); }
warn() { local n="$1" e="${2:-}"; echo "warn|$n|$e" >> "$CHECKS_FILE"; [[ "$JSON_OUT" == "false" ]] && echo "warn $n $e"; true; }

# ── DRY RUN ──────────────────────────────────────────────────────────────────
if [[ "$DRY_RUN" == "true" ]]; then
    echo "DRY RUN — would copy to: $DEST_DIR"
    echo "  SOURCE home: $HOME_DIR"
    echo "  .claude/{CLAUDE.md,settings.json,rules,agents,commands,skills,scripts}"
    echo "  .claude/hooks  (excl *.old __pycache__)"
    echo "  .claude/projects/*/memory/  (excl *.jsonl)"
    echo "  .agents/"
    echo "  .claude.json  (chmod 600)"
    echo "  .config/yt-dlp/"
    echo "  .claude-video-vision/config.json"
    echo "  .gitconfig"
    echo "  .ssh/*.pub"
    if [[ -d "$HOME_DIR/.claude/skills" ]]; then
        while IFS= read -r -d '' lnk; do
            target="$(readlink "$lnk")"
            if [[ "$target" == /* ]] \
               && [[ "$target" != "$HOME_DIR/.claude"* ]] \
               && [[ "$target" != "$HOME_DIR/.agents"* ]]; then
                echo "  resolved/analise-skills/$(basename "$lnk")  (cp -rL $target)"
            fi
        done < <(find "$HOME_DIR/.claude/skills" -maxdepth 1 -type l -print0 2>/dev/null || true)
    fi
    echo "  touring-db/  (projects/touring/.claude/touring/*.db)"
    echo "  efi/{efibootmgr-v.txt,bootctl-status.txt,lsblk-f.txt,blkid.txt}"
    echo "  settings.stageA.json  settings.stageB.json"
    echo "  MANIFEST.sha256  SYMLINKS.tsv  kit.json"
    exit 0
fi

mkdir -p "$DEST_DIR"

# ── 1. .claude individual items ───────────────────────────────────────────────
mkdir -p "$DEST_DIR/.claude"
for item in CLAUDE.md settings.json rules agents commands skills scripts; do
    src="$HOME_DIR/.claude/$item"
    if [[ -e "$src" ]] || [[ -L "$src" ]]; then
        if rsync -a "$src" "$DEST_DIR/.claude/" 2>/dev/null; then
            ok "rsync-claude-$item" "$src"
        else
            fail "rsync-claude-$item" "rsync failed for $src"
        fi
    else
        warn "rsync-claude-$item" "not found: $src"
    fi
done

# ── 2. .claude/hooks (excl *.old __pycache__) ────────────────────────────────
if [[ -d "$HOME_DIR/.claude/hooks" ]]; then
    mkdir -p "$DEST_DIR/.claude/hooks"
    if rsync -a --exclude='*.old' --exclude='__pycache__' \
               "$HOME_DIR/.claude/hooks/" "$DEST_DIR/.claude/hooks/" 2>/dev/null; then
        ok "rsync-claude-hooks" "excl *.old __pycache__"
    else
        fail "rsync-claude-hooks" "rsync failed"
    fi
else
    warn "rsync-claude-hooks" "hooks dir not found"
fi

# ── 3. .claude/projects/*/memory/ (never *.jsonl) ────────────────────────────
found_mem=0
if [[ -d "$HOME_DIR/.claude/projects" ]]; then
    for proj_dir in "$HOME_DIR/.claude/projects"/*/; do
        [[ -d "$proj_dir" ]] || continue
        mem_dir="${proj_dir}memory"
        if [[ -d "$mem_dir" ]]; then
            proj_name="$(basename "$proj_dir")"
            dest_mem="$DEST_DIR/.claude/projects/$proj_name/memory"
            mkdir -p "$dest_mem"
            if rsync -a --exclude='*.jsonl' "$mem_dir/" "$dest_mem/" 2>/dev/null; then
                found_mem=$((found_mem + 1))
            else
                warn "rsync-memory-$proj_name" "rsync failed"
            fi
        fi
    done
    ok "rsync-projects-memory" "$found_mem project memory dir(s)"
else
    warn "rsync-projects-memory" "projects dir not found"
fi

# ── 4. .agents/ ──────────────────────────────────────────────────────────────
if [[ -d "$HOME_DIR/.agents" ]]; then
    if rsync -a "$HOME_DIR/.agents/" "$DEST_DIR/.agents/" 2>/dev/null; then
        ok "rsync-agents" "$HOME_DIR/.agents/"
    else
        fail "rsync-agents" "rsync failed"
    fi
else
    warn "rsync-agents" ".agents not found"
fi

# ── 5. .claude.json (chmod 600) ───────────────────────────────────────────────
if [[ -f "$HOME_DIR/.claude.json" ]]; then
    cp -a "$HOME_DIR/.claude.json" "$DEST_DIR/.claude.json"
    chmod 600 "$DEST_DIR/.claude.json"
    ok "copy-claude-json" "chmod 600 applied"
else
    warn "copy-claude-json" ".claude.json not found"
fi

# ── 6. .config/yt-dlp ─────────────────────────────────────────────────────────
if [[ -d "$HOME_DIR/.config/yt-dlp" ]]; then
    mkdir -p "$DEST_DIR/.config"
    if rsync -a "$HOME_DIR/.config/yt-dlp/" "$DEST_DIR/.config/yt-dlp/" 2>/dev/null; then
        ok "rsync-yt-dlp" "ok"
    else
        warn "rsync-yt-dlp" "rsync failed"
    fi
else
    warn "rsync-yt-dlp" "not found"
fi

# ── 7. .claude-video-vision/config.json ──────────────────────────────────────
if [[ -f "$HOME_DIR/.claude-video-vision/config.json" ]]; then
    mkdir -p "$DEST_DIR/.claude-video-vision"
    cp -a "$HOME_DIR/.claude-video-vision/config.json" "$DEST_DIR/.claude-video-vision/config.json"
    ok "copy-video-vision-config" "ok"
else
    warn "copy-video-vision-config" "not found"
fi

# ── 8. .gitconfig ─────────────────────────────────────────────────────────────
if [[ -f "$HOME_DIR/.gitconfig" ]]; then
    cp -a "$HOME_DIR/.gitconfig" "$DEST_DIR/.gitconfig"
    ok "copy-gitconfig" "ok"
else
    warn "copy-gitconfig" "not found"
fi

# ── 9. .ssh/*.pub ─────────────────────────────────────────────────────────────
pub_count=0
for pub in "$HOME_DIR/.ssh/"*.pub; do
    [[ -f "$pub" ]] || continue
    mkdir -p "$DEST_DIR/.ssh"
    cp -a "$pub" "$DEST_DIR/.ssh/"
    pub_count=$((pub_count + 1))
done
if [[ "$pub_count" -gt 0 ]]; then
    ok "copy-ssh-pub" "$pub_count public key(s)"
else
    warn "copy-ssh-pub" "no .pub keys found"
fi

# ── 10. resolved/analise-skills/ (cp -rL absolute external symlinks) ──────────
mkdir -p "$DEST_DIR/resolved/analise-skills"
abs_count=0
if [[ -d "$HOME_DIR/.claude/skills" ]]; then
    while IFS= read -r -d '' lnk; do
        target="$(readlink "$lnk")"
        if [[ "$target" == /* ]] \
           && [[ "$target" != "$HOME_DIR/.claude"* ]] \
           && [[ "$target" != "$HOME_DIR/.agents"* ]]; then
            skill_name="$(basename "$lnk")"
            if [[ -e "$target" ]]; then
                cp -rL "$target" "$DEST_DIR/resolved/analise-skills/$skill_name"
                abs_count=$((abs_count + 1))
            else
                warn "resolved-$skill_name" "target not found: $target"
            fi
        fi
    done < <(find "$HOME_DIR/.claude/skills" -maxdepth 1 -type l -print0 2>/dev/null || true)
fi
ok "resolved-analise-skills" "$abs_count resolved"

# ── 11. efi/ — captura sem sudo; warn se falhar ───────────────────────────────
mkdir -p "$DEST_DIR/efi"
_efi_cmd() {
    local name="$1" outfile="$2"; shift 2
    if "$@" > "$outfile" 2>/dev/null; then
        ok "efi-$name" "ok"
    else
        printf "" > "$outfile"
        warn "efi-$name" "command failed (may need root)"
    fi
}
_efi_cmd "efibootmgr" "$DEST_DIR/efi/efibootmgr-v.txt"   efibootmgr -v
_efi_cmd "bootctl"    "$DEST_DIR/efi/bootctl-status.txt"  bootctl status
_efi_cmd "lsblk"      "$DEST_DIR/efi/lsblk-f.txt"        lsblk -f -o NAME,SERIAL,SIZE,FSTYPE,PARTUUID,MOUNTPOINT
_efi_cmd "blkid"      "$DEST_DIR/efi/blkid.txt"           blkid

# ── 12. touring-db/ — sqlite3 .backup ou cp ──────────────────────────────────
mkdir -p "$DEST_DIR/touring-db"
db_count=0
DB_SRC="$HOME_DIR/projects/touring/.claude/touring"
if [[ -d "$DB_SRC" ]]; then
    for db_file in "$DB_SRC"/*.db; do
        [[ -f "$db_file" ]] || continue
        db_name="$(basename "$db_file")"
        dest_db="$DEST_DIR/touring-db/$db_name"
        if command -v sqlite3 &>/dev/null; then
            if sqlite3 "$db_file" ".backup \"${dest_db//\"/}\"" 2>/dev/null; then
                db_count=$((db_count + 1))
            else
                cp "$db_file" "$dest_db" 2>/dev/null || true
                warn "touring-db-backup-$db_name" "sqlite3 .backup failed; used cp"
                db_count=$((db_count + 1))
            fi
        else
            cp "$db_file" "$dest_db" 2>/dev/null || true
            warn "touring-db-nosqlite3-$db_name" "sqlite3 not available; used cp"
            db_count=$((db_count + 1))
        fi
    done
    ok "touring-db" "$db_count database(s)"
else
    warn "touring-db" "source not found: $DB_SRC"
fi

# ── 13. settings.stageA.json + settings.stageB.json ─────────────────────────
SETTINGS_SRC="$HOME_DIR/.claude/settings.json"
if [[ -f "$SETTINGS_SRC" ]]; then
    if python3 "$SCRIPT_DIR/settings_stage.py" \
            --in "$SETTINGS_SRC" --stage A \
            --out "$DEST_DIR/settings.stageA.json" \
            --home "$HOME_DIR" 2>/dev/null; then
        ok "settings-stageA" "generated"
    else
        fail "settings-stageA" "settings_stage.py --stage A failed"
    fi
    if python3 "$SCRIPT_DIR/settings_stage.py" \
            --in "$SETTINGS_SRC" --stage B \
            --out "$DEST_DIR/settings.stageB.json" \
            --home "$HOME_DIR" 2>/dev/null; then
        ok "settings-stageB" "generated"
    else
        fail "settings-stageB" "settings_stage.py --stage B failed"
    fi
else
    warn "settings-stages" "settings.json not found"
fi

# ── 14. SYMLINKS.tsv — path<TAB>target<TAB>abs|rel ───────────────────────────
{
    printf 'path\ttarget\ttype\n'
    find "$DEST_DIR" -type l | sort | while IFS= read -r lnk; do
        rel_path="${lnk#${DEST_DIR}/}"
        target="$(readlink "$lnk")"
        if [[ "$target" == /* ]]; then
            ltype="abs"
        else
            ltype="rel"
        fi
        printf '%s\t%s\t%s\n' "$rel_path" "$target" "$ltype"
    done
} > "$DEST_DIR/SYMLINKS.tsv"
symlink_count=$(find "$DEST_DIR" -type l | wc -l)
ok "symlinks-tsv" "$symlink_count symlink(s)"

# ── 15. MANIFEST.sha256 — todos os arquivos regulares ────────────────────────
(
    cd "$DEST_DIR"
    # kit.json é gerado depois do manifest (contém manifest_sha256); exclui-o
    find . -type f ! -name 'MANIFEST.sha256' ! -name 'kit.json' | sort | while IFS= read -r f; do
        sha256sum "$f"
    done
) > "$DEST_DIR/MANIFEST.sha256"
manifest_count=$(wc -l < "$DEST_DIR/MANIFEST.sha256")
ok "manifest-sha256" "$manifest_count file(s)"

# ── 16. kit.json ──────────────────────────────────────────────────────────────
KIT_CREATED_AT="$(date -Iseconds)"
KIT_HOSTNAME="$(hostname)"
KIT_USER="$(id -un)"
# `touring --version` writes to stderr and prints a second line (git hash);
# `| head -1` under pipefail made the producer die of SIGPIPE, the pipeline
# report failure, and `|| echo unknown` APPEND to output already captured
# ("30.4.13unknown"). Capture everything, then keep the first line.
if _tv="$(touring --version 2>&1)"; then KIT_TOURING="${_tv%%$'\n'*}"; else KIT_TOURING="unknown"; fi
if _cv="$(claude --version 2>&1)"; then KIT_CLAUDE="${_cv%%$'\n'*}"; else KIT_CLAUDE="unknown"; fi
KIT_SHA="$(sha256sum "$DEST_DIR/MANIFEST.sha256" | awk '{print $1}')"
KIT_FILES="$(find "$DEST_DIR" -type f | wc -l)"
KIT_SYMLINKS="$(find "$DEST_DIR" -type l | wc -l)"
KIT_BYTES="$(du -sb "$DEST_DIR" 2>/dev/null | awk '{print $1}' || echo 0)"

KIT_CREATED_AT="$KIT_CREATED_AT" KIT_HOSTNAME="$KIT_HOSTNAME" KIT_USER="$KIT_USER" \
KIT_TOURING="$KIT_TOURING" KIT_CLAUDE="$KIT_CLAUDE" KIT_SHA="$KIT_SHA" \
KIT_FILES="$KIT_FILES" KIT_SYMLINKS="$KIT_SYMLINKS" KIT_BYTES="$KIT_BYTES" \
python3 - <<'PYEOF' > "$DEST_DIR/kit.json"
import json, os
print(json.dumps({
    "created_at":      os.environ["KIT_CREATED_AT"],
    "hostname":        os.environ["KIT_HOSTNAME"],
    "user":            os.environ["KIT_USER"],
    "touring_version": os.environ["KIT_TOURING"],
    "claude_version":  os.environ["KIT_CLAUDE"],
    "manifest_sha256": os.environ["KIT_SHA"],
    "bytes_total":     int(os.environ["KIT_BYTES"]),
    "files_total":     int(os.environ["KIT_FILES"]),
    "symlinks_total":  int(os.environ["KIT_SYMLINKS"]),
}, indent=2))
PYEOF
ok "kit-json" "created_at $KIT_CREATED_AT"

# ── JSON output ───────────────────────────────────────────────────────────────
if [[ "$JSON_OUT" == "true" ]]; then
    python3 - "$CHECKS_FILE" "$FAIL_COUNT" <<'PYEOF'
import json, sys
checks_file = sys.argv[1]
fail_count  = int(sys.argv[2])
checks = []
with open(checks_file) as fh:
    for line in fh:
        line = line.rstrip('\n')
        parts = line.split('|', 2)
        if len(parts) == 3:
            checks.append({"name": parts[1], "status": parts[0], "evidence": parts[2]})
print(json.dumps({
    "phase": "migration_kit",
    "ok":    fail_count == 0,
    "checks": checks,
}, indent=2))
PYEOF
fi

exit "$((FAIL_COUNT > 0 ? 1 : 0))"
