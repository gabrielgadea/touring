#!/usr/bin/env bash
# propagate-release.sh — consolida uma alteração da fonte canônica em uma
# ATUALIZAÇÃO DE SISTEMA e a propaga, de forma reprodutível, a todos os
# projetos consumidores.
#
# Contexto (topologia rustup-like, ver ~/.claude/rules/touring-per-project.md):
#   L1 fonte      ~/projects/touring            (este workspace)
#   L2 toolchain  ~/.touring/toolchains/<v>/    (snapshot IMUTÁVEL)
#   L3 shim CC    ~/.claude/hooks/touring-hook  (walk-up)
#   L4 projeto    <proj>/.touring/              (pin + lock + bins + daemon)
#
# O gap que este script fecha: `update-touring` cobre APENAS L1→dev (build,
# install em ~/.local/bin + ~/.claude/hooks, restart, verify). Ele NÃO instala
# toolchain (L2) nem propaga para os projetos (L4) — esses eram dois passos
# manuais, logo não-reprodutíveis. Aqui a cadeia inteira vira um comando.
#
# USO:
#   scripts/propagate-release.sh 30.4.0                # pipeline completo
#   scripts/propagate-release.sh 30.4.0 --dry-run      # mostra sem aplicar
#   scripts/propagate-release.sh 30.4.0 --skip-build   # binário já instalado
#   scripts/propagate-release.sh 30.4.0 --skip-gates   # pula cargo check/clippy
#   scripts/propagate-release.sh 30.4.0 --skip-freeze  # toolchain já instalada
#   scripts/propagate-release.sh 30.4.0 --no-default   # não muda canal default
#   scripts/propagate-release.sh --rollback            # reverte todos os projetos
#
# ETAPAS:
#   1. gates      cargo check + clippy -D warnings (REGRA #21 — 0 falhas)
#   2. build      update-touring  (L1 → dev: build + install dual-target + restart)
#   3. freeze     touring toolchain install --from-source . <v> --force  (L2)
#   4. default    touring toolchain default <v>                          (L2)
#   5. propagate  touring update --all-projects                          (L4)
#   6. verify     cada projeto reporta a versão resolvida
#
# ROLLBACK: `--rollback` chama `touring update --all-projects --rollback`, que
# usa o campo `previous` de cada .touring/toolchain.lock (determinístico).
#
# REGRA #19: daemons são tocados SOMENTE via `touring update` / `daemon-ctl`.
# Nenhum pkill/kill aqui, jamais.
set -euo pipefail

WORKSPACE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION=""
DRY_RUN=0
SKIP_BUILD=0
SKIP_GATES=0
SKIP_FREEZE=0
NO_DEFAULT=0
ROLLBACK=0

RED=$'\033[31m'; GREEN=$'\033[32m'; YELLOW=$'\033[33m'; BOLD=$'\033[1m'; RESET=$'\033[0m'
[ -n "${NO_COLOR:-}" ] && { RED=""; GREEN=""; YELLOW=""; BOLD=""; RESET=""; }

log()  { echo "${BOLD}[propagate]${RESET} $*"; }
warn() { echo "${YELLOW}[propagate] WARN${RESET} $*" >&2; }
die()  { echo "${RED}[propagate] ERRO${RESET} $*" >&2; exit 1; }
step() { echo; echo "${BOLD}── $* ──${RESET}"; }

usage() { sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }

# Lista os roots registrados em ~/.claude/touring/projects.json (a mesma fonte
# que `touring update --all-projects` consome), um por linha.
list_projects() {
    python3 -c "
import json,os
p=os.path.expanduser('~/.claude/touring/projects.json')
try:
    d=json.load(open(p))
    entries = d if isinstance(d,list) else d.get('projects', d.get('entries', []))
    for e in entries:
        root = e if isinstance(e,str) else e.get('root', e.get('path',''))
        if root: print(root)
except Exception:
    pass
"
}

for arg in "$@"; do
    case "$arg" in
        --dry-run)    DRY_RUN=1 ;;
        --skip-build)  SKIP_BUILD=1 ;;
        --skip-gates)  SKIP_GATES=1 ;;
        --skip-freeze) SKIP_FREEZE=1 ;;
        --no-default) NO_DEFAULT=1 ;;
        --rollback)   ROLLBACK=1 ;;
        -h|--help)    usage ;;
        -*)           die "flag desconhecida: $arg" ;;
        *)            VERSION="$arg" ;;
    esac
done

run() {
    if [ "$DRY_RUN" -eq 1 ]; then
        echo "  ${YELLOW}[dry-run]${RESET} $*"
    else
        echo "  ${BOLD}\$${RESET} $*"
        "$@"
    fi
}

# ─── ROLLBACK ────────────────────────────────────────────────────────────────
if [ "$ROLLBACK" -eq 1 ]; then
    step "ROLLBACK — revertendo todos os projetos ao toolchain anterior"
    run touring update --all-projects --rollback
    log "${GREEN}rollback concluído${RESET} — confira com: touring update --all-projects --dry-run"
    exit 0
fi

[ -n "$VERSION" ] || die "versão obrigatória. Ex: $(basename "$0") 30.4.0  (--help para detalhes)"
[ -f "$WORKSPACE/Cargo.toml" ] || die "não parece a fonte canônica: $WORKSPACE/Cargo.toml ausente"

log "workspace : $WORKSPACE"
log "versão    : $VERSION"
[ "$DRY_RUN" -eq 1 ] && warn "modo DRY-RUN — nada será aplicado"

# ─── 1. GATES (REGRA #21 — zero falhas antes de virar update de sistema) ─────
if [ "$SKIP_GATES" -eq 1 ]; then
    warn "gates pulados (--skip-gates)"
else
    step "1/6 GATES — cargo check + clippy"
    if [ "$DRY_RUN" -eq 1 ]; then
        echo "  ${YELLOW}[dry-run]${RESET} cargo check --workspace && cargo clippy --workspace -- -D warnings"
    else
        ( cd "$WORKSPACE" && cargo check --workspace ) \
            || die "cargo check falhou — não se propaga build quebrada"
        ( cd "$WORKSPACE" && cargo clippy --workspace -- -D warnings ) \
            || die "clippy falhou (-D warnings) — corrija antes de propagar"
        log "${GREEN}gates OK${RESET}"
    fi
fi

# ─── 2. BUILD + INSTALL dev (L1) ─────────────────────────────────────────────
if [ "$SKIP_BUILD" -eq 1 ]; then
    warn "build pulado (--skip-build)"
else
    step "2/6 BUILD — update-touring (kill → build → install dual-target → restart → verify)"
    command -v update-touring >/dev/null 2>&1 \
        || die "update-touring não encontrado no PATH (esperado em ~/.local/bin)"
    run update-touring
fi

# ─── 3. FREEZE toolchain imutável (L2) ───────────────────────────────────────
if [ "$SKIP_FREEZE" -eq 1 ]; then
    warn "freeze pulado (--skip-freeze) — toolchain $VERSION deve já existir em ~/.touring/toolchains/"
else
    step "3/6 FREEZE — snapshot imutável em ~/.touring/toolchains/$VERSION"
    run touring toolchain install --from-source "$WORKSPACE" "$VERSION" --force
fi

# ─── 4. DEFAULT channel (L2) ─────────────────────────────────────────────────
if [ "$NO_DEFAULT" -eq 1 ]; then
    warn "canal default inalterado (--no-default)"
else
    step "4/6 DEFAULT — canal default → $VERSION"
    run touring toolchain default "$VERSION"
fi

# ─── 5. PROPAGATE para todos os projetos (L4) ────────────────────────────────
#
# GOTCHA verificado (2026-08-02): `touring update --all-projects` SEM canal
# explícito resolve pelo lock de cada projeto — ou seja, mantém a versão velha
# e NÃO propaga a nova. Já `touring update <v> --all-projects` COM canal
# propaga, mas arrasta também o workspace FONTE de `dev` para <v>, quebrando o
# desenvolvimento iterativo (a fonte precisa continuar em `dev`).
# Por isso iteramos projeto a projeto, pulando o workspace fonte.
step "5/6 PROPAGATE — canal $VERSION para cada projeto consumidor (fonte permanece em 'dev')"
PROPAGATED=0
PROP_FAILED=0
while IFS= read -r proj; do
    [ -n "$proj" ] || continue
    if [ "$(readlink -f "$proj" 2>/dev/null)" = "$(readlink -f "$WORKSPACE")" ]; then
        log "pulando fonte canônica $(basename "$proj") — permanece no canal 'dev'"
        continue
    fi
    rc=0
    if [ "$DRY_RUN" -eq 1 ]; then
        out="$(touring update "$VERSION" --project "$proj" --dry-run 2>&1)" || rc=$?
    else
        out="$(touring update "$VERSION" --project "$proj" 2>&1)" || rc=$?
    fi
    echo "$out" | grep -v ' INFO ' || true
    if [ "$rc" -ne 0 ]; then
        # Em dry-run a toolchain ainda não existe (o passo 3 também foi simulado):
        # isso é esperado, não é falha do pipeline.
        if [ "$DRY_RUN" -eq 1 ] && echo "$out" | grep -q 'is not installed'; then
            warn "esperado em dry-run: toolchain $VERSION ainda não instalada (o passo 3/6 a criaria)"
        else
            warn "FALHA ao propagar para $(basename "$proj")"
            PROP_FAILED=$((PROP_FAILED+1))
        fi
    fi
    PROPAGATED=$((PROPAGATED+1))
done < <(list_projects)
log "projetos alcançados: $PROPAGATED"
[ "$PROP_FAILED" -eq 0 ] || die "$PROP_FAILED projeto(s) falharam na propagação — nada é declarado pronto com falha aberta (REGRA #21)"

# ─── 6. VERIFY ───────────────────────────────────────────────────────────────
step "6/6 VERIFY — versão resolvida por projeto"
if [ "$DRY_RUN" -eq 1 ]; then
    echo "  ${YELLOW}[dry-run]${RESET} (verificação roda apenas em execução real)"
else
    FAIL=0
    while IFS= read -r proj; do
        [ -n "$proj" ] || continue
        name="$(basename "$proj")"
        bin="$proj/.touring/bin/touring"
        lock="$(grep -m1 '^active' "$proj/.touring/toolchain.lock" 2>/dev/null | cut -d'"' -f2 || echo '?')"
        if [ -x "$bin" ]; then
            # GOTCHA (verificado 2026-08-02): `touring --version` escreve em
            # STDERR, não stdout — `2>/dev/null` apagaria a versão inteira e o
            # gate passaria sem verificar nada. Daí o 2>&1.
            ver="$($bin --version 2>&1 | head -1)"
            [ -n "$ver" ] || ver="ERRO (sem saída de --version)"
        else
            ver="(sem bin local)"
        fi
        printf "  %-26s lock=%-10s %s\n" "$name" "$lock" "$ver"
        case "$ver" in ERRO*) FAIL=1 ;; esac
    done < <(list_projects)
    [ "$FAIL" -eq 0 ] || die "ao menos um projeto não resolveu o binário"
    log "${GREEN}propagação completa e verificada${RESET}"
    echo
    log "rollback determinístico: $(basename "$0") --rollback"
fi
