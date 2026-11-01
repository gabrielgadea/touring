#!/usr/bin/env bash
# V3 a frio (10/10/2026, touring-09): o p95 do V3 > 1000 ms nas medicoes de 09-10/10 foi atribuido a temperatura (92-96 C).
# Controle: o MESMO binario base (e935aaf1) no vivo, a frio, na forma do P2. Se p95 <= 1000 ms a frio, a hipotese ambiental se confirma.
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base; O=$B/diagnostics/v3-frio
T=$HOME/.claude/touring/vgp/binarios/e935aaf1/touring; C=$HOME/.claude/touring/vgp/corpus_v0d
HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
cd /home/gabrielgadea/projects/analise || exit 9
{ date -u +%FT%TZ; $HW --status 2>/dev/null | python3 -c "import json,sys; print('temp_inicio', json.load(sys.stdin)['temp_c'])"; cat /proc/loadavg; } > $O/inicio.txt
$HW --label v3-frio-e935 --wait 3600 -- python3 $B/scripts/juiz_motor_vgp.py --touring $T --corpus $C --v5-estrito --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --json $O/e935_v3_frio.json > $O/e935_v3_frio.out 2>&1; echo "rc=$?" > $O/rc.txt
{ date -u +%FT%TZ; $HW --status 2>/dev/null | python3 -c "import json,sys; print('temp_fim', json.load(sys.stdin)['temp_c'])"; cat /proc/loadavg; } > $O/fim.txt
