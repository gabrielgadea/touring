#!/usr/bin/env bash
# Three mutation harnesses in parallel inside ONE hw-slot hold; each owns its MUT_DIR.
SCR=/tmp/claude-1000/-home-gabrielgadea-projects-touring/efe418c2-95f7-4a62-8bac-57080ed455b8/scratchpad
cd /home/gabrielgadea/projects/touring-wt-ecc-py || exit 1
for i in 0 1 2; do
    SHARD=$i NSHARDS=3 MUT_DIR=$SCR/mut_d2_$i PYTHONDONTWRITEBYTECODE=1 python3 $SCR/mutate.py $SCR/spec_d2_shard.py $SCR/mut_d2_$i.json > $SCR/mut_d2_$i.log 2>&1 &
done
wait
echo ALL-SHARDS-DONE > $SCR/mut_d2.done
