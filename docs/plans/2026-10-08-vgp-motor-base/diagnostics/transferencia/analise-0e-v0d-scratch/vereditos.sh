#!/usr/bin/env bash
# Cadeia de vereditos do motor VGP — rodar DEPOIS do build final (target/vgp-judge).
# Cada veredito é um exit code; o log fica em /tmp/v0d/.
set -u
BIN=~/projects/touring/target/vgp-judge/release/touring
cd ~/projects/touring

echo "══ 0) replay do crash (jev_client — stack overflow no baseline)"
timeout 60 "$BIN" vgp check --path /home/gabrielgadea/projects/touring/scripts/jev_client.py \
  --content-file /tmp/v0d/crash1.py --project-root /home/gabrielgadea/projects/touring -j \
  > /tmp/v0d/replay-crash.json 2>&1
echo "replay_rc=$? (esperado 0, verdict json)"

echo "══ 1) aceite (4): pyright-langserver antes/depois de --camadas 1,2"
antes=$(pgrep -fc "pyright-langserver" || echo 0)
printf 'import os\nprint(os.caminho_inventado)\n' > /tmp/v0d/l2-demo.py
timeout 120 "$BIN" vgp check --path /tmp/v0d/l2-demo.py --content-file /tmp/v0d/l2-demo.py \
  --project-root /home/gabrielgadea/projects/touring --camadas 1,2 -j > /tmp/v0d/l2-demo.json 2>&1
echo "l2_rc=$?"
sleep 2
depois=$(pgrep -fc "pyright-langserver" || echo 0)
echo "pyright: antes=$antes depois=$depois (esperado: sem órfão novo persistente — pool idle 900s)"

echo "══ 2) juiz v2 (camada 1) — 704 chamadas"
cd ~/projects/analise
python3 ~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/juiz_motor_vgp.py \
  --touring "$BIN" --json ~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/juiz_v2.json \
  --v5-estrito > /tmp/v0d/juiz-v2-final.log 2>&1
echo "juiz_rc=$?"
tail -8 /tmp/v0d/juiz-v2-final.log
