#!/usr/bin/env bash
# kit_check.sh — verifica a integridade de um omarchy-kit.
# Uso: kit_check.sh <kit-dir> [--self] [--json]
#   --self   modo origem: recalcula MANIFEST.sha256 e confere SYMLINKS.tsv
#   (sem --self)  modo destino: inclui checks de symlinks quebrados, chmod, JSON válido
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "Usage: kit_check.sh <kit-dir> [--self] [--json]" >&2
    exit 1
fi

KIT_DIR="$1"; shift
SELF_MODE="false"
JSON_OUT="false"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --self) SELF_MODE="true"; shift ;;
        --json) JSON_OUT="true";  shift ;;
        *)      echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

if [[ ! -d "$KIT_DIR" ]]; then
    echo "FAIL kit-dir-exists kit dir not found: $KIT_DIR"
    exit 1
fi

CHECKS_FILE="$(mktemp)"
trap 'rm -f "$CHECKS_FILE"' EXIT
FAIL_COUNT=0

ok()   { local n="$1" e="${2:-}"; echo "ok|$n|$e"   >> "$CHECKS_FILE"; [[ "$JSON_OUT" == "false" ]] && echo "ok $n $e";   true; }
fail() { local n="$1" e="${2:-}"; echo "FAIL|$n|$e" >> "$CHECKS_FILE"; [[ "$JSON_OUT" == "false" ]] && echo "FAIL $n $e"; FAIL_COUNT=$((FAIL_COUNT + 1)); }
warn() { local n="$1" e="${2:-}"; echo "warn|$n|$e" >> "$CHECKS_FILE"; [[ "$JSON_OUT" == "false" ]] && echo "warn $n $e"; true; }

# ── MANIFEST.sha256 ───────────────────────────────────────────────────────────
if [[ -f "$KIT_DIR/MANIFEST.sha256" ]]; then
    if (cd "$KIT_DIR" && sha256sum -c --quiet MANIFEST.sha256 2>/dev/null); then
        ok "manifest-hashes" "all match"
    else
        fail "manifest-hashes" "sha256sum mismatch detected"
    fi
else
    fail "manifest-hashes" "MANIFEST.sha256 not found in $KIT_DIR"
fi

# ── SYMLINKS.tsv ──────────────────────────────────────────────────────────────
if [[ -f "$KIT_DIR/SYMLINKS.tsv" ]]; then
    # header line + data lines; count data lines
    tsv_count=$(tail -n +2 "$KIT_DIR/SYMLINKS.tsv" | wc -l)
    actual_count=$(find "$KIT_DIR" -type l | wc -l)
    if [[ "$tsv_count" -eq "$actual_count" ]]; then
        ok "symlinks-tsv" "$tsv_count symlink(s) match"
    else
        fail "symlinks-tsv" "TSV has $tsv_count rows but found $actual_count symlinks"
    fi
else
    fail "symlinks-tsv" "SYMLINKS.tsv not found"
fi

# ── Destino-only checks ────────────────────────────────────────────────────────
if [[ "$SELF_MODE" == "false" ]]; then
    _home="${HOME:-/home/$(id -un)}"

    # Symlinks quebrados DENTRO do kit: um alvo absoluto que existia no Pop
    # (ex.: ~/projects/analise/…) e não existe aqui ainda. `-xtype l` segue o
    # link e casa só quando o destino falta. São esperados ANTES de P3.2 terminar
    # (analise clonada / target/release construído em P4) — por isso `warn`,
    # não `fail`; o `fail` vem do check de ~/.claude/skills abaixo, que é o que
    # o Claude realmente carrega.
    kit_broken=$(find "$KIT_DIR" -xtype l 2>/dev/null | wc -l)
    if [[ "$kit_broken" -eq 0 ]]; then
        ok "no-broken-kit-symlinks" "0 broken inside $KIT_DIR"
    else
        warn "no-broken-kit-symlinks" "$kit_broken broken symlink(s) inside the kit (targets not installed yet?)"
    fi

    # Sem symlinks quebrados em ~/.claude/skills
    if [[ -d "$_home/.claude/skills" ]]; then
        broken=$(find "$_home/.claude/skills" -xtype l 2>/dev/null | wc -l)
        if [[ "$broken" -eq 0 ]]; then
            ok "no-broken-skills-symlinks" "0 broken"
        else
            fail "no-broken-skills-symlinks" "$broken broken symlink(s) in ~/.claude/skills"
        fi
    else
        warn "no-broken-skills-symlinks" "\$HOME/.claude/skills not found"
    fi

    # .claude.json chmod 600
    if [[ -f "$_home/.claude.json" ]]; then
        mode="$(stat -c '%a' "$_home/.claude.json")"
        if [[ "$mode" == "600" ]]; then
            ok "claude-json-chmod" "600"
        else
            fail "claude-json-chmod" "mode=$mode expected 600"
        fi
    else
        warn "claude-json-chmod" ".claude.json not found"
    fi

    # Parse JSON das stages
    for stage_file in \
            "$KIT_DIR/settings.stageA.json" \
            "$KIT_DIR/settings.stageB.json"; do
        bn="$(basename "$stage_file")"
        if [[ -f "$stage_file" ]]; then
            if python3 -c "import json; json.load(open('${stage_file}'))" 2>/dev/null; then
                ok "parse-$bn" "valid JSON"
            else
                fail "parse-$bn" "invalid JSON"
            fi
        else
            warn "parse-$bn" "not found"
        fi
    done
fi

# ── JSON output ────────────────────────────────────────────────────────────────
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
    "phase": "kit_check",
    "ok":    fail_count == 0,
    "checks": checks,
}, indent=2))
PYEOF
fi

exit "$((FAIL_COUNT > 0 ? 1 : 0))"
