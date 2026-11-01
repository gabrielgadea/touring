#!/bin/bash
# Apply each mutant, run its test (it must FAIL), restore. One hw-slot acquisition for all.
# Run as:  hw-slot --class rust --wait 7200 --label ecc-hk -- bash mutants.sh
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-p0bd
SP=/tmp/claude-1000/-home-gabrielgadea-projects-touring/efe418c2-95f7-4a62-8bac-57080ed455b8/scratchpad
cd /home/gabrielgadea/projects/touring-wt-ecc-hk || exit 99
: >"$SP/mut_summary.txt"
for id in $(python3 -I "$SP/mut.py" ids); do
  trap 'python3 -I "$SP/mut.py" restore "$id"' EXIT
  if ! python3 -I "$SP/mut.py" apply "$id" >>"$SP/mut_summary.txt" 2>&1; then
    echo "$id APPLY-FAILED" >>"$SP/mut_summary.txt"; continue
  fi
  cmd=$(python3 -I "$SP/mut.py" cmd "$id")
  $cmd >"$SP/mut_$id.log" 2>&1
  rc=$?
  python3 -I "$SP/mut.py" restore "$id" >>"$SP/mut_summary.txt" 2>&1
  failed=$(grep -E "^test .* FAILED$" "$SP/mut_$id.log" | sed 's/^test //; s/ \.\.\. FAILED//' | tr '\n' ' ')
  if [ $rc -ne 0 ]; then verdict=KILLED; else verdict=SURVIVED; fi
  echo "$id rc=$rc $verdict failed=[$failed]" >>"$SP/mut_summary.txt"
done
trap - EXIT
echo done >>"$SP/mut_summary.txt"
