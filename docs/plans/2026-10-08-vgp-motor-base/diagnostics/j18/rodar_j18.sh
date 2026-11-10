#!/usr/bin/env bash
# J18 (10/11/2026): production 30.4.108 vs 30.4.104 on sealed held-out 20261110.
# Only PREREGISTRO-J18 §4 changes the J17 instrument: interleaved v2, ABBA V3
# (v3a decides; v3b explains timeouts), and daemon-route warmup/repair.
set -u
B=${HOME}/projects/touring/docs/plans/2026-10-08-vgp-motor-base
O=$B/diagnostics/j18
S=${J18_SCRATCH:-${TMPDIR:-/tmp}/vgp-j18}
C=$HOME/.claude/touring/vgp/corpus_holdout_disjunto_20261110
SELO=$B/diagnostics/holdout-20261110/selo.json
CTL=$S/j18_hibrido_20261110
I=$B/diagnostics/instrumentos-2026-10-09
INST=$I/rodar_juiz_instrumentado.py
J=$B/scripts/juiz_motor_vgp.py
HOT=$O/medir_daemon_quente_j18.py
HW=${HOME}/projects/touring/scripts/hw-slot
TOOLCHAIN=$HOME/.touring/toolchains/30.4.108/bin
NOVO=$HOME/.claude/touring/vgp/binarios/30.4.108/touring
REF=$HOME/.claude/touring/vgp/binarios/30.4.104/touring
R=$O/rcs.txt
[ -e "$R" ] && { echo "ABORTADO: $R ja existe; preserve o run anterior"; exit 4; }
: > "$R"
mkdir -p "$S"
date -u +%FT%TZ > "$O/inicio_utc.txt"
[ "$(sha256sum "$NOVO" | cut -c1-64)" = "$(sha256sum "$TOOLCHAIN/touring" | cut -c1-64)" ] \
  || { echo "ABORTADO: candidato congelado difere do toolchain 30.4.108" >> "$R"; exit 5; }
touring daemon-ctl status > "$O/daemon_status.txt" 2>&1
DAEMON_EXE=$(awk '/^ *exe:/ {print $2; exit}' "$O/daemon_status.txt")
{ [ -n "$DAEMON_EXE" ] && sha256sum "$DAEMON_EXE" "$TOOLCHAIN/touring-daemon"; } > "$O/daemon_identidade.txt" 2>&1
[ "$(cut -c1-64 "$O/daemon_identidade.txt" | sort -u | wc -l)" = 1 ] \
  || { echo "ABORTADO: daemon vivo difere do daemon do candidato (ver daemon_identidade.txt)" >> "$R"; exit 5; }
sha256sum "$NOVO" "$REF" "$J" "$INST" "$I/montar_ctl_generico.py" "$HOT" "$B/scripts/confere_selo.py" \
  "$B/scripts/estado_no_instante_v0d.py" "$HOME/.claude/hooks/touring-hook" > "$O/shas.txt"
"$NOVO" --version > "$O/versao_30_4_108.txt" 2>&1
"$REF" --version > "$O/versao_30_4_104.txt" 2>&1
python3 "$B/scripts/confere_selo.py" --corpus "$C" --selo "$SELO" > "$O/selo_antes.out" 2>&1
echo "selo_antes rc=$?" >> "$R"
grep -q "selo_antes rc=0" "$R" || { echo "ABORTADO: selo violado" >> "$R"; exit 3; }
[ -e "$CTL" ] && { echo "ABORTADO: $CTL ja existe" >> "$R"; exit 4; }
(cd /tmp && python3 -I "$I/montar_ctl_generico.py" "$C" "$CTL") > "$O/hibrido.out" 2>&1
echo "hibrido rc=$?" >> "$R"
grep -q "hibrido rc=0" "$R" || { echo "ABORTADO: hibrido falhou" >> "$R"; exit 4; }
(cd "$CTL" && find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-16) > "$O/hibrido_sha_tree.txt"
timeouts() {
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); d=d.get("por_id", d); n=sum(str(r.get("camada2") or "").startswith("unknown: prazo de") for r in d.values()); print("%d/%d" % (n, len(d)))' "$1" 2> /dev/null || echo ilegivel
}
for bin in 30_4_108:$NOVO 30_4_104:$REF; do
  n=${bin%%:*}
  t=${bin#*:}
  (cd /tmp && TOURING_CODE_MODE=native "$HW" --class rust --label "j18-$n-v2" --wait 7200 -- \
    python3 -I "$INST" "$J" "$O/${n}_v2.porid.json" --touring "$t" --v5-estrito --corpus "$CTL" --exigir-limite-superior \
    --json "$O/${n}_v2.json" > "$O/${n}_v2.out" 2>&1)
  echo "$n v2 rc=$? $(date -Is)" >> "$R"
done
v3() {
  n=$1
  t=$2
  fase=$3
  (cd ${HOME}/projects/analise && TOURING_CODE_MODE=native "$HW" --class rust --label "j18-$n-$fase" --wait 7200 -- \
    python3 -I "$INST" "$J" "$O/${n}_${fase}.porid.json" --touring "$t" --corpus "$CTL" --v5-estrito \
    --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --exigir-limite-superior --base "$O/${n}_v2.json" \
    --json "$O/${n}_${fase}.json" > "$O/${n}_${fase}.out" 2>&1)
  echo "$n $fase rc=$? timeouts_camada2=$(timeouts "$O/${n}_${fase}.porid.json") $(date -Is)" >> "$R"
}
v3 30_4_108 "$NOVO" v3a
v3 30_4_104 "$REF" v3a
v3 30_4_104 "$REF" v3b
v3 30_4_108 "$NOVO" v3b
TOURING_CODE_MODE=native "$HW" --class medida --label j18-hot --wait 7200 -- \
  python3 -I "$HOT" --corpus "$CTL" --saida "$O/hot.json" --por-id "$O/hot.porid.json" --trace "$O/hot.trace.jsonl" > "$O/hot.out" 2>&1
echo "hot rc=$? $(date -Is)" >> "$R"
python3 -I "$O/consolidar_j18.py" "$O" --candidato 30_4_108 --controle 30_4_104 --saida "$O/aceite.json" > "$O/aceite.out" 2>&1
echo "consolidar rc=$? $(date -Is)" >> "$R"
python3 "$B/scripts/confere_selo.py" --corpus "$C" --selo "$SELO" > "$O/selo_depois.out" 2>&1
echo "selo_depois rc=$?" >> "$R"
date -u +%FT%TZ > "$O/fim_utc.txt"
echo FIM >> "$R"
