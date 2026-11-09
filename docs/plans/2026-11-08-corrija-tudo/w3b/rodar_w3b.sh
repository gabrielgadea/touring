#!/usr/bin/env bash
# W3b: #8 latency baseline — pre-#8 (208d5cd0) vs HEAD, release, INTERLEAVED rounds.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../../../.." && pwd); WT=${WT:-$ROOT/../touring-wt-w3b}; OUT=$(cd "$(dirname "$0")" && pwd)
ROUNDS=${ROUNDS:-4}; export VGP_LATENCY_N=${VGP_LATENCY_N:-1000}
bin() { # newest test binary of vgp_contencao_replay under $1/target/release/deps
  ls -t "$1"/target/release/deps/vgp_contencao_replay-* 2>/dev/null | grep -v '\.d$' | head -1; }
for w in "$ROOT" "$WT"; do
  (cd "$w" && "$ROOT/scripts/hw-slot" --class rust --wait 7200 -- cargo test --release -p touring-code --test vgp_contencao_replay --no-run) || { echo "build failed in $w"; exit 1; }
done
B_HEAD=$(bin "$ROOT"); B_PRE=$(bin "$WT"); echo "head=$B_HEAD pre=$B_PRE"
[ -x "$B_HEAD" ] && [ -x "$B_PRE" ] || { echo "binary missing"; exit 1; }
: > "$OUT/rodadas.jsonl"
for r in $(seq 1 "$ROUNDS"); do
  for lado in pre head; do
    b=$B_PRE; [ $lado = head ] && b=$B_HEAD
    tmp=$(mktemp -p "$OUT"); 
    VGP_LATENCY_OUT=$tmp "$ROOT/scripts/hw-slot" --class medida --wait 7200 -- "$b" --ignored --exact latencia_do_vgp_na_arvore --test-threads 1 >/dev/null 2>&1 || { echo "run failed $lado $r"; rm -f "$tmp"; exit 1; }
    python3 -c "import json,sys; d=json.loads(open(sys.argv[1]).read().splitlines()[-1]); d['lado']=sys.argv[2]; d['rodada']=int(sys.argv[3]); print(json.dumps(d))" "$tmp" $lado $r >> "$OUT/rodadas.jsonl"; rm -f "$tmp"
  done
done
python3 - "$OUT/rodadas.jsonl" <<'PY'
import json,sys,statistics as st
rs=[json.loads(l) for l in open(sys.argv[1])]
for k in ('overall',):
  for lado in ('pre','head'):
    v=[r[k]['p95_ms'] for r in rs if r['lado']==lado]; print(lado,k,'p95 rodadas',[round(x,4) for x in v],'mediana',round(st.median(v),4))
g=sorted(rs[0]['groups'])
for gr in g:
  m={l:st.median([r['groups'][gr]['p95_ms'] for r in rs if r['lado']==l]) for l in ('pre','head')}
  print(gr,'p95 mediana pre',round(m['pre'],4),'head',round(m['head'],4),'delta_ms',round(m['head']-m['pre'],4))
PY
