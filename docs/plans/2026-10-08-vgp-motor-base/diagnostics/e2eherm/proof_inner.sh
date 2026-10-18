#!/bin/bash
# Runs INSIDE one hw-slot acquisition. Tight windows so ambient writers (other sessions' daemons)
# have as little room as possible to blur the before/after row counts.
#   A: snapshot  -> suite (clean env)            -> B: snapshot      => diff(A,B)  "suite window"
#   C: snapshot  -> suite in read-only mount ns  -> D: snapshot      => diff(C,D)  "ro window"
#   E: snapshot  -> nothing for as long as the suite takes -> F      => diff(E,F)  "ambient control"
# usage: proof_inner.sh <label>
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/e2eherm
label=${1:?label}
exe=$(cat "$S/$label.exe")
H=/home/gabrielgadea
snap() { python3 -I "$S/dbcount.py" > "$S/snap/$label.$1.json"; }
cd "$H/projects/touring-wt-e2eherm/crates/touring-hooks" || exit 1
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-e2eherm
CLEAN=(env -u TOURING_PROJECT_ROOT -u CLAUDE_PROJECT_DIR -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK -u TOURING_SESSION_ID)

echo "### 1. suite window (clean env)"
snap A; t0=$(date +%s.%N)
"${CLEAN[@]}" "$exe" --nocapture > "$S/$label.proof.suite.log" 2>&1; rc1=$?
t1=$(date +%s.%N); snap B
echo "suite rc=$rc1 wall=$(python3 -c "print(round($t1-$t0,2))")s"
grep -E "^test |test result|E2EHERM" "$S/$label.proof.suite.log"
python3 -I "$S/dbcount.py" --diff "$S/snap/$label.A.json" "$S/snap/$label.B.json" | python3 -I -c "
import json,sys; d=json.load(sys.stdin)
print('dbs',d['dbs'],'| tables identical',d['tables_identical'],'| tables changed',len(d['tables_changed']),'| files w/ new mtime/size',len(d['files_with_new_mtime_or_size']))
for c in d['tables_changed']: print('  CHANGED',c['db'].replace('/home/gabrielgadea/',''),c['table'],c['before'],'->',c['after'])
for f in d['files_with_new_mtime_or_size']: print('  MTIME  ',f.replace('/home/gabrielgadea/',''))"

echo "### 2. read-only mount namespace window"
RO=()
for d in "$H/.claude" "$H/.touring" "$H/projects/touring/.claude" "$H/projects/touring/.touring" \
         "$H/projects/touring-wt-e2eherm/.claude" "$H/projects/touring-wt-e2eherm/.touring"; do
  [ -e "$d" ] && RO+=(--ro-bind "$d" "$d")
done
snap C
bwrap --bind / / --dev-bind /dev /dev --proc /proc "${RO[@]}" -- "${CLEAN[@]}" "$exe" --nocapture > "$S/$label.proof.ro.log" 2>&1; rc2=$?
snap D
echo "ro rc=$rc2 trees_ro=$((${#RO[@]}/3))"
grep -E "^test |test result|E2EHERM|Read-only|EROFS" "$S/$label.proof.ro.log"
python3 -I "$S/dbcount.py" --diff "$S/snap/$label.C.json" "$S/snap/$label.D.json" | python3 -I -c "
import json,sys; d=json.load(sys.stdin)
print('dbs',d['dbs'],'| tables identical',d['tables_identical'],'| tables changed',len(d['tables_changed']),'| files w/ new mtime/size',len(d['files_with_new_mtime_or_size']))
for c in d['tables_changed']: print('  CHANGED',c['db'].replace('/home/gabrielgadea/',''),c['table'],c['before'],'->',c['after'])
for f in d['files_with_new_mtime_or_size']: print('  MTIME  ',f.replace('/home/gabrielgadea/',''))"

echo "### 3. ambient control (no suite)"
snap E; sleep 2; snap F
python3 -I "$S/dbcount.py" --diff "$S/snap/$label.E.json" "$S/snap/$label.F.json" | python3 -I -c "
import json,sys; d=json.load(sys.stdin)
print('dbs',d['dbs'],'| tables identical',d['tables_identical'],'| tables changed',len(d['tables_changed']),'| files w/ new mtime/size',len(d['files_with_new_mtime_or_size']))
for c in d['tables_changed']: print('  CHANGED',c['db'].replace('/home/gabrielgadea/',''),c['table'],c['before'],'->',c['after'])"
exit $((rc1 + rc2))
