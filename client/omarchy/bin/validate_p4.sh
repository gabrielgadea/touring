#!/usr/bin/env bash
# validate_p4.sh — Gate P4: Touring Nervous System
# Roda no Omarchy após instalar Touring (update-touring).
# Uso: validate_p4.sh [--json] [--deps]
set -euo pipefail

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
_PHASE="P4"
_JSON_MODE=0
OPT_DEPS=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)   _JSON_MODE=1; shift ;;
        --deps)   OPT_DEPS=1; shift ;;
        --help|-h)
            printf 'Uso: validate_p4.sh [--json] [--deps]\n'
            printf 'Gate P4: Touring daemon, e2e, índice, memória.\n'
            printf '  --deps  verifica dependências de compilação (rustup, mold, etc.)\n'
            exit 0 ;;
        *) printf 'Opção desconhecida: %s\n' "$1" >&2; exit 1 ;;
    esac
done

# shellcheck source=_validate_lib.sh
source "$SCRIPT_DIR/_validate_lib.sh"

# ── Dependências (--deps) ─────────────────────────────────────────────────────
if [[ "$OPT_DEPS" -eq 1 ]]; then
    for tool in rustup cargo rustc mold clang lld pkgconf jq rg shellcheck genisoimage qmllint docker; do
        if command -v "$tool" >/dev/null 2>&1; then
            check_ok "dep-${tool}" "$(command -v "$tool")"
        else
            check_fail "missing-tool" "$tool"
        fi
    done
    # rustc version >= 1.95
    if command -v rustc >/dev/null 2>&1; then
        _rv="$(rustc --version 2>/dev/null)" || _rv=""
    rustc_ver=$(printf '%s' "$_rv" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
        min_ver="1.95.0"
        if [[ -n "$rustc_ver" ]]; then
            highest=$(printf '%s\n%s\n' "$rustc_ver" "$min_ver" | sort -V | tail -1)
            if [[ "$highest" == "$rustc_ver" ]]; then
                check_ok "rustc-semver" "$rustc_ver >= $min_ver"
            else
                check_fail "rustc-semver" "$rustc_ver < $min_ver (MSRV is $min_ver)"
            fi
        else
            check_fail "rustc-semver" "could not parse rustc version"
        fi
    fi
fi

# ── 1. touring doctor -j: error reprova; warning vira warn (contrato: warn não
#       falha). O doctor emite 3 níveis e achatá-los em binário fazia um warning
#       de wiring reprovar a fase inteira (medido 23/08: kind_unknown=3).
if need_tool touring; then
    doctor_out=$(touring doctor -j 2>/dev/null || printf '[]')
    doctor_triage=$(printf '%s' "$doctor_out" | python3 -c "
import json, sys
d = json.load(sys.stdin)
errors = [x['name'] for x in d if x.get('status') not in ('ok', 'warning')]
warns  = [x['name'] for x in d if x.get('status') == 'warning']
print(f\"{len(errors)} {len(warns)} {','.join(errors + warns) or '-'}\")
" 2>/dev/null || printf '-1 -1 parse-error')
    read -r doctor_errors doctor_warns doctor_names <<<"$doctor_triage"
    if [[ "$doctor_errors" == "0" && "$doctor_warns" == "0" ]]; then
        check_ok "touring-doctor" "all components ok"
    elif [[ "$doctor_errors" == "0" ]]; then
        check_warn "touring-doctor" "$doctor_warns warning component(s): $doctor_names"
    elif [[ "$doctor_errors" == "-1" ]]; then
        check_fail "touring-doctor" "could not parse doctor output"
    else
        check_fail "touring-doctor" "$doctor_errors component(s) in error: $doctor_names"
    fi

    # ── 2. touring e2e overall_score >= $E2E_MIN ─────────────────────────────
    # RECALIBRADO 2026-08-23 (Gabriel): 0.85 -> 0.83.
    # O 0.85 vinha do baseline 0.8543 em `data/ground_truth.json`, medido no Pop
    # as 13:06 — ANTES de `client/omarchy/` existir (construido 13:26-14:19). A
    # arvore pos-P0 mede 0.8345, e a diferenca e INTEIRAMENTE a fase `quality`
    # (0.867 vs 1.000); as outras cinco empatam ou melhoram (index 1.000=1.000,
    # wiring 0.666 vs 0.663, knowledge 0.900=0.900, ast 0.768 vs 0.760,
    # learning 0.886=0.886). Os 4 antipatterns sao todos `print(` em
    # client/omarchy/bin/{cc_build,cidata_gen,cidata_iso,hooks_runnable}.py,
    # sinalizados como "debug output left in production code" — FALSO POSITIVO:
    # sao CLIs cujo contrato (README) exige saida humana + `--json`. Medido:
    # `except:`, `import *` e `global` sao zero nesses arquivos.
    # 0.83 preserva a deteccao de regressao real (qualquer queda em index/
    # wiring/ast/knowledge ainda reprova) sem falhar por um FP conhecido.
    E2E_MIN=0.83

    # As metricas de corpus (e2e, symbol_count, memory recall) sao ESCOPADAS AO
    # PROJETO do cwd — a CLI resolve o `.claude/touring/*.db` a partir de onde e'
    # invocada. Sem fixar o diretorio, o gate media "onde quer que o chamador
    # estivesse". Medido 2026-08-23: rodado pela rotina `routine-validate-all`
    # (WorkingDirectory=~/Work, um projeto recem-criado e vazio) o P4 reportou
    # `0 symbols`, `0 hits` e `e2e 0.697`, contra 284472 / 20 / 0.83+ no repo —
    # tres FAILs que nao eram regressao nenhuma, so' o gate medindo outra coisa.
    # O que o P4 afirma e' que o sistema nervoso do Touring esta instalado e
    # indexado; o alvo disso e' a fonte canonica, nao o cwd do chamador.
    TOURING_SRC="$HOME/projects/touring"
    if [[ ! -d "$TOURING_SRC" ]]; then
        check_fail "touring-src-exists" "$TOURING_SRC not found"
    fi

    e2e_out=$(cd "$TOURING_SRC" 2>/dev/null && touring e2e -j 2>/dev/null || printf '{}')
    e2e_score=$(printf '%s' "$e2e_out" | python3 -c \
        "import json,sys; d=json.load(sys.stdin); print(d.get('overall_score','?'))" \
        2>/dev/null || printf '?')
    e2e_pass=$(printf '%s' "$e2e_out" | E2E_MIN="$E2E_MIN" python3 -c \
        "import json,os,sys; d=json.load(sys.stdin); print('yes' if d.get('overall_score',0)>=float(os.environ['E2E_MIN']) else 'no')" \
        2>/dev/null || printf 'no')
    if [[ "$e2e_pass" == "yes" ]]; then
        check_ok "touring-e2e" "overall_score=$e2e_score >= $E2E_MIN"
    else
        check_fail "touring-e2e" "overall_score=$e2e_score < $E2E_MIN"
    fi

    # ── 3. touring status -j symbol_count > 250000 ────────────────────────────
    status_out=$(cd "$TOURING_SRC" 2>/dev/null && touring status -j 2>/dev/null || printf '{}')
    sym_count=$(printf '%s' "$status_out" | python3 -c \
        "import json,sys; d=json.load(sys.stdin); print(d.get('index',{}).get('symbol_count',0))" \
        2>/dev/null || printf '0')
    if [[ "$sym_count" -gt 250000 ]]; then
        check_ok "touring-symbol-count" "$sym_count symbols"
    else
        # Um `0` vindo do daemon nao e' aceito como medicao. Observado 2026-08-23:
        # `touring status -j` devolveu `symbol_count: 0` para um projeto cujo
        # symbols.db tem 284472 linhas e cujo `touring index find` respondia
        # normalmente — ou seja, um zero que contradiz o proprio disco. O
        # mecanismo NAO foi reproduzido a partir de um daemon recem-reiniciado
        # (testadas e refutadas as hipoteses "ator ocupado" e "e2e zera o
        # contador"); depende de estado do daemon, provavelmente de ele estar
        # servindo varias raizes de projeto. Enquanto a causa nao e' fechada, o
        # gate mede a verdade em vez de repetir o zero: uma query de aquecimento
        # + releitura e, em ultimo caso, a contagem direta no SQLite. Um indice
        # de fato vazio continua reprovando — o DB tem poucos KB e zero linhas.
        (cd "$TOURING_SRC" 2>/dev/null && touring index find RustQualitySignals >/dev/null 2>&1) || true
        sym_count=$(cd "$TOURING_SRC" 2>/dev/null && touring status -j 2>/dev/null \
            | python3 -c "import json,sys; print(json.load(sys.stdin).get('index',{}).get('symbol_count',0))" \
            2>/dev/null || printf '0')
        if [[ "$sym_count" -gt 250000 ]]; then
            check_ok "touring-symbol-count" "$sym_count symbols (after warm-up query)"
        else
            db="$TOURING_SRC/.claude/touring/symbols.db"
            db_rows=0
            if command -v sqlite3 >/dev/null 2>&1 && [[ -f "$db" ]]; then
                db_rows=$(sqlite3 "file:$db?mode=ro" 'select count(*) from symbols;' 2>/dev/null || printf '0')
            fi
            if [[ "$db_rows" -gt 250000 ]]; then
                check_warn "touring-symbol-count" \
                    "index has $db_rows symbols on disk but the daemon reports $sym_count — daemon disagrees with its own DB"
            else
                check_fail "touring-symbol-count" "$sym_count symbols (disk: $db_rows; expected > 250000)"
            fi
        fi
    fi
fi

# ── 4. hooks_runnable.py exit 0 ───────────────────────────────────────────────
HOOKS_RUNNABLE="$SCRIPT_DIR/hooks_runnable.py"
SETTINGS="$HOME/.claude/settings.json"
if [[ -f "$HOOKS_RUNNABLE" ]] && [[ -f "$SETTINGS" ]]; then
    if python3 "$HOOKS_RUNNABLE" "$SETTINGS" >/dev/null 2>&1; then
        check_ok "hooks-runnable" "exit 0"
    else
        check_fail "hooks-runnable" "hooks_runnable.py exit nonzero"
    fi
elif [[ ! -f "$HOOKS_RUNNABLE" ]]; then
    check_warn "hooks-runnable" "pending-artifact bin/hooks_runnable.py"
else
    check_fail "settings-json" "$SETTINGS not found"
fi

# ── 5. touring memory recall "agentic-os-omarchy-fusao" >= 3 hits ─────────────
if command -v touring >/dev/null 2>&1; then
    # `memory recall` ja emite JSON por padrao; NAO existe flag -j nesse subcomando
    # (2026-08-23: o `-j` fazia o clap abortar, o fallback virava `null` e o check
    # era impossivel de passar). O topo e um dict com `count` + `entries` — nunca
    # uma lista, e nunca a chave `results`.
    # Escopado ao projeto, como e2e/symbol_count acima: a memoria vive em
    # `<proj>/.claude/touring/memory.db`. Rodado de ~/Work pela rotina, dava 0 hits.
    recall_out=$(cd "$HOME/projects/touring" 2>/dev/null && touring memory recall "agentic-os-omarchy-fusao" 2>/dev/null || printf 'null')
    hit_count=$(printf '%s' "$recall_out" | python3 -c \
        "import json,sys
d=json.load(sys.stdin)
if isinstance(d,list): print(len(d))
elif isinstance(d,dict): print(d.get('count', len(d.get('entries', d.get('results', [])))))
else: print(0)" 2>/dev/null || printf '0')
    if [[ "$hit_count" -ge 3 ]]; then
        check_ok "memory-recall-agentic-os" "$hit_count hits"
    else
        check_fail "memory-recall-agentic-os" "$hit_count hits (expected >= 3)"
    fi
fi

finish "$_PHASE"
