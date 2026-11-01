#!/usr/bin/env bash
# Roda a re-rotulacao do estrato OSS (V-8, 01/11/2026) e DEIXA O LOG AO LADO da saida.
#
# O que este lote e:
#   - a MESMA adjudicator congelada (`~/.claude/touring/vgp/oss/oraculo_ls.js`), os MESMOS
#     clones (`~/.claude/touring/vgp/oss/`) e o MESMO corte temporal (2026-10-09 23:00, que o
#     rotulador tem hardcoded no `git log --before`);
#   - o teto de concentracao de 3% aplicado NA SELECAO (e nao so medido depois);
#   - a DISJUNCAO contra os corpora anteriores: `--excluir-corpus` tira da populacao os
#     pares que um corpus anterior ja rotulou, entao a camada nova nao repete a antiga.
#
# O que este lote NAO faz: nao toca o daemon global, nao roda `update-touring`, nao roda
# `propagate-release.sh`, nao faz `cargo build --release`. O AB34 esta armado ate 08/11 00:10.
set -uo pipefail

BUNDLE=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
VG=$HOME/.claude/touring/vgp

SAIDA="$VG/oss_rotulado_20261101"
LOG="$VG/oss_rotulado_20261101.log"
ALVO=450
MAX_COMMITS=600
TETO=0.03

# O `/tmp` da maquina esta com cota (medido em 17/10: tmpfs com usrquota); o rotulador
# precisa de um TMPDIR que nao seja o do sistema.
export TMPDIR="$HOME/.cache/vgp-semeado/tmp"
mkdir -p "$TMPDIR"

cd "$BUNDLE" || exit 2

echo "inicio: $(date -Is)"
echo "saida: $SAIDA"
echo "log:   $LOG"
echo "alvo=$ALVO max_commits=$MAX_COMMITS teto=$TETO"
echo "exclusao: todo o historico de corpus em $VG (menos a propria saida)"

python3 -I scripts/rotular_oss.py \
  --alvo "$ALVO" \
  --teto-concentracao "$TETO" \
  --max-commits "$MAX_COMMITS" \
  --saida "$SAIDA" \
  --excluir-corpus "$VG" \
  > "$LOG" 2> "$LOG.err"
RC=$?

echo "fim: $(date -Is) rc=$RC"
echo "--- por_lang e selo (do log) ---"
python3 -I - "$LOG" <<'PYEOF'
import json, sys
t = open(sys.argv[1], errors="replace").read()
i = t.find("{")
if i < 0:
    print("sem JSON no log; ultimas linhas:")
    print(t[-2000:])
    raise SystemExit(0)
d = json.loads(t[i:])
s = d.get("selo") or {}
print("por_lang:", d.get("por_lang"))
print("paradas:", json.dumps(d.get("paradas"), ensure_ascii=False)[:800])
print("dentro_do_teto:", (s.get("concentracao_medida") or {}).get("dentro_do_teto"))
print("exclusao:", json.dumps(s.get("exclusao"), ensure_ascii=False)[:600])
for nome, rel in (d.get("repos") or {}).items():
    st = rel.get("stats") if isinstance(rel, dict) else None
    print(" ", nome, json.dumps(st, ensure_ascii=False)[:400] if st else rel)
PYEOF
exit $RC
