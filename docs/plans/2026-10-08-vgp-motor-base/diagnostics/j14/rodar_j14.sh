#!/usr/bin/env bash
# J14 (31/10/2026): judge the 30.4.101 binary (a6b9a550) on the sealed held-out
# corpus_holdout_disjunto_20261031b, with 8fd0efb0 as the discrimination control.
# The seal is checked before and after. The hybrid corpus adds live positives only
# as a liveness control; false denies count only on held-out negatives. v2 runs
# from /tmp and V3 from the analise cwd. Criterion A (raw counts): one-sided CP95
# <= 1% per bucket, --exigir-limite-superior, p95 <= 1000 ms. The HEAD judge also
# returns INCONCLUSIVE (exit 3) when layer-2 timeouts exceed the 0.20 threshold.
set -u
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
O=$B/diagnostics/j14
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/064bba23-c3ac-461e-b4f6-e2a4d998460e/scratchpad
C=$HOME/.claude/touring/vgp/corpus_holdout_disjunto_20261031b
SELO=$B/diagnostics/holdout-20261031/selo.json
CTL=$S/j14_hibrido_20261031b
I=$B/diagnostics/instrumentos-2026-10-09
INST=$I/rodar_juiz_instrumentado.py
J=$B/scripts/juiz_motor_vgp.py
HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
NOVO=$HOME/.claude/touring/vgp/binarios/a6b9a550/touring
REF=$HOME/.claude/touring/vgp/binarios/8fd0efb0/touring
R=$O/rcs.txt
: > "$R"
date -u +%FT%TZ > "$O/inicio_utc.txt"
sha256sum "$NOVO" "$REF" "$J" "$INST" "$I/montar_ctl_generico.py" "$B/scripts/confere_selo.py" > "$O/shas.txt"
python3 "$B/scripts/confere_selo.py" --corpus "$C" --selo "$SELO" > "$O/selo_antes.out" 2>&1
echo "selo_antes rc=$?" >> "$R"
grep -q "selo_antes rc=0" "$R" || { echo "ABORTADO: selo violado" >> "$R"; exit 3; }
[ -e "$CTL" ] && { echo "ABORTADO: $CTL ja existe" >> "$R"; exit 4; }
(cd /tmp && python3 -I "$I/montar_ctl_generico.py" "$C" "$CTL") > "$O/hibrido.out" 2>&1
echo "hibrido rc=$?" >> "$R"
(cd "$CTL" && find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-16) > "$O/hibrido_sha_tree.txt"
timeouts() {  # <porid.json> -> "<timeouts>/<ids>"
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); d=d.get("por_id", d); n=sum(str(r.get("camada2") or "").startswith("unknown: prazo de") for r in d.values()); print("%d/%d" % (n, len(d)))' "$1" 2> /dev/null || echo ilegivel
}
for bin in a6b9a550:$NOVO 8fd0efb0:$REF; do
  n=${bin%%:*}
  t=${bin#*:}
  (cd /tmp && TOURING_CODE_MODE=native "$HW" --class rust --label "j14-$n-v2" --wait 7200 -- \
    python3 -I "$INST" "$J" "$O/${n}_v2.porid.json" --touring "$t" --v5-estrito --corpus "$CTL" --exigir-limite-superior \
    --json "$O/${n}_v2.json" > "$O/${n}_v2.out" 2>&1)
  echo "$n v2 rc=$? $(date -Is)" >> "$R"
done
for bin in a6b9a550:$NOVO 8fd0efb0:$REF; do
  n=${bin%%:*}
  t=${bin#*:}
  (cd /home/gabrielgadea/projects/analise && TOURING_CODE_MODE=native "$HW" --class rust --label "j14-$n-v3" --wait 7200 -- \
    python3 -I "$INST" "$J" "$O/${n}_v3.porid.json" --touring "$t" --corpus "$CTL" --v5-estrito \
    --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --exigir-limite-superior --base "$O/${n}_v2.json" \
    --json "$O/${n}_v3.json" > "$O/${n}_v3.out" 2>&1)
  echo "$n V3 rc=$? timeouts_camada2=$(timeouts "$O/${n}_v3.porid.json") $(date -Is)" >> "$R"
done
python3 "$B/scripts/confere_selo.py" --corpus "$C" --selo "$SELO" > "$O/selo_depois.out" 2>&1
echo "selo_depois rc=$?" >> "$R"
date -u +%FT%TZ > "$O/fim_utc.txt"
echo FIM >> "$R"
