#!/usr/bin/env bash
# J16 (05/11/2026): judge the installed 30.4.103 binary on the sealed held-out corpus_holdout_disjunto_20261101c
# (local dependencies frozen at the instant through Git, commit 8d166db3), with 30.4.102 as the control.
# Cold runs keep the judge's recall / false-deny / comparative evidence; their p95 is informational only.
# The preregistered latency verdict (PREREGISTRO-V3-QUENTE-2026-10-31.md) comes from the live hot daemon:
# p95 <= 1000 ms, layer-2 timeout/signal fail-closed. The criterion itself is consolidar_j15.avaliar (single source).
set -u
B=${HOME}/projects/touring/docs/plans/2026-10-08-vgp-motor-base
O=$B/diagnostics/j16
S=${J16_SCRATCH:-${TMPDIR:-/tmp}/vgp-j16}
C=$HOME/.claude/touring/vgp/corpus_holdout_disjunto_20261101c
SELO=$B/diagnostics/holdout-20261101c/selo.json
CTL=$S/j16_hibrido_20261101c
I=$B/diagnostics/instrumentos-2026-10-09
INST=$I/rodar_juiz_instrumentado.py
J=$B/scripts/juiz_motor_vgp.py
HOT=$B/diagnostics/j14/medir_daemon_quente.py
HW=${HOME}/projects/touring/scripts/hw-slot
TOOLCHAIN=$HOME/.touring/toolchains/30.4.103/bin
NOVO=$HOME/.claude/touring/vgp/binarios/30.4.103/touring
REF=$HOME/.claude/touring/vgp/binarios/30.4.102/touring
R=$O/rcs.txt
[ -e "$R" ] && { echo "ABORTADO: $R ja existe; preserve o run anterior"; exit 4; }
: > "$R"
mkdir -p "$S"
date -u +%FT%TZ > "$O/inicio_utc.txt"
# The frozen candidate must be byte-identical to the installed toolchain binary.
[ "$(sha256sum "$NOVO" | cut -c1-64)" = "$(sha256sum "$TOOLCHAIN/touring" | cut -c1-64)" ] \
  || { echo "ABORTADO: candidato congelado difere do toolchain 30.4.103" >> "$R"; exit 5; }
# The hot criterion is only attributable to the candidate if the live daemon IS the candidate's daemon (read-only check).
touring daemon-ctl status > "$O/daemon_status.txt" 2>&1
DAEMON_EXE=$(awk '/^ *exe:/ {print $2; exit}' "$O/daemon_status.txt")
{ [ -n "$DAEMON_EXE" ] && sha256sum "$DAEMON_EXE" "$TOOLCHAIN/touring-daemon"; } > "$O/daemon_identidade.txt" 2>&1
[ "$(cut -c1-64 "$O/daemon_identidade.txt" | sort -u | wc -l)" = 1 ] \
  || { echo "ABORTADO: daemon vivo difere do daemon do candidato (ver daemon_identidade.txt)" >> "$R"; exit 5; }
sha256sum "$NOVO" "$REF" "$J" "$INST" "$I/montar_ctl_generico.py" "$HOT" "$B/scripts/confere_selo.py" \
  "$B/scripts/estado_no_instante_v0d.py" "$HOME/.claude/hooks/touring-hook" > "$O/shas.txt"
"$NOVO" --version > "$O/versao_30_4_103.txt" 2>&1
"$REF" --version > "$O/versao_30_4_102.txt" 2>&1
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
for bin in 30_4_103:$NOVO 30_4_102:$REF; do
  n=${bin%%:*}
  t=${bin#*:}
  (cd /tmp && TOURING_CODE_MODE=native "$HW" --class rust --label "j16-$n-v2" --wait 7200 -- \
    python3 -I "$INST" "$J" "$O/${n}_v2.porid.json" --touring "$t" --v5-estrito --corpus "$CTL" --exigir-limite-superior \
    --json "$O/${n}_v2.json" > "$O/${n}_v2.out" 2>&1)
  echo "$n v2 rc=$? $(date -Is)" >> "$R"
done
for bin in 30_4_103:$NOVO 30_4_102:$REF; do
  n=${bin%%:*}
  t=${bin#*:}
  (cd ${HOME}/projects/analise && TOURING_CODE_MODE=native "$HW" --class rust --label "j16-$n-v3" --wait 7200 -- \
    python3 -I "$INST" "$J" "$O/${n}_v3.porid.json" --touring "$t" --corpus "$CTL" --v5-estrito \
    --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --exigir-limite-superior --base "$O/${n}_v2.json" \
    --json "$O/${n}_v3.json" > "$O/${n}_v3.out" 2>&1)
  echo "$n V3 rc=$? timeouts_camada2=$(timeouts "$O/${n}_v3.porid.json") $(date -Is)" >> "$R"
done
TOURING_CODE_MODE=native "$HW" --class medida --label j16-hot --wait 7200 -- \
  python3 -I "$HOT" --corpus "$CTL" --saida "$O/hot.json" --por-id "$O/hot.porid.json" --trace "$O/hot.trace.jsonl" > "$O/hot.out" 2>&1
echo "hot rc=$? $(date -Is)" >> "$R"
python3 -I "$O/consolidar_j16.py" "$O" --candidato 30_4_103 --saida "$O/aceite.json" > "$O/aceite.out" 2>&1
echo "consolidar rc=$? $(date -Is)" >> "$R"
python3 "$B/scripts/confere_selo.py" --corpus "$C" --selo "$SELO" > "$O/selo_depois.out" 2>&1
echo "selo_depois rc=$?" >> "$R"
date -u +%FT%TZ > "$O/fim_utc.txt"
echo FIM >> "$R"
