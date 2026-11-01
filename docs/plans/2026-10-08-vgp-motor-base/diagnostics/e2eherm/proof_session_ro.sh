#!/bin/bash
# Final exe under the session env INSIDE a read-only mount namespace (real touring state trees ro): any drift seen in the
# snapshots in this run cannot come from the suite's processes — it is ambient (live sessions' hooks/daemons, outside the namespace).
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/e2eherm
exe=$(cat "$S/a1.exe"); H=/home/gabrielgadea
RO=(); for d in "$H/.claude" "$H/.touring" "$H/projects/touring/.claude" "$H/projects/touring/.touring" "$H/projects/touring-wt-e2eherm/.claude"; do [ -e "$d" ] && RO+=(--ro-bind "$d" "$d"); done
cd $H/projects/touring-wt-e2eherm/crates/touring-hooks || exit 1
python3 -I "$S/dbcount.py" > "$S/snap/a1.R1.json"
bwrap --bind / / --dev-bind /dev /dev --proc /proc "${RO[@]}" -- "$exe" --nocapture > "$S/a1.proof.session.ro.log" 2>&1; rc=$?
python3 -I "$S/dbcount.py" > "$S/snap/a1.R2.json"
echo "session-env+ro rc=$rc trees_ro=$((${#RO[@]}/3))"
grep -E "^test |test result|E2EHERM|SKIP" "$S/a1.proof.session.ro.log"
python3 -I "$S/dbcount.py" --diff "$S/snap/a1.R1.json" "$S/snap/a1.R2.json" | python3 -I -c "
import json,sys; d=json.load(sys.stdin)
print('dbs',d['dbs'],'| tables identical',d['tables_identical'],'| tables changed',len(d['tables_changed']),'| files w/ new mtime/size',len(d['files_with_new_mtime_or_size']))
for c in d['tables_changed']: print('  CHANGED',c['db'].replace('/home/gabrielgadea/',''),c['table'],c['before'],'->',c['after'])"
exit $rc
