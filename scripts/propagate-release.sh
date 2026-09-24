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
#   scripts/propagate-release.sh <versão>              # pipeline completo
#   scripts/propagate-release.sh <versão> --dry-run    # mostra sem aplicar
#   scripts/propagate-release.sh <versão> --skip-build # binário já instalado
#   scripts/propagate-release.sh <versão> --skip-gates # pula cargo check/clippy/espelho
#   scripts/propagate-release.sh <versão> --skip-freeze# toolchain já instalada
#   scripts/propagate-release.sh <versão> --no-default # não muda canal default
#   scripts/propagate-release.sh <versão> --allow-no-projects # aceita 0 consumidores
#
# <versão> é um ARGUMENTO, nunca um exemplo a copiar: estes exemplos citavam
# 30.4.0 por escrito, e depois do release seguinte copiá-los REBAIXARIA todos os
# projetos consumidores para a versão antiga. A versão corrente do workspace
# está em Cargo.toml [workspace.package].
#   scripts/propagate-release.sh --rollback            # reverte todos os projetos
#
# ETAPAS:
#   1. gates      cargo check + clippy -D warnings + espelho client/
#                 + update-touring versionado/escopado (REGRA #21)
#   2. build      update-touring  (L1 → dev: build + install dual-target + restart)
#   3. freeze     touring toolchain install --from-source . <v> --force  (L2)
#   4. default    touring toolchain default <v>                          (L2)
#   5. propagate  touring update <v> --project <p>, projeto a projeto      (L4)
#                 (NAO --all-projects: ver o GOTCHA no corpo do passo 5)
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
# Só permite terminar com 0 projetos quando isso for uma decisão explícita
# (ver o guard fail-closed no passo 5).
ALLOW_NO_PROJECTS=0

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
        --allow-no-projects) ALLOW_NO_PROJECTS=1 ;;
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

# A sugestão vem do Cargo.toml, nunca de um literal: um exemplo escrito à mão
# envelhece para uma versão ANTERIOR, e copiá-lo rebaixa a frota.
[ -n "$VERSION" ] || die "versão obrigatória. Ex: $(basename "$0") $(grep -m1 '^version = ' "$WORKSPACE/Cargo.toml" 2>/dev/null | cut -d'"' -f2)  (--help para detalhes)"
[ -f "$WORKSPACE/Cargo.toml" ] || die "não parece a fonte canônica: $WORKSPACE/Cargo.toml ausente"

log "workspace : $WORKSPACE"
log "versão    : $VERSION"
[ "$DRY_RUN" -eq 1 ] && warn "modo DRY-RUN — nada será aplicado"

# ─── 1. GATES (REGRA #21 — zero falhas antes de virar update de sistema) ─────
if [ "$SKIP_GATES" -eq 1 ]; then
    warn "gates pulados (--skip-gates)"
else
    step "1/6 GATES — cargo check + clippy + espelho client/ + update-touring"
    if [ "$DRY_RUN" -eq 1 ]; then
        echo "  ${YELLOW}[dry-run]${RESET} cargo check --workspace && cargo clippy --workspace -- -D warnings"
        echo "  ${YELLOW}[dry-run]${RESET} python3 scripts/sync-client-skills.py --check"
        echo "  ${YELLOW}[dry-run]${RESET} python3 -m pytest scripts/test_update_touring.py -q"
        echo "  ${YELLOW}[dry-run]${RESET} python3 -m pytest scripts/test_touring_quality_score.py -q"
    else
        ( cd "$WORKSPACE" && cargo check --workspace ) \
            || die "cargo check falhou — não se propaga build quebrada"
        ( cd "$WORKSPACE" && cargo clippy --workspace -- -D warnings ) \
            || die "clippy falhou (-D warnings) — corrija antes de propagar"
        # O espelho client/ é a cópia versionada das skills que rodam em
        # ~/.claude/. Nada o mantinha em dia: nascido de uma cópia em massa em
        # 25/07/2026, divergiu por três semanas — e foi assim que a correção de
        # shell injection chegou ao espelho e às instanciações mas nunca à
        # biblioteca IMPLANTADA, de onde `adw from-template` copia. Um release
        # que publica um espelho velho publica código que ninguém está rodando.
        ( cd "$WORKSPACE" && python3 scripts/sync-client-skills.py --check ) \
            || die "espelho client/ fora de sincronia — rode: python3 scripts/sync-client-skills.py --apply"
        # A ferramenta de deploy também é código. Ela viveu até 18/08/2026 apenas
        # em ~/.local/bin/, e o passo 2/6 abaixo a invoca pelo PATH: se lá houver
        # uma cópia solta em vez do symlink para scripts/update-touring, este
        # release roda um deploy que ninguém revisou. Aqui, diferente do CI, o
        # lado vivo existe — então o teste verifica o symlink de verdade.
        # UPDATE_TOURING_REQUIRE_SYMLINK=1: no release o teste do symlink não
        # pode virar skip silencioso — se ~/.local/bin/update-touring não
        # existe, o passo 2/6 executaria pelo PATH uma cópia não revisada.
        ( cd "$WORKSPACE" && UPDATE_TOURING_REQUIRE_SYMLINK=1 python3 -m pytest scripts/test_update_touring.py -q ) \
            || die "update-touring divergiu do repo — rode: ln -sfn $WORKSPACE/scripts/update-touring ~/.local/bin/update-touring"
        # O mesmo vale para o wrapper de qualidade que o juiz de convergência
        # chama pelo PATH (versionado em 14/09/2026, depois de um `flock -n`
        # mudo fazer o juiz ler "qualidade não medida").
        ( cd "$WORKSPACE" && TOURING_QUALITY_SCORE_REQUIRE_SYMLINK=1 python3 -m pytest scripts/test_touring_quality_score.py -q ) \
            || die "touring-quality-score divergiu do repo — rode: ln -sfn $WORKSPACE/scripts/touring-quality-score ~/.local/bin/touring-quality-score"
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

# ─── 2.5 LABEL GUARD — a toolchain nunca declara outra versão (2026-09-24) ────
# Família propagacao-rotulo-nao-prova-build / bump-de-versao-antes-do-build:
# `.touring/bin/touring --version` É a verdade local de um projeto — a
# toolchain 30.4.66 instalada hoje se declarava 30.4.65 (o bump nunca foi
# feito), e o diagnóstico de amanhã concluiria que a release não chegou.
# `--version` escreve em STDERR (gotcha já pago): um `2>/dev/null` apagaria a
# string e a guarda passaria sem checar nada.
if [ "$SKIP_BUILD" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
    # `--version` é MULTI-LINHA (versão + sha + data + features): ler tudo e
    # cortar a 1ª linha em bash puro — um `| head -1` fecha o pipe cedo e o
    # binário morre de EPIPE/SIGABRT (a classe já documentada no passo 6), e um
    # `awk '{print $NF}'` na saída inteira lê o último campo das FEATURES.
    ver="$("$WORKSPACE/target/release/touring" --version 2>&1)"
    BUILT_VERSION="$(awk '{print $NF}' <<< "${ver%%$'\n'*}")"
    if [ "$BUILT_VERSION" != "$VERSION" ]; then
        die "rótulo divergente: o binário construído declara '$BUILT_VERSION', alvo '$VERSION' — faça o bump no Cargo.toml do workspace (version = \"$VERSION\") antes de propagar"
    fi
    log "rótulo OK: o binário declara $VERSION"
    # O lock também é rótulo (os 4 bumps anteriores o levaram junto): um membro
    # do workspace fora da versão alvo é a mesma mentira um diretório abaixo —
    # o CI não usa --locked, mas o próximo build deixaria diff solto no tree.
    LOCK_OLD="$(awk '/^name = "touring/ {getline; if ($0 ~ /version = "/ && $0 !~ "\"'"$VERSION"'\"") {print $0; exit}}' "$WORKSPACE/Cargo.lock" 2>/dev/null || true)"
    if [ -n "$LOCK_OLD" ]; then
        die "Cargo.lock fora do rótulo: $(echo "$LOCK_OLD" | head -1) ≠ $VERSION — o build do passo 2 regrava o lock; commite-o com caminho explícito"
    fi
elif [ "$DRY_RUN" -eq 1 ]; then
    warn "dry-run: a checagem de rótulo não roda (build simulado)"
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
# Zero projetos NÃO é sucesso — é o sinal de que o passo que dá nome a este
# script não aconteceu. `list_projects()` lê ~/.claude/touring/projects.json e
# engole qualquer erro (`except: pass`), então um registro ausente ou vazio
# chegava aqui indistinguível de "nada a fazer", e o pipeline seguia para o
# VERIFY declarando sucesso sem ter propagado para ninguém.
#
# Observado em 24/08/2026: o registro simplesmente não existia; `analise` e
# `konverter` tinham `.touring/` no disco, mas `touring projects list` devolvia
# `count: 0`. O dry-run dizia "projetos alcançados: 0" e teria terminado verde.
# Fail-closed, como a Lei L2: sinal ausente ≠ zero.
if [ "$PROPAGATED" -eq 0 ] && [ "$ALLOW_NO_PROJECTS" -eq 0 ]; then
    die "nenhum projeto consumidor alcançado — registre-os com \`touring projects add <alias> <path>\` \
(o registro vive em ~/.claude/touring/projects.json) ou passe --allow-no-projects se a ausência for intencional"
fi
[ "$PROP_FAILED" -eq 0 ] || die "$PROP_FAILED projeto(s) falharam na propagação — nada é declarado pronto com falha aberta (REGRA #21)"

# ─── 5.5 PROVA COMPORTAMENTAL (code mode + CEG) ──────────────────────────────
#
# O rótulo da toolchain NÃO prova o build — 24/08/2026 o lock dizia 30.4.14
# rodando binário de 4 dias antes. A prova é sempre COMPORTAMENTAL: exercitar
# o contrato contra o binário instalado e julgar o que ele devolveu.
#
# Esta bateria cobre as duas metades que mais custaram para
# ficar certas: o CEG discriminando benigno de perigoso (rede e padrão
# destrutivo negam DURO; builtins passam limpos) e o code mode entregando o
# que promete (transporte, 71 hooks, piso do --brief, par start/settle).
# Precisa do lado vivo — por isso mora aqui, e não no CI.
step "5.5/6 PROVA — contrato de code mode + CEG contra o binário instalado"
if [ "$DRY_RUN" -eq 1 ]; then
    echo "  ${YELLOW}[dry-run]${RESET} (a prova roda apenas em execução real)"
elif [ ! -f "$WORKSPACE/scripts/prova_code_mode_ceg.py" ]; then
    echo "  ${YELLOW}(prova ausente — pulada)${RESET}"
else
    # O passo 5 reinicia o daemon global E um por projeto. Rodar a prova em
    # cima do restart mede o transiente, não o contrato — e um gate que reprova
    # por transiente é um gate que as pessoas aprendem a ignorar (a fadiga que
    # a REGRA #19 já documenta: "daemon degraded ≠ daemon inexistente").
    #
    # ESTREIA REAL (28/08/2026, propagação 30.4.17): a espera de 20s esgotou com
    # o project actor ainda drenando (o index de ~350 MB recarrega após o restart)
    # e a prova mediu o transiente — 27/35, todos os probes do SDK em None.
    # Re-rodada com o daemon estável: 35/35. A espera era best-effort SEM
    # verificação final: esgotar o teto caía direto na prova. Agora o esgotamento
    # é anunciado e a prova tem UM retry após re-espera — transiente vira atraso;
    # falha real reprova duas vezes seguidas e mata o script igual.
    wait_doctor_clean() {
        for _ in $(seq 1 "$1"); do
            if touring doctor -j 2>/dev/null | grep -q '"status": *"ok"'; then
                if ! touring doctor -j 2>/dev/null | grep -q '"status": *"error"'; then
                    return 0
                fi
            fi
            sleep 1
        done
        return 1
    }
    run_prova() {
        python3 "$WORKSPACE/scripts/prova_code_mode_ceg.py" >/tmp/prova-code-mode.log 2>&1
    }
    wait_doctor_clean 20 \
        || echo "  ${YELLOW}(doctor ainda degradado após 20s — se a prova falhar, re-espera + retry)${RESET}"
    if run_prova; then
        log "prova comportamental: $(tail -2 /tmp/prova-code-mode.log | head -1)"
    else
        echo "  ${YELLOW}(prova falhou; re-esperando o doctor limpar — teto 60s — e re-rodando 1×)${RESET}"
        wait_doctor_clean 60 || true
        if run_prova; then
            log "prova comportamental (2ª tentativa, pós-transiente): $(tail -2 /tmp/prova-code-mode.log | head -1)"
        else
            tail -20 /tmp/prova-code-mode.log
            die "a prova comportamental falhou 2× — o binário instalado não cumpre o contrato (REGRA #21)"
        fi
    fi
fi

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
            # GOTCHA 1 (verificado 2026-08-02): `touring --version` escreve em
            # STDERR, não stdout — `2>/dev/null` apagaria a versão inteira e o
            # gate passaria sem verificar nada. Daí o 2>&1.
            #
            # GOTCHA 2 (verificado 2026-08-03): NÃO usar `| head -1` aqui.
            # `--version` escreve 5 linhas; o `head` fecha o pipe após a
            # primeira, o processo recebe EPIPE, `println!` entra em panic e
            # `panic = "abort"` vira SIGABRT — que `pipefail` propaga e `set -e`
            # transforma em morte do script (exit 134). Reproduzido: 3 em 200.
            # A correção de campo veio no binário (SIG_DFL para CLI), mas o
            # script não depende dela: lê tudo e corta a primeira linha em
            # bash puro, sem pipe para fechar cedo.
            ver="$("$bin" --version 2>&1)"
            ver="${ver%%$'\n'*}"
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
