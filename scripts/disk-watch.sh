#!/usr/bin/env bash
# disk-watch.sh — observabilidade de target/ Rust (REGRA #12 disk-hygiene)
# Recriado 2026-08-23 (migração Pop!_OS → Omarchy; os originais da Wave
# 2026-04-26 não sobreviveram à migração). Contrato documentado em
# ~/.claude/skills/Touring/references/disk-hygiene-detail.md (REGRA #5/#6).
#
# Output: ~/.claude/touring/disk_baseline.json (estado) +
#         ~/.claude/touring/disk_watch.log (append diário)
# Env:    DISK_WATCH_QUIET=1 (sem stdout) · DISK_WATCH_THRESHOLD_GB=50
set -u
export LC_ALL=C   # pt_BR awk emite vírgula decimal e quebra JSON (REGRA #7)

THRESHOLD_GB="${DISK_WATCH_THRESHOLD_GB:-50}"
OUT_DIR="${HOME}/.claude/touring"
BASELINE="${OUT_DIR}/disk_baseline.json"
LOG="${OUT_DIR}/disk_watch.log"
mkdir -p "$OUT_DIR"

# REGRA #6: workspace Rust novo → adicionar aqui
declare -a TARGETS=(
  "touring|${HOME}/projects/touring/target"
  "analise-packages|${HOME}/projects/analise/packages/target"
  "kazuba-rust-core|${HOME}/projects/analise/packages/kazuba-rust-core/target"
  "kazuba-geo-engine|${HOME}/projects/analise/packages/kazuba-geo-engine/target"
  "kazuba-converters-rs|${HOME}/projects/analise/packages/kazuba-converters-rs/target"
  "kazuba-sei-core|${HOME}/projects/analise/packages/kazuba-sei-core/target"
)

ts="$(date -Is)"
disco="$(df -h /home | tail -1 | awk '{print $5}')"
json="{\"timestamp\":\"${ts}\",\"disk_home_used\":\"${disco}\",\"threshold_gb\":${THRESHOLD_GB},\"targets\":["
primeiro=1
total_gb=0

for entry in "${TARGETS[@]}"; do
  nome="${entry%%|*}"; caminho="${entry##*|}"
  if [ -d "$caminho" ]; then
    kb="$(du -sk "$caminho" 2>/dev/null | awk '{print $1}')"
    gb="$(awk -v k="$kb" 'BEGIN{printf "%.2f", k/1048576}')"
  else
    gb="0.00"
  fi
  total_gb="$(awk -v a="$total_gb" -v b="$gb" 'BEGIN{printf "%.2f", a+b}')"
  [ "$primeiro" -eq 0 ] && json="${json},"
  primeiro=0
  json="${json}{\"name\":\"${nome}\",\"path\":\"${caminho}\",\"size_gb\":${gb}}"
  if awk -v g="$gb" -v t="$THRESHOLD_GB" 'BEGIN{exit !(g>t)}'; then
    echo "[${ts}] WARN ${nome} ${gb}GB > ${THRESHOLD_GB}GB — rodar safe-clean.sh incremental; se target/debug seguir grande, cargo clean --profile dev sem build ativo (sweep aborta com o daemon vivo)" | tee -a "$LOG" >&2
  fi
done
json="${json}],\"total_gb\":${total_gb}}"

echo "$json" > "$BASELINE"
echo "[${ts}] total=${total_gb}GB disco_home=${disco}" >> "$LOG"
[ "${DISK_WATCH_QUIET:-0}" = "1" ] || echo "$json"
