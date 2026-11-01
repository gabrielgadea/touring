#!/usr/bin/env bash
# J13 (18/10/2026): the INTEGRATED binary (INT97 + P0B + FRAG + P0B-d + routing + ECC patches, release 5c7b3d80) on the
# sealed held-out corpus_holdout_disjunto_20261013f (seal 2f9282aa59a0566f). Same procedure as P0M (rodar_p0m2.sh):
# seal checked before and after, HYBRID corpus in the scratchpad (held-out negatives + live positives used ONLY as a
# liveness control; false denies count only on the held-out negatives), v2 from /tmp and V3 from cwd analise, with
# TOURING_CODE_MODE=native. Criterion PRE-REGISTERED (Gabriel 10/10, «(A) contagem BRUTA»): one-sided CP95 <= 1% per
# bucket, `--exigir-limite-superior`. The fixed reference 8fd0efb0 runs too, as the control that the held-out
# discriminates. The instrumented runner writes per-id records, so the V3 layer-2 timeouts ("prazo ... estourado") are
# counted per run: a timeout drops layer-2 findings, which can HIDE false denies — the V3 verdict is read with it.
set -u
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
O=$B/diagnostics/j13
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/efe418c2-95f7-4a62-8bac-57080ed455b8/scratchpad
C=$HOME/.claude/touring/vgp/corpus_holdout_disjunto_20261013f
SELO=$B/diagnostics/holdout-20261013f/selo.json
CTL=$S/j13_hibrido_20261013f
I=$B/diagnostics/instrumentos-2026-10-09
INST=$I/rodar_juiz_instrumentado.py
J=$B/scripts/juiz_motor_vgp.py
HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
NOVO=$HOME/.claude/touring/vgp/binarios/5c7b3d80/touring
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
for bin in 5c7b3d80:$NOVO 8fd0efb0:$REF; do
  n=${bin%%:*}
  t=${bin#*:}
  (cd /tmp && TOURING_CODE_MODE=native "$HW" --class rust --label "j13-$n-v2" --wait 7200 -- \
    python3 -I "$INST" "$J" "$O/${n}_v2.porid.json" --touring "$t" --v5-estrito --corpus "$CTL" --exigir-limite-superior \
    --json "$O/${n}_v2.json" > "$O/${n}_v2.out" 2>&1)
  echo "$n v2 rc=$? $(date -Is)" >> "$R"
done
for bin in 5c7b3d80:$NOVO 8fd0efb0:$REF; do
  n=${bin%%:*}
  t=${bin#*:}
  (cd /home/gabrielgadea/projects/analise && TOURING_CODE_MODE=native "$HW" --class rust --label "j13-$n-v3" --wait 7200 -- \
    python3 -I "$INST" "$J" "$O/${n}_v3.porid.json" --touring "$t" --corpus "$CTL" --v5-estrito \
    --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --exigir-limite-superior --base "$O/${n}_v2.json" \
    --json "$O/${n}_v3.json" > "$O/${n}_v3.out" 2>&1)
  echo "$n V3 rc=$? timeouts_camada2=$(timeouts "$O/${n}_v3.porid.json") $(date -Is)" >> "$R"
done
python3 "$B/scripts/confere_selo.py" --corpus "$C" --selo "$SELO" > "$O/selo_depois.out" 2>&1
echo "selo_depois rc=$?" >> "$R"
date -u +%FT%TZ > "$O/fim_utc.txt"
echo FIM >> "$R"
