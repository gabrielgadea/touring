#!/bin/bash
# Final exe under THIS Claude Code session's environment (TOURING_PROJECT_ROOT=main repo, TOURING_DAEMON_SOCKET=global, ...), tight snapshots.
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/e2eherm
exe=$(cat "$S/a1.exe")
cd /home/gabrielgadea/projects/touring-wt-e2eherm/crates/touring-hooks || exit 1
python3 -I "$S/dbcount.py" > "$S/snap/a1.S1.json"
"$exe" --nocapture > "$S/a1.proof.session.log" 2>&1; rc=$?
python3 -I "$S/dbcount.py" > "$S/snap/a1.S2.json"
echo "session-env rc=$rc"
env | grep -E "^(TOURING_PROJECT_ROOT|TOURING_DAEMON_SOCKET|CLAUDE_PROJECT_DIR)=" | sed -E 's/=.*/=<set>/'
grep -E "^test |test result|E2EHERM|SKIP" "$S/a1.proof.session.log"
python3 -I "$S/dbcount.py" --diff "$S/snap/a1.S1.json" "$S/snap/a1.S2.json" | python3 -I -c "
import json,sys; d=json.load(sys.stdin)
print('dbs',d['dbs'],'| tables identical',d['tables_identical'],'| tables changed',len(d['tables_changed']),'| files w/ new mtime/size',len(d['files_with_new_mtime_or_size']))
for c in d['tables_changed']: print('  CHANGED',c['db'].replace('/home/gabrielgadea/',''),c['table'],c['before'],'->',c['after'])"
exit $rc
