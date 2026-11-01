#!/usr/bin/env bash
# J15 (31/10/2026): judge the installed 30.4.102 binary on the sealed held-out
# corpus_holdout_disjunto_20261101, with 8fd0efb0 as the discrimination control.
# Cold runs preserve the judge's recall/false-deny/comparative evidence; their
# p95 is informational only. The preregistered latency verdict comes from the
# live hot daemon: p95 <= 1000 ms, with layer-2 timeout/signal fail-closed.
set -u
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
O=$B/diagnostics/j15
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/064bba23-c3ac-461e-b4f6-e2a4d998460e/scratchpad
C=$HOME/.claude/touring/vgp/corpus_holdout_disjunto_20261101
SELO=$B/diagnostics/holdout-20261101/selo.json
CTL=$S/j15_hibrido_20261101
I=$B/diagnostics/instrumentos-2026-10-09
INST=$I/rodar_juiz_instrumentado.py
J=$B/scripts/juiz_motor_vgp.py
HOT=$B/diagnostics/j14/medir_daemon_quente.py
HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
NOVO=$HOME/.claude/touring/vgp/binarios/30.4.102/touring
REF=$HOME/.claude/touring/vgp/binarios/8fd0efb0/touring
R=$O/rcs.txt
[ -e "$R" ] && { echo "ABORTADO: $R ja existe; preserve o run anterior"; exit 4; }
: > "$R"
date -u +%FT%TZ > "$O/inicio_utc.txt"
sha256sum "$NOVO" "$REF" "$J" "$INST" "$I/montar_ctl_generico.py" "$HOT" "$B/scripts/confere_selo.py" > "$O/shas.txt"
"$NOVO" --version > "$O/versao_30_4_102.txt" 2>&1
python3 "$B/scripts/confere_selo.py" --corpus "$C" --selo "$SELO" > "$O/selo_antes.out" 2>&1
echo "selo_antes rc=$?" >> "$R"
grep -q "selo_antes rc=0" "$R" || { echo "ABORTADO: selo violado" >> "$R"; exit 3; }
[ -e "$CTL" ] && { echo "ABORTADO: $CTL ja existe" >> "$R"; exit 4; }
(cd /tmp && python3 -I "$I/montar_ctl_generico.py" "$C" "$CTL") > "$O/hibrido.out" 2>&1
echo "hibrido rc=$?" >> "$R"
grep -q "hibrido rc=0" "$R" || { echo "ABORTADO: hibrido falhou" >> "$R"; exit 4; }
(cd "$CTL" && find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-16) > "$O/hibrido_sha_tree.txt"
timeouts() {  # <porid.json> -> "<timeouts>/<ids>"
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); d=d.get("por_id", d); n=sum(str(r.get("camada2") or "").startswith("unknown: prazo de") for r in d.values()); print("%d/%d" % (n, len(d)))' "$1" 2> /dev/null || echo ilegivel
}
for bin in 30_4_102:$NOVO 8fd0efb0:$REF; do
  n=${bin%%:*}
  t=${bin#*:}
  (cd /tmp && TOURING_CODE_MODE=native "$HW" --class rust --label "j15-$n-v2" --wait 7200 -- \
    python3 -I "$INST" "$J" "$O/${n}_v2.porid.json" --touring "$t" --v5-estrito --corpus "$CTL" --exigir-limite-superior \
    --json "$O/${n}_v2.json" > "$O/${n}_v2.out" 2>&1)
  echo "$n v2 rc=$? $(date -Is)" >> "$R"
done
for bin in 30_4_102:$NOVO 8fd0efb0:$REF; do
  n=${bin%%:*}
  t=${bin#*:}
  (cd /home/gabrielgadea/projects/analise && TOURING_CODE_MODE=native "$HW" --class rust --label "j15-$n-v3" --wait 7200 -- \
    python3 -I "$INST" "$J" "$O/${n}_v3.porid.json" --touring "$t" --corpus "$CTL" --v5-estrito \
    --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --exigir-limite-superior --base "$O/${n}_v2.json" \
    --json "$O/${n}_v3.json" > "$O/${n}_v3.out" 2>&1)
  echo "$n V3 rc=$? timeouts_camada2=$(timeouts "$O/${n}_v3.porid.json") $(date -Is)" >> "$R"
done
TOURING_CODE_MODE=native "$HW" --class medida --label j15-hot --wait 7200 -- \
  python3 -I "$HOT" --corpus "$CTL" --saida "$O/hot.json" --por-id "$O/hot.porid.json" --trace "$O/hot.trace.jsonl" > "$O/hot.out" 2>&1
echo "hot rc=$? $(date -Is)" >> "$R"
python3 "$B/scripts/confere_selo.py" --corpus "$C" --selo "$SELO" > "$O/selo_depois.out" 2>&1
echo "selo_depois rc=$?" >> "$R"
date -u +%FT%TZ > "$O/fim_utc.txt"
echo FIM >> "$R"
