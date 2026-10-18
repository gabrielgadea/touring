#!/usr/bin/env bash
# safe-clean.sh — limpeza CIRÚRGICA de target/ Rust (REGRA #12 disk-hygiene)
# Recriado 2026-08-23 (migração Pop!_OS → Omarchy). Contrato:
# ~/.claude/skills/Touring/references/disk-hygiene-detail.md (REGRA #4).
#
# Modos:  incremental  — remove artefatos incremental/ (sempre seguro)
#         sweep        — rlibs órfãos > SAFE_CLEAN_DAYS (default 7) via cargo-sweep
#         stats        — relatório de tamanhos (nada destrutivo)
#         dry-run      — cargo clean --dry-run por workspace
# Gates:  aborta (exit 2) com cargo/rustc ativo (sweep/dry-run) · incremental toma a trava do cargo por perfil
#         · avisa com daemon touring vivo
# Log:    ~/.claude/touring/disk_cleanup.log
set -u
export LC_ALL=C

MODE="${1:-stats}"
DAYS="${SAFE_CLEAN_DAYS:-7}"
LOG="${HOME}/.claude/touring/disk_cleanup.log"
mkdir -p "$(dirname "$LOG")"

# SAFE_CLEAN_WORKSPACES (colon-separated) overrides the list — for tests against a synthetic tree, never a gate bypass
declare -a WORKSPACES=(
  "${HOME}/projects/touring"
  "${HOME}/projects/analise/packages"
  "${HOME}/projects/analise/packages/kazuba-rust-core"
  "${HOME}/projects/analise/packages/kazuba-geo-engine"
  "${HOME}/projects/analise/packages/kazuba-converters-rs"
  "${HOME}/projects/analise/packages/kazuba-sei-core"
)

if [ -n "${SAFE_CLEAN_WORKSPACES:-}" ]; then IFS=: read -r -a WORKSPACES <<< "$SAFE_CLEAN_WORKSPACES"; fi

registrar() { echo "[$(date -Is)] $*" | tee -a "$LOG"; }

# The target dir itself plus every nested target dir (CARGO_TARGET_DIR=target/wt-<worktree>): cargo marks each root
# with CACHEDIR.TAG, and a nested root keeps its incremental/ one level deeper than -maxdepth 2 reaches from target/.
# Measured 18/10/2026: ~150 GB of target/wt-*/debug/incremental were invisible to this mode while the disk hit 92%.
raizes_de_target() {
  local alvo="$1" sub
  echo "$alvo"
  for sub in "$alvo"/*/; do
    if [ -f "${sub}CACHEDIR.TAG" ]; then echo "${sub%/}"; fi
  done
}

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
#
# `incremental` is exempt too, because it no longer needs a machine-wide quiet window: it takes cargo's OWN lock
# (`flock` on `<target>/<profile>/.cargo-lock`, the lock every cargo build holds for its whole run) around each
# deletion, so it can never remove a profile a build is using, and a build that starts meanwhile waits for it.
# Measured 18/10/2026: with 10+ sessions compiling, the global gate found no 30 s window in 60 min while the disk went
# from 87 to 79 GiB free; the per-profile lock reported exactly one profile in use (wt-int97/release, where the live
# rustc was writing) and ~157 GiB of incremental in idle profiles. The lock also closes the race the pgrep check had
# (a build starting between the check and the rm).
if [ "$MODE" != "stats" ] && [ "$MODE" != "incremental" ] &&
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
      while IFS= read -r raiz; do
        for perfil in "$raiz"/*/; do
          perfil="${perfil%/}"
          [ -d "$perfil/incremental" ] || continue
          if [ ! -f "$perfil/.cargo-lock" ]; then
            registrar "preservado ${perfil}/incremental: sem .cargo-lock, a trava do cargo não pode ser tomada"
            continue
          fi
          flock -n -E 75 "$perfil/.cargo-lock" find "$perfil" -maxdepth 1 -type d -name incremental -exec rm -rf {} +
          rc_trava=$?
          if [ "$rc_trava" = 75 ]; then registrar "preservado ${perfil}/incremental: build em uso (trava do cargo ocupada)"; fi
        done
      done < <(raizes_de_target "$inc")
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
