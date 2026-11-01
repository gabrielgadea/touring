#!/usr/bin/env bash
# P2 (09/10/2026, touring-09): juiz v2 e V3 do binario congelado e935 no corpus VIVO promovido,
# para comparar por id com k_v2_ev.json / k_v3_ev.json (mesmo conteudo, rodado como staging as 21:14Z).
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base; O=$B/diagnostics/p2-promocao
T=$HOME/.claude/touring/vgp/binarios/e935aaf1/touring; C=$HOME/.claude/touring/vgp/corpus_v0d
HW=/home/gabrielgadea/projects/touring/scripts/hw-slot
cd /home/gabrielgadea/projects/analise || exit 9
date -u +%FT%TZ > $O/inicio_utc.txt; sha256sum $T > $O/binario.sha256; sha256sum $B/scripts/juiz_motor_vgp.py > $O/juiz.sha256
$HW --label p2-v2 --wait 3300 -- python3 $B/scripts/juiz_motor_vgp.py --touring $T --corpus $C --v5-estrito --json $O/k_v2_vivo_p2.json > $O/v2.out 2>&1; echo "v2 rc=$?" >> $O/rcs.txt
$HW --label p2-v3 --wait 3300 -- python3 $B/scripts/juiz_motor_vgp.py --touring $T --corpus $C --v5-estrito --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --json $O/k_v3_vivo_p2.json > $O/v3.out 2>&1; echo "V3 rc=$?" >> $O/rcs.txt
python3 - "$B" "$O" > $O/comparacao.txt 2>&1 <<'PY'
import json, sys
B, O = sys.argv[1], sys.argv[2]
for ref, novo in [("k_v2_ev.json", "k_v2_vivo_p2.json"), ("k_v3_ev.json", "k_v3_vivo_p2.json")]:
    a = json.load(open(f"{B}/diagnostics/juiz-e935/{ref}")); b = json.load(open(f"{O}/{novo}"))
    difs = [k for k in sorted(set(a) | set(b)) if k not in ("p95_ms", "falso_deny_por_balde", "pendencias") and a.get(k) != b.get(k)]
    print(ref, "x", novo, "| pronto", a.get("pronto"), b.get("pronto"), "| chaves divergentes (fora p95/limite/pendencias):", difs or "NENHUMA")
    for k in difs[:6]:
        print("   ", k, "ref=", json.dumps(a.get(k))[:300], "| novo=", json.dumps(b.get(k))[:300])
    print("   pendencias novo:", b.get("pendencias"), "| p95", a.get("p95_ms"), "->", b.get("p95_ms"))
    print("   limite_superior_95 novo:", {k: v.get("limite_superior_95") for k, v in (b.get("falso_deny_por_balde") or {}).items()})
PY
date -u +%FT%TZ > $O/fim_utc.txt
