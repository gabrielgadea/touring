#!/usr/bin/env bash
# safe-clean.sh — limpeza CIRÚRGICA de target/ Rust (REGRA #12 disk-hygiene)
# Recriado 2026-08-23 (migração Pop!_OS → Omarchy). Contrato:
# ~/.claude/skills/Touring/references/disk-hygiene-detail.md (REGRA #4).
#
# Modos:  incremental  — remove artefatos incremental/ (sempre seguro)
#         sweep        — rlibs órfãos > SAFE_CLEAN_DAYS (default 7) via cargo-sweep
#         stats        — relatório de tamanhos (nada destrutivo)
#         dry-run      — cargo clean --dry-run por workspace
# Gates:  aborta (exit 2) com cargo/rustc ativo · avisa com daemon touring vivo
# Log:    ~/.claude/touring/disk_cleanup.log
set -u
export LC_ALL=C

MODE="${1:-stats}"
DAYS="${SAFE_CLEAN_DAYS:-7}"
LOG="${HOME}/.claude/touring/disk_cleanup.log"
mkdir -p "$(dirname "$LOG")"

declare -a WORKSPACES=(
  "${HOME}/projects/touring"
  "${HOME}/projects/analise/packages"
  "${HOME}/projects/analise/packages/kazuba-rust-core"
  "${HOME}/projects/analise/packages/kazuba-geo-engine"
  "${HOME}/projects/analise/packages/kazuba-converters-rs"
  "${HOME}/projects/analise/packages/kazuba-sei-core"
)

registrar() { echo "[$(date -Is)] $*" | tee -a "$LOG"; }

# Gate anti-build-vivo (exit 2). `stats` é isento: ele só roda `du -sh` e não
# apaga nada — abortar uma MEDIÇÃO durante um build negava a leitura exatamente
# quando ela é mais útil (querer saber o tamanho do target enquanto ele cresce).
# O gate existe contra remoção concorrente, não contra observar.
#
# O predicado casa o NOME DO EXECUTÁVEL (`pgrep -x`, que compara `comm`), nunca a
# linha de comando. `pgrep -f "cargo (build|…)"` casava qualquer processo cuja
# LINHA contivesse o texto — inclusive o shell que invoca esta limpeza, se ele
# tiver `cargo build` escrito nele. Medido em 16/09/2026: três tentativas seguidas
# abortaram com "cargo/rustc ativo" sem nenhum build existir; os dois PIDs
# acusados tinham `comm=bash`, e um era um laço de espera que continha a string e
# casava consigo mesmo. Um gate que se autobloqueia é pior que um gate ausente,
# porque a mensagem culpa o sistema e o operador acredita nela. Mesma família de
# "Cargo" casar com a busca por "rg".
if [ "$MODE" != "stats" ] &&
   { pgrep -x rustc >/dev/null 2>&1 || pgrep -x cargo >/dev/null 2>&1; }; then
  registrar "ABORT: cargo/rustc ativo — limpeza durante build vivo é o principal causador de erros"
  registrar "       pids: $(pgrep -x rustc 2>/dev/null | tr '\n' ' ')$(pgrep -x cargo 2>/dev/null | tr '\n' ' ')"
  exit 2
fi
# Gate daemon. A mensagem antiga era impressa AQUI, antes do `case`, e prometia
# "target/release preservado" para TODOS os modos — mas só `incremental` (que
# toca apenas diretórios `incremental/`) cumpre isso. `sweep` roda
# `cargo sweep --time N` sobre o target inteiro e remove artefatos de release
# por idade: com o daemon vivo, isso apaga o binário que ele está executando —
# exatamente o "running deleted binary" que o update-touring reporta. A promessa
# passou a ser feita só por quem a cumpre (D8: o anúncio não promete o que o
# executor não aplica).
DAEMON_VIVO=0
if pgrep -x touring-daemon >/dev/null 2>&1; then DAEMON_VIVO=1; fi

case "$MODE" in
  incremental)
    [ "$DAEMON_VIVO" = 1 ] && registrar "daemon vivo — modo incremental toca apenas incremental/; target/release preservado"
    for ws in "${WORKSPACES[@]}"; do
      inc="${ws}/target"
      [ -d "$inc" ] || continue
      antes="$(du -sk "$inc" | awk '{print $1}')"
      find "$inc" -maxdepth 2 -type d -name incremental -exec rm -rf {} + 2>/dev/null
      depois="$(du -sk "$inc" | awk '{print $1}')"
      registrar "incremental ${ws}: $(( (antes-depois)/1024 ))MB liberados"
    done
    ;;
  sweep)
    # Fail-closed: sweep varre o target INTEIRO por idade, release incluso.
    if [ "$DAEMON_VIVO" = 1 ] && [ "${SWEEP_OK:-0}" != "1" ]; then
      registrar "ABORT: touring-daemon vivo e sweep removeria artefatos de release por idade — o daemon passaria a rodar um binário deletado. Pare o daemon (touring daemon-ctl stop) ou reexecute com SWEEP_OK=1 assumindo o risco."
      exit 2
    fi
    if ! command -v cargo-sweep >/dev/null 2>&1; then
      registrar "sweep: cargo-sweep AUSENTE — instalar: cargo install cargo-sweep; nada feito (falha explícita, não silêncio)"
      exit 1
    fi
    for ws in "${WORKSPACES[@]}"; do
      [ -f "${ws}/Cargo.toml" ] || continue
      registrar "sweep ${ws} (> ${DAYS}d)"
      (cd "$ws" && cargo sweep --time "$DAYS" 2>&1 | tail -2 | tee -a "$LOG")
    done
    ;;
  stats)
    for ws in "${WORKSPACES[@]}"; do
      # if-fi, não `&&`: workspace sem target/ no último item vazava rc=1
      # como exit do script (stats funcionava e reportava falha)
      if [ -d "${ws}/target" ]; then du -sh "${ws}/target"; fi
    done
    ;;
  dry-run)
    for ws in "${WORKSPACES[@]}"; do
      [ -f "${ws}/Cargo.toml" ] || continue
      echo "── ${ws}"
      (cd "$ws" && cargo clean --dry-run 2>&1 | tail -3)
    done
    ;;
  *)
    echo "uso: safe-clean.sh {incremental|sweep|stats|dry-run}" >&2
    exit 64
    ;;
esac
exit 0
